//! Turns the GUI-side CLI commands (open, print, scan dialog) into a request the frontend applies.

use std::path::{Path, PathBuf};

use clap::ValueEnum;
use serde::Serialize;
use serde_json::{Map, Value, json};

use crate::cli::{Cli, Command, Duplex, Media, Orientation, PrintArgs, Quality, Scale};

#[derive(Serialize, Default, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Launch {
    pub files: Vec<String>,
    /// Virtual printer jobs (deleted once opened).
    pub inbox: Vec<String>,
    /// Print as soon as the files are loaded.
    pub print: bool,
    /// Open the scan dialog.
    pub scan: bool,
    /// Close the window when printing is done (only for a fresh `nbsp print`).
    pub quit_after: bool,
    pub preset: Option<String>,
    pub printer: Option<String>,
    /// Partial PrintSettings (frontend field names).
    pub settings: Map<String, Value>,
    /// PPD options, e.g. PrintQuality=High.
    pub ppd: Map<String, Value>,
}

impl Launch {
    pub fn is_empty(&self) -> bool {
        self.files.is_empty() && self.inbox.is_empty() && !self.print && !self.scan
    }
}

/// Resolves CLI paths (relative to the caller's cwd, `file://` URLs from desktop launchers).
pub fn resolve(paths: &[PathBuf], cwd: &Path) -> Vec<String> {
    paths
        .iter()
        .filter_map(|p| {
            let s = p.to_string_lossy();
            let p = match s.strip_prefix("file://") {
                Some(rest) => PathBuf::from(percent_decode(rest)),
                None => p.clone(),
            };
            let p = if p.is_absolute() { p } else { cwd.join(p) };
            if p.is_file() {
                Some(p.to_string_lossy().into_owned())
            } else {
                eprintln!("nbsp: not a file: {}", p.display());
                None
            }
        })
        .collect()
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(b) = u8::from_str_radix(&s[i + 1..i + 3], 16)
        {
            out.push(b);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub fn from_cli(cli: &Cli, cwd: &Path, fresh: bool) -> Launch {
    match &cli.command {
        Some(Command::Print(args)) => print(args, cwd, fresh),
        Some(Command::Scan(_)) => Launch {
            scan: true,
            ..Default::default()
        },
        _ => Launch {
            files: resolve(&cli.files, cwd),
            ..Default::default()
        },
    }
}

fn print(a: &PrintArgs, cwd: &Path, fresh: bool) -> Launch {
    let mut s = Map::new();
    let mut ppd = Map::new();
    if let Some(n) = a.copies {
        s.insert("copies".into(), json!(n));
    }
    match a.pages.as_deref() {
        Some("odd") => {
            s.insert("subset".into(), json!("odd"));
        }
        Some("even") => {
            s.insert("subset".into(), json!("even"));
        }
        Some(range) => {
            s.insert("subset".into(), json!("custom"));
            s.insert("range".into(), json!(range));
        }
        None => {}
    }
    if let Some(d) = a.duplex {
        let v = match d {
            Duplex::Off => "off",
            Duplex::Long => "long",
            Duplex::Short => "short",
        };
        s.insert("duplex".into(), json!(v));
    }
    if let Some(n) = &a.per_sheet {
        s.insert("perSheet".into(), json!(n.parse::<u32>().unwrap_or(1)));
        s.insert("mode".into(), json!("normal"));
    }
    if a.booklet {
        s.insert("mode".into(), json!("booklet"));
    }
    if let Some(p) = &a.paper {
        s.insert("paperId".into(), json!(p));
    }
    if let Some((w, h)) = a.paper_size.as_deref().and_then(parse_size) {
        s.insert("paperId".into(), json!(format!("Custom.{w}x{h}mm")));
        s.insert("paper".into(), json!({ "w": w.min(h), "h": w.max(h) }));
    }
    if let Some(o) = a.orientation {
        let v = match o {
            Orientation::Auto => "auto",
            Orientation::Portrait => "portrait",
            Orientation::Landscape => "landscape",
        };
        s.insert("orientation".into(), json!(v));
    }
    if let Some(sc) = a.scale {
        let v = match sc {
            Scale::Fit => "fit",
            Scale::Shrink => "shrink",
            Scale::Actual => "actual",
            Scale::Custom => "custom",
        };
        s.insert("scale".into(), json!(v));
    }
    if let Some(pct) = a.scale_pct {
        s.insert("scale".into(), json!("custom"));
        s.insert("scalePct".into(), json!(pct));
    }
    if let Some(m) = a.margins {
        s.insert(
            "margins".into(),
            json!({ "top": m, "right": m, "bottom": m, "left": m }),
        );
    }
    if let Some(format) = &a.page_numbers {
        let mut n = json!({ "enabled": true, "format": format });
        if let Some(pos) = a.number_position {
            n["position"] = json!(pos.to_possible_value().map(|v| v.get_name().to_string()));
        }
        s.insert("pageNumbers".into(), n);
    }
    if a.no_reverse {
        s.insert("firstOnTop".into(), json!(false));
    }
    if let Some(q) = a.quality {
        let v = match q {
            Quality::Draft => "Draft",
            Quality::Standard => "Standard",
            Quality::High => "High",
        };
        ppd.insert("PrintQuality".into(), json!(v));
    }
    if a.gray {
        ppd.insert("Color".into(), json!("Grayscale"));
    }
    if let Some(m) = a.media {
        let v = match m {
            Media::Plain => "PLAIN",
            Media::PhotoQuality => "EPQIJ",
            Media::Matte => "EMATTE",
            Media::Glossy => "EPSGLOS",
            Media::PremiumGlossy => "EPREMGLOSS",
            Media::UltraGlossy => "EULTRAGLOSSY",
            Media::PhotoGlossy => "PHOTOGLOSSY",
            Media::Envelope => "ENVELOPE",
        };
        ppd.insert("MediaType".into(), json!(v));
    }
    Launch {
        files: resolve(&a.files, cwd),
        print: !a.review,
        quit_after: fresh && !a.review,
        preset: a.preset.clone(),
        printer: a.printer.clone(),
        settings: s,
        ppd,
        ..Default::default()
    }
}

/// "100x150" (mm) → (100, 150).
fn parse_size(s: &str) -> Option<(f32, f32)> {
    let (w, h) = s.trim_end_matches("mm").split_once(['x', 'X'])?;
    Some((w.trim().parse().ok()?, h.trim().parse().ok()?))
}
