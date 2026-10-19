//! Command line interface. Self-contained (clap only) so build.rs can include it to generate
//! shell completions.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum, ValueHint};

#[derive(Parser, Debug)]
#[command(
    name = "nbsp",
    version,
    about = "No Bull-Shit Print: print, impose and scan",
    args_conflicts_with_subcommands = true
)]
pub struct Cli {
    /// Files to open (PDF, images, office documents)
    #[arg(value_hint = ValueHint::FilePath)]
    pub files: Vec<PathBuf>,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Print files with the given settings (opens NBSP to show progress)
    #[command(visible_alias = "p")]
    Print(PrintArgs),
    /// Scan a page to a file, or open the scan dialog when no output is given
    #[command(visible_alias = "s")]
    Scan(ScanArgs),
    /// Show printer, queue and device status
    Status {
        /// Printer queue (default: system default printer)
        #[arg(short = 'P', long)]
        printer: Option<String>,
    },
    /// List printers
    Printers,
    /// Print a shell completion script
    Completions {
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
}

#[derive(Args, Debug, Default)]
pub struct PrintArgs {
    /// Files to print
    #[arg(required = true, value_hint = ValueHint::FilePath)]
    pub files: Vec<PathBuf>,

    /// Open NBSP with these settings applied instead of printing right away
    #[arg(short, long)]
    pub review: bool,

    /// Apply a saved preset first (other options override it)
    #[arg(long)]
    pub preset: Option<String>,

    /// Printer queue
    #[arg(short = 'P', long)]
    pub printer: Option<String>,

    /// Number of copies
    #[arg(short = 'n', long, value_parser = clap::value_parser!(u32).range(1..=999))]
    pub copies: Option<u32>,

    /// Pages to print: a range like "1-3,5,8-", or "odd" / "even"
    #[arg(short, long)]
    pub pages: Option<String>,

    /// Double-sided (without a duplexer, NBSP asks you to flip the stack)
    #[arg(short, long, value_enum)]
    pub duplex: Option<Duplex>,

    /// Pages per sheet
    #[arg(short = 'N', long, value_parser = ["1", "2", "4", "6", "9", "16"])]
    pub per_sheet: Option<String>,

    /// Booklet: fold in half and staple (implies short-edge double-sided)
    #[arg(short, long)]
    pub booklet: bool,

    /// Paper size, as the driver names it (A4, Letter, 4x6in, ...)
    #[arg(long)]
    pub paper: Option<String>,

    /// Custom paper size in mm, e.g. 100x150, within what the printer takes
    #[arg(long, value_name = "WxH", conflicts_with = "paper", value_parser = paper_size)]
    pub paper_size: Option<String>,

    #[arg(short, long, value_enum)]
    pub orientation: Option<Orientation>,

    #[arg(long, value_enum)]
    pub scale: Option<Scale>,

    /// Custom scale in percent (implies --scale custom)
    #[arg(long, value_name = "PERCENT", value_parser = clap::value_parser!(u32).range(5..=400))]
    pub scale_pct: Option<u32>,

    /// Margins in mm, all sides (0 to 60)
    #[arg(short, long, value_name = "MM", value_parser = margin)]
    pub margins: Option<f32>,

    #[arg(short, long, value_enum)]
    pub quality: Option<Quality>,

    /// Print in grayscale
    #[arg(short, long)]
    pub gray: bool,

    /// Paper type
    #[arg(long, value_enum)]
    pub media: Option<Media>,

    /// Print in document order (last page ends on top of the stack)
    #[arg(long)]
    pub no_reverse: bool,

    /// Stamp page numbers; optional format using {page} and {total}, e.g. "Page {page} of {total}"
    #[arg(long, value_name = "FORMAT", num_args = 0..=1, default_missing_value = "{page}")]
    pub page_numbers: Option<String>,

    /// Where page numbers go
    #[arg(long, value_enum, requires = "page_numbers")]
    pub number_position: Option<NumberPosition>,
}

#[derive(ValueEnum, Clone, Copy, Debug)]
pub enum NumberPosition {
    BottomCenter,
    BottomRight,
    BottomLeft,
    TopCenter,
    TopRight,
    TopLeft,
}

#[derive(Args, Debug)]
pub struct ScanArgs {
    /// Output file (.pdf, .png or .jpg). Without it, opens the scan dialog
    #[arg(short, long, value_hint = ValueHint::FilePath)]
    pub output: Option<PathBuf>,

    /// Resolution in dpi
    #[arg(short, long, default_value_t = 300, value_parser = clap::value_parser!(u32).range(50..=1200))]
    pub dpi: u32,

    #[arg(short, long, value_enum, default_value_t = ScanMode::Color)]
    pub mode: ScanMode,

    /// Area to scan, from the glass's top-left corner
    #[arg(short, long, value_enum, default_value_t = ScanSize::A4)]
    pub size: ScanSize,

    /// SANE device (default: first scanner found)
    #[arg(long)]
    pub device: Option<String>,
}

/// "100x150" or "100x150mm" (either orientation); the printer's own range is checked when printing.
fn paper_size(s: &str) -> Result<String, String> {
    let bad = || format!("expected WxH in mm, like 100x150 (not \"{s}\")");
    let (w, h) = s
        .trim_end_matches("mm")
        .split_once(['x', 'X'])
        .ok_or_else(bad)?;
    let (w, h): (f32, f32) = (
        w.trim().parse().map_err(|_| bad())?,
        h.trim().parse().map_err(|_| bad())?,
    );
    if w <= 0.0 || h <= 0.0 {
        return Err(bad());
    }
    Ok(s.to_string())
}

fn margin(s: &str) -> Result<f32, String> {
    match s.parse::<f32>() {
        Ok(m) if (0.0..=60.0).contains(&m) => Ok(m),
        _ => Err("margins are 0 to 60 mm".into()),
    }
}

#[derive(ValueEnum, Clone, Copy, Debug)]
pub enum Duplex {
    Off,
    Long,
    Short,
}

#[derive(ValueEnum, Clone, Copy, Debug)]
pub enum Orientation {
    Auto,
    Portrait,
    Landscape,
}

#[derive(ValueEnum, Clone, Copy, Debug)]
pub enum Scale {
    Fit,
    Shrink,
    Actual,
    Custom,
}

#[derive(ValueEnum, Clone, Copy, Debug)]
pub enum Quality {
    Draft,
    Standard,
    High,
}

#[derive(ValueEnum, Clone, Copy, Debug)]
pub enum Media {
    Plain,
    PhotoQuality,
    Matte,
    Glossy,
    PremiumGlossy,
    UltraGlossy,
    PhotoGlossy,
    Envelope,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq)]
pub enum ScanMode {
    Color,
    Gray,
    /// Black & white (1-bit, for text)
    Bw,
}

#[derive(ValueEnum, Clone, Copy, Debug)]
pub enum ScanSize {
    A4,
    Letter,
    B5,
    A5,
    A6,
    Postcard,
    #[value(name = "4r")]
    R4,
    Card,
    /// The whole glass
    Max,
}

impl ScanSize {
    /// Width and height in mm.
    pub fn mm(self) -> [f32; 2] {
        match self {
            Self::A4 => [210.0, 297.0],
            Self::Letter => [215.9, 279.4],
            Self::B5 => [182.0, 257.0],
            Self::A5 => [148.0, 210.0],
            Self::A6 => [105.0, 148.0],
            Self::Postcard => [100.0, 148.0],
            Self::R4 => [101.6, 152.4],
            Self::Card => [85.6, 54.0],
            Self::Max => [215.9, 297.18],
        }
    }
}
