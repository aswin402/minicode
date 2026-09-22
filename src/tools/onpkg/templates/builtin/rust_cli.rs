#![allow(clippy::all, unused)]

use crate::tools::onpkg::stacks::{Stack, StackFile};

pub fn rust_cli() -> Stack {
    Stack {
        hooks: vec![],
        name: "rust-cli".into(),
        runtime: "cargo".into(),
        description: "Minimal Rust CLI application with clap derive and anyhow".into(),
        packages: vec!["clap".into(), "anyhow".into()],
        dev_packages: vec![],
        transitive_packages: vec![],
        files: vec![
            StackFile {
                path: "Cargo.toml".into(),
                content: r##"[package]
name = "rust-cli"
version = "0.1.0"
edition = "2021"

[dependencies]
clap = { version = "4.5", features = ["derive"] }
anyhow = "1.0"
"##
                .into(),
                binary_content: None,
            },
            StackFile {
                path: ".cargo/config.toml".into(),
                content: r##"[build]
# Limit parallel compilation jobs to conserve CPU and RAM
jobs = 2

[target.'cfg(target_os = "linux")']
# Limit the number of threads used by the linker to reduce peak RAM usage on Linux
rustflags = ["-C", "link-arg=-Wl,--threads=2"]
"##
                .into(),
                binary_content: None,
            },
            StackFile {
                path: "src/main.rs".into(),
                content: r##"use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "rust-cli", version = "0.1.0", about = "Minimal Rust CLI created with minicode")]
struct Args {
    /// Optional name argument
    #[arg(short, long)]
    name: Option<String>,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let name = args.name.unwrap_or_else(|| "World".to_string());
    println!("Hello, {}!", name);
    Ok(())
}
"##
                .into(),
                binary_content: None,
            },
            StackFile {
                path: "README.md".into(),
                content: r##"# Rust CLI Starter

Scaffolded with minicode + MiniKit.

## Usage

```bash
cargo run -- --name Developer
```
"##
                .into(),
                binary_content: None,
            },
        ],
    }
}
