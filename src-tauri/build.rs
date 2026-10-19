use clap::CommandFactory;
use clap_complete::{Shell, generate_to};

#[path = "src/cli.rs"]
#[allow(dead_code)]
mod cli;

fn main() {
    // Shell completions for the packages (see bundle.linux.*.files in tauri.conf.json).
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/completions");
    std::fs::create_dir_all(&dir).expect("completions dir");
    let mut cmd = cli::Cli::command();
    for shell in [Shell::Bash, Shell::Zsh, Shell::Fish] {
        generate_to(shell, &mut cmd, "nbsp", &dir).expect("generate completions");
    }
    println!("cargo:rerun-if-changed=src/cli.rs");

    tauri_build::build()
}
