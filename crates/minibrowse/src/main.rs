//! `minibrowse` CLI Entrypoint — Low-RAM Agentic Browser Daemon for `minicode`.

use minibrowse::ffi::tune_allocator_env;
use minibrowse::server::MiniBrowseServer;
use minibrowse::webview::{spawn_browser_backend, LaunchOptions};
use std::path::PathBuf;

struct CliArgs {
    port: u16,
    headless: bool,
    width: u32,
    height: u32,
    initial_url: Option<String>,
    adblock: bool,
    watch_dart_dir: Option<PathBuf>,
    vm_url: Option<String>,
}

fn parse_cli_args() -> Result<Option<CliArgs>, String> {
    let mut port: u16 = 9222;
    let mut headless = true;
    let mut width: u32 = 1440;
    let mut height: u32 = 900;
    let mut initial_url: Option<String> = None;
    let mut adblock = true;
    let mut watch_dart_dir: Option<PathBuf> = None;
    let mut vm_url: Option<String> = None;

    let mut args = std::env::args().skip(1).peekable();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-V" | "--version" | "version" => {
                println!("minibrowse v{}", env!("CARGO_PKG_VERSION"));
                return Ok(None);
            }
            "-h" | "--help" | "help" => {
                print_help();
                return Ok(None);
            }
            "serve" => {}
            "--headless" => headless = true,
            "--gui" | "--headed" | "--window" => headless = false,
            "--adblock" => adblock = true,
            "--no-adblock" => adblock = false,
            "--port" | "-p" => {
                let raw = args
                    .next()
                    .ok_or_else(|| "Missing value for --port".to_string())?;
                port = raw
                    .parse::<u16>()
                    .map_err(|_| format!("Invalid port number: '{raw}'"))?;
            }
            "--width" => {
                let raw = args
                    .next()
                    .ok_or_else(|| "Missing value for --width".to_string())?;
                width = raw
                    .parse::<u32>()
                    .map_err(|_| format!("Invalid width: '{raw}'"))?;
            }
            "--height" => {
                let raw = args
                    .next()
                    .ok_or_else(|| "Missing value for --height".to_string())?;
                height = raw
                    .parse::<u32>()
                    .map_err(|_| format!("Invalid height: '{raw}'"))?;
            }
            "--url" => {
                initial_url = args.next();
            }
            "--watch-dart" => {
                let raw = args
                    .next()
                    .ok_or_else(|| "Missing directory path for --watch-dart".to_string())?;
                watch_dart_dir = Some(PathBuf::from(raw));
            }
            "--vm-url" => {
                vm_url = args.next();
            }
            other => {
                if let Some(val) = other.strip_prefix("--port=") {
                    port = val
                        .parse::<u16>()
                        .map_err(|_| format!("Invalid port: '{val}'"))?;
                } else if let Some(val) = other.strip_prefix("--width=") {
                    width = val
                        .parse::<u32>()
                        .map_err(|_| format!("Invalid width: '{val}'"))?;
                } else if let Some(val) = other.strip_prefix("--height=") {
                    height = val
                        .parse::<u32>()
                        .map_err(|_| format!("Invalid height: '{val}'"))?;
                } else if let Some(val) = other.strip_prefix("--url=") {
                    initial_url = Some(val.to_string());
                } else if other.starts_with("http://") || other.starts_with("https://") {
                    initial_url = Some(other.to_string());
                }
            }
        }
    }

    Ok(Some(CliArgs {
        port,
        headless,
        width,
        height,
        initial_url,
        adblock,
        watch_dart_dir,
        vm_url,
    }))
}

fn print_help() {
    println!(
        "minibrowse v{} — Agentic Browser, Research Extractor, Web & Flutter Debugger for minicode\n\n\
         USAGE:\n  \
           minibrowse serve [OPTIONS]\n\n\
         OPTIONS:\n  \
           -p, --port <PORT>         CDP + MiniBrowse WebSocket/HTTP port (default: 9222)\n  \
               --headless            Offscreen headless rendering with zero window popups (default)\n  \
               --gui                 Visible chromeless window for live developer viewing\n  \
               --width <PX>          Initial viewport width (default: 1440)\n  \
               --height <PX>         Initial viewport height (default: 900)\n  \
               --url <URL>           Optional initial URL to load on startup\n  \
               --adblock             Block ad/tracker/analytics domains (default: on)\n  \
               --no-adblock          Disable built-in tracker blocking\n  \
               --watch-dart <DIR>    Watch .dart files via Linux inotify and auto-hot-reload\n  \
               --vm-url <URI>        Dart VM Service URL for instant reloadSources hot-reload\n  \
           -V, --version             Print version\n  \
           -h, --help                Print help",
        env!("CARGO_PKG_VERSION")
    );
}

fn main() -> anyhow::Result<()> {
    let Some(cli) = parse_cli_args().map_err(anyhow::Error::msg)? else {
        return Ok(());
    };

    // Apply glibc arena/trim + JavaScriptCore MiniVM heap & JIT bounds BEFORE spawning threads
    tune_allocator_env(cli.headless);

    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;

    rt.block_on(async move {
        let engine = spawn_browser_backend(LaunchOptions {
            headless: cli.headless,
            width: cli.width,
            height: cli.height,
            initial_url: cli.initial_url,
            adblock: cli.adblock,
        })
        .map_err(anyhow::Error::msg)?;

        #[cfg(target_os = "linux")]
        if let Some(watch_dir) = cli.watch_dart_dir {
            minibrowse::flutter_vm::spawn_dart_inotify_watcher(
                watch_dir,
                cli.vm_url,
                engine.clone(),
            );
        }

        let server = MiniBrowseServer::bind(cli.port, engine, cli.width, cli.height)
            .await
            .map_err(anyhow::Error::msg)?;
        server.run().await.map_err(anyhow::Error::msg)
    })
}
