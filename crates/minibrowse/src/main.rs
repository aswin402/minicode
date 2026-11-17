//! `minibrowse` CLI Entrypoint — Low-RAM Agentic Browser Daemon for `minicode`.

use minibrowse::fallback::PureRustPageState;
use minibrowse::ffi::tune_allocator_env_with_webgl;
use minibrowse::server::MiniBrowseServer;
use minibrowse::webview::{spawn_browser_backend, BrowserBackendCmd, LaunchOptions};
use std::path::PathBuf;

enum Subcommand {
    Serve(CliArgs),
    Fetch(FetchArgs),
}

struct CliArgs {
    port: u16,
    headless: bool,
    webgl: bool,
    width: u32,
    height: u32,
    initial_url: Option<String>,
    adblock: bool,
    watch_dart_dir: Option<PathBuf>,
    vm_url: Option<String>,
}

struct FetchArgs {
    url: String,
    format: String,
    query: Option<String>,
    max_chars: usize,
    use_js: bool,
    adblock: bool,
}

fn parse_cli_args() -> Result<Option<Subcommand>, String> {
    let mut is_fetch = false;
    let mut fetch_format = "markdown".to_string();
    let mut fetch_query: Option<String> = None;
    let mut fetch_max_chars: usize = 16_000;
    let mut fetch_js = false;

    let mut port: u16 = 9222;
    let mut headless = true;
    let mut webgl = std::env::var("MINIBROWSE_ENABLE_WEBGL")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    let mut width_opt: Option<u32> = None;
    let mut height_opt: Option<u32> = None;
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
            "fetch" | "dump" => {
                is_fetch = true;
            }
            "--headless" => headless = true,
            "--gui" | "--headed" | "--window" => headless = false,
            "--webgl" => webgl = true,
            "--no-webgl" => webgl = false,
            "--js" => fetch_js = true,
            "--adblock" => adblock = true,
            "--no-adblock" => adblock = false,
            "--format" | "-f" => {
                fetch_format = args
                    .next()
                    .ok_or_else(|| "Missing value for --format".to_string())?;
            }
            "--query" | "-q" => {
                fetch_query = args.next();
            }
            "--max-chars" => {
                let raw = args
                    .next()
                    .ok_or_else(|| "Missing value for --max-chars".to_string())?;
                fetch_max_chars = raw
                    .parse::<usize>()
                    .map_err(|_| format!("Invalid --max-chars: '{raw}'"))?;
            }
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
                width_opt = Some(
                    raw.parse::<u32>()
                        .map_err(|_| format!("Invalid width: '{raw}'"))?,
                );
            }
            "--height" => {
                let raw = args
                    .next()
                    .ok_or_else(|| "Missing value for --height".to_string())?;
                height_opt = Some(
                    raw.parse::<u32>()
                        .map_err(|_| format!("Invalid height: '{raw}'"))?,
                );
            }
            "--max-memory-mb" => {
                let raw = args
                    .next()
                    .ok_or_else(|| "Missing value for --max-memory-mb".to_string())?;
                let mb = raw
                    .parse::<u64>()
                    .map_err(|_| format!("Invalid --max-memory-mb: '{raw}'"))?;
                std::env::set_var("MINIBROWSE_MAX_MEMORY_MB", mb.to_string());
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
                } else if let Some(val) = other.strip_prefix("--format=") {
                    fetch_format = val.to_string();
                } else if let Some(val) = other.strip_prefix("--query=") {
                    fetch_query = Some(val.to_string());
                } else if let Some(val) = other.strip_prefix("--max-chars=") {
                    fetch_max_chars = val
                        .parse::<usize>()
                        .map_err(|_| format!("Invalid --max-chars: '{val}'"))?;
                } else if let Some(val) = other.strip_prefix("--width=") {
                    width_opt = Some(
                        val.parse::<u32>()
                            .map_err(|_| format!("Invalid width: '{val}'"))?,
                    );
                } else if let Some(val) = other.strip_prefix("--height=") {
                    height_opt = Some(
                        val.parse::<u32>()
                            .map_err(|_| format!("Invalid height: '{val}'"))?,
                    );
                } else if let Some(val) = other.strip_prefix("--max-memory-mb=") {
                    let mb = val
                        .parse::<u64>()
                        .map_err(|_| format!("Invalid --max-memory-mb: '{val}'"))?;
                    std::env::set_var("MINIBROWSE_MAX_MEMORY_MB", mb.to_string());
                } else if let Some(val) = other.strip_prefix("--url=") {
                    initial_url = Some(val.to_string());
                } else if other.starts_with("http://")
                    || other.starts_with("https://")
                    || other.starts_with("file://")
                    || other.starts_with("data:")
                {
                    initial_url = Some(other.to_string());
                }
            }
        }
    }

    if is_fetch {
        let url = initial_url.ok_or_else(|| {
            "Usage: minibrowse fetch <URL> [--format markdown|semantic|tables|links|html|json] [--js]"
                .to_string()
        })?;
        return Ok(Some(Subcommand::Fetch(FetchArgs {
            url,
            format: fetch_format,
            query: fetch_query,
            max_chars: fetch_max_chars,
            use_js: fetch_js,
            adblock,
        })));
    }

    let width = width_opt.unwrap_or(if headless { 1440 } else { 1280 });
    let height = height_opt.unwrap_or(if headless { 900 } else { 800 });

    Ok(Some(Subcommand::Serve(CliArgs {
        port,
        headless,
        webgl,
        width,
        height,
        initial_url,
        adblock,
        watch_dart_dir,
        vm_url,
    })))
}

fn print_help() {
    println!(
        "minibrowse v{} — Agentic Browser, Research Extractor, Web & Flutter Debugger for minicode\n\n\
         USAGE:\n  \
           minibrowse serve [OPTIONS]\n  \
           minibrowse fetch <URL> [--format markdown|semantic|tables|links|html|json] [--query <Q>] [--js]\n\n\
         OPTIONS:\n  \
           -p, --port <PORT>         CDP + MiniBrowse WebSocket/HTTP port (default: 9222)\n  \
               --headless            Offscreen headless rendering with zero window popups (default)\n  \
               --gui                 Visible window in Zero-LLVM 2D Cairo diet (~77 MB PSS; ~32 MB idle frozen)\n  \
               --webgl               Enable GPU EGL/WebGL hardware stack (off by default to save ~80 MB PSS)\n  \
               --width <PX>          Initial viewport width (default: 1440 headless / 1280 gui)\n  \
               --height <PX>         Initial viewport height (default: 900 headless / 800 gui)\n  \
               --url <URL>           Optional initial URL to load on startup\n  \
               --format <FMT>        Output format for `fetch`: markdown, semantic, tables, links, html, json\n  \
               --query <TEXT>        Relevance filter query for `fetch`\n  \
               --js                  Allow temporary Gear-1 WebKit JS hydration during `fetch`\n  \
               --adblock             Block ad/tracker/analytics domains (default: on)\n  \
               --no-adblock          Disable built-in tracker blocking\n  \
               --watch-dart <DIR>    Watch .dart files via Linux inotify and auto-hot-reload\n  \
               --vm-url <URI>        Dart VM Service URL for instant reloadSources hot-reload\n  \
           -V, --version             Print version\n  \
           -h, --help                Print help",
        env!("CARGO_PKG_VERSION")
    );
}

async fn run_stateless_fetch(args: FetchArgs) -> Result<(), String> {
    let fmt = args.format.trim().to_ascii_lowercase();
    if !args.use_js {
        let mut page = PureRustPageState::new(1280, 800, args.adblock);
        let _ = page.navigate(&args.url).await?;
        match fmt.as_str() {
            "html" => println!("{}", page.html),
            "semantic" | "semantic_tree" | "tree" => {
                println!("{}", page.extract_semantic_tree(args.max_chars));
            }
            "tables" => {
                let tbl = page.extract_tables(None);
                println!(
                    "{}",
                    serde_json::to_string_pretty(&tbl).unwrap_or_else(|_| tbl.to_string())
                );
            }
            "links" | "citations" => {
                let res = page.extract_research(args.query.as_deref(), args.max_chars);
                let links = res
                    .get("citations")
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!([]));
                println!(
                    "{}",
                    serde_json::to_string_pretty(&links).unwrap_or_else(|_| links.to_string())
                );
            }
            "json" => {
                let res = page.extract_research(args.query.as_deref(), args.max_chars);
                println!(
                    "{}",
                    serde_json::to_string_pretty(&res).unwrap_or_else(|_| res.to_string())
                );
            }
            _ => {
                let res = page.extract_research(args.query.as_deref(), args.max_chars);
                println!(
                    "{}",
                    res.get("markdown").and_then(|v| v.as_str()).unwrap_or("")
                );
            }
        }
        return Ok(());
    }

    // Optional `--js` mode: hydrate via temporary headless supervisor and exit immediately freeing 100% RAM
    let engine = spawn_browser_backend(LaunchOptions {
        headless: true,
        width: 1280,
        height: 800,
        initial_url: None,
        adblock: args.adblock,
        webgl: false,
    })?;
    let (ntx, nrx) = tokio::sync::oneshot::channel();
    engine.send(BrowserBackendCmd::Navigate {
        url: args.url,
        reply: ntx,
    })?;
    let _ = tokio::time::timeout(std::time::Duration::from_secs(18), nrx).await;

    let (rtx, rrx) = tokio::sync::oneshot::channel();
    let req_fmt = if matches!(fmt.as_str(), "semantic" | "semantic_tree" | "tree") {
        Some("semantic_tree".to_string())
    } else {
        None
    };
    engine.send(BrowserBackendCmd::ResearchExtract {
        query: args.query,
        max_chars: args.max_chars,
        format: req_fmt,
        reply: rtx,
    })?;
    if let Ok(Ok(Ok(val))) = tokio::time::timeout(std::time::Duration::from_secs(10), rrx).await {
        if fmt == "json" {
            println!(
                "{}",
                serde_json::to_string_pretty(&val).unwrap_or_else(|_| val.to_string())
            );
        } else {
            println!(
                "{}",
                val.get("markdown").and_then(|v| v.as_str()).unwrap_or("")
            );
        }
    }
    let _ = engine.send(BrowserBackendCmd::Shutdown);
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let Some(sub) = parse_cli_args().map_err(anyhow::Error::msg)? else {
        return Ok(());
    };

    match sub {
        Subcommand::Fetch(fetch_args) => {
            tune_allocator_env_with_webgl(true, false);
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?;
            rt.block_on(run_stateless_fetch(fetch_args))
                .map_err(anyhow::Error::msg)
        }
        Subcommand::Serve(cli) => {
            // Apply glibc arena/trim + Zero-LLVM 2D surface / JSC bounds BEFORE spawning threads
            tune_allocator_env_with_webgl(cli.headless, cli.webgl);

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
                    webgl: cli.webgl,
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
    }
}
