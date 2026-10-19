//! CLI commands that run without the GUI.

use std::io::Write;
use std::path::{Path, PathBuf};

use clap::CommandFactory;

use crate::cli::{Cli, Command, ScanArgs, ScanMode, ScanSize};
use crate::engine::{Cell, Engine, Sheet};
use crate::{cups, maintenance, scan};

/// Runs a terminal-only command; `None` means the GUI should start.
pub fn run(cli: &Cli) -> Option<i32> {
    let result = match &cli.command {
        Some(Command::Completions { shell }) => {
            clap_complete::generate(*shell, &mut Cli::command(), "nbsp", &mut std::io::stdout());
            Ok(())
        }
        Some(Command::Printers) => block(printers()),
        Some(Command::Status { printer }) => block(status(printer.clone())),
        Some(Command::Scan(args)) if args.output.is_some() => block(scan_to_file(args)),
        _ => return None,
    };
    Some(match result {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("nbsp: {e}");
            1
        }
    })
}

fn block<F: std::future::Future<Output = Result<(), String>>>(f: F) -> Result<(), String> {
    tauri::async_runtime::block_on(f)
}

async fn default_printer() -> Result<String, String> {
    let list = cups::printers().await?;
    list.iter()
        .find(|p| p.is_default)
        .or_else(|| list.iter().find(|p| p.name != "NBSP"))
        .map(|p| p.name.clone())
        .ok_or_else(|| "no printers configured".into())
}

async fn printers() -> Result<(), String> {
    for p in cups::printers().await? {
        println!(
            "{}{}\t{}",
            p.name,
            if p.is_default { " (default)" } else { "" },
            p.state
        );
    }
    Ok(())
}

async fn status(printer: Option<String>) -> Result<(), String> {
    let name = match printer {
        Some(p) => p,
        None => default_printer().await?,
    };
    let info = cups::info(&name).await?;
    let device = maintenance::status(&info.model).await;
    println!("{}", info.name);
    println!("  Model       {}", info.model);
    println!(
        "  Connection  {}",
        info.uri.split('?').next().unwrap_or("").replace("%20", " ")
    );
    println!(
        "  Queue       {}{}",
        info.state,
        if info.accepting {
            ""
        } else {
            ", not accepting jobs"
        }
    );
    if device.available {
        println!("  Printer     {}", device.status);
    }
    if !info.reasons.is_empty() {
        println!("  Alerts      {}", info.reasons.join(", "));
    }
    if info.jobs.is_empty() {
        println!("  Jobs        none");
    } else {
        for j in &info.jobs {
            println!("  Job         {}  {} KB  {}", j.id, j.size / 1024, j.time);
        }
    }
    Ok(())
}

/// pdfium location: the installed package's resources, else the dev checkout.
pub fn pdfium_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![PathBuf::from("/usr/lib/NBSP/lib")];
    if let Ok(exe) = std::env::current_exe()
        && let Some(bin) = exe.parent()
    {
        dirs.push(bin.join("../lib/NBSP/lib"));
        // Inside a macOS .app: Contents/MacOS/nbsp next to Contents/Resources/lib.
        dirs.push(bin.join("../Resources/lib"));
        dirs.push(bin.join("lib"));
    }
    dirs.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("lib"));
    dirs
}

async fn scan_to_file(args: &ScanArgs) -> Result<(), String> {
    let out = args.output.clone().unwrap();
    let ext = out
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !matches!(ext.as_str(), "pdf" | "png" | "jpg" | "jpeg") {
        return Err("output must end in .pdf, .png or .jpg".into());
    }
    let device = match &args.device {
        Some(d) => d.clone(),
        None => {
            scan::devices()
                .await?
                .into_iter()
                .next()
                .ok_or("no scanner found")?
                .id
        }
    };
    let [w, h] = match args.size {
        ScanSize::Max => scan::glass(&device).await,
        size => size.mm(),
    };
    let opts = scan::ScanOptions {
        device,
        mode: match args.mode {
            ScanMode::Color => "Color",
            ScanMode::Gray => "Grayscale",
            ScanMode::Bw => "Monochrome",
        }
        .into(),
        dpi: args.dpi,
        area: [0.0, 0.0, w, h],
        brightness: 0,
        contrast: 0,
    };
    let tmp = std::env::temp_dir().join(format!("nbsp-scan-{}.png", std::process::id()));
    let (_cancel_tx, cancel_rx) = tokio::sync::oneshot::channel();
    eprint!("Scanning…");
    let res = scan::scan(
        |p| {
            eprint!("\rScanning… {p:.0}%  ");
            let _ = std::io::stderr().flush();
        },
        &opts,
        &tmp,
        false,
        cancel_rx,
    )
    .await;
    eprintln!();
    res?;
    let written = write_scan(&tmp, &out, &ext, &opts);
    let _ = std::fs::remove_file(&tmp);
    let _ = std::fs::remove_file(tmp.with_extension("jpg"));
    written?;
    eprintln!("Saved {}", out.display());
    Ok(())
}

fn write_scan(png: &Path, out: &Path, ext: &str, opts: &scan::ScanOptions) -> Result<(), String> {
    let dpi = opts.dpi as f32;
    match ext {
        "png" => std::fs::copy(png, out)
            .map(|_| ())
            .map_err(|e| e.to_string()),
        "jpg" | "jpeg" => image::open(png)
            .map_err(|e| e.to_string())?
            .to_rgb8()
            .save(out)
            .map_err(|e| e.to_string()),
        _ => {
            let engine = Engine::start(pdfium_dirs())?;
            let (tx, rx) = std::sync::mpsc::channel();
            let (png, out) = (scan::finish(png, opts)?, out.to_path_buf());
            engine.send(move |ctx| {
                let res = (|| {
                    let info = ctx.open_image(&png, "scan".into(), Some(dpi))?;
                    let [w, h] = info.pages[0];
                    let cell = Cell {
                        src: info.id,
                        index: 0,
                        rot: 0,
                        x: 0.0,
                        y: 0.0,
                        w,
                        h,
                        border: false,
                    };
                    let bytes = ctx.compose(&[Sheet {
                        w,
                        h,
                        cells: vec![cell],
                        lines: vec![],
                        labels: vec![],
                    }])?;
                    std::fs::write(&out, bytes).map_err(|e| e.to_string())
                })();
                let _ = tx.send(res);
            });
            rx.recv().map_err(|e| e.to_string())?
        }
    }
}
