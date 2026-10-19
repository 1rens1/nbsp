//! Scanning through SANE's `scanimage`.

use std::path::Path;
use std::process::Stdio;

use serde::{Deserialize, Serialize};
use tokio::io::AsyncReadExt;
use tokio::process::Command;

#[derive(Serialize)]
pub struct Scanner {
    pub id: String,
    pub name: String,
}

#[derive(Deserialize)]
pub struct ScanOptions {
    pub device: String,
    pub mode: String,
    pub dpi: u32,
    /// Scan area in mm from the top-left corner of the glass: [x, y, width, height].
    pub area: [f32; 4],
    pub brightness: i32,
    pub contrast: i32,
}

pub async fn devices() -> Result<Vec<Scanner>, String> {
    let out = Command::new("scanimage")
        .arg("-L")
        .output()
        .await
        .map_err(|e| format!("scanimage: {e}"))?;
    let text = String::from_utf8_lossy(&out.stdout);
    // device `epsonscan2:L3210 Series:...' is a EPSON L3210 Series flatbed scanner
    let mut list: Vec<Scanner> = text
        .lines()
        .filter_map(|l| {
            let rest = l.strip_prefix("device `")?;
            let (id, name) = rest.split_once("' is a ")?;
            if id.starts_with("v4l:") {
                return None;
            }
            // "EPSON L3210 Series:5841474B… flatbed scanner" → "EPSON L3210 Series"
            let name = name
                .split(':')
                .next()
                .unwrap_or(name)
                .trim_end_matches(" flatbed scanner")
                .trim();
            let kind = if id.starts_with("epsonscan2") {
                ""
            } else {
                " (generic driver)"
            };
            Some(Scanner {
                id: id.into(),
                name: format!("{name}{kind}"),
            })
        })
        .collect();
    // epsonscan2 exposes the full option set; the generic epson2 backend is the fallback.
    list.sort_by_key(|s| !s.id.starts_with("epsonscan2"));
    Ok(list)
}

/// Glass size when the backend doesn't say: Letter wide, A4 long (most flatbeds).
const GLASS: [f32; 2] = [215.9, 297.18];

/// Upper end of a `scanimage -A` range line like "    -x 0..215.9mm [215.9]".
fn range_max(text: &str, flag: &str) -> Option<f32> {
    let line = text.lines().find_map(|l| l.trim().strip_prefix(flag))?;
    let (_, max) = line.split_once("..")?;
    max.split_whitespace()
        .next()?
        .strip_suffix("mm")?
        .parse()
        .ok()
}

/// Glass size of a flatbed in mm, from the scan area it accepts.
pub async fn glass(device: &str) -> [f32; 2] {
    let text = match Command::new("scanimage")
        .args(["-d", device, "-A"])
        .output()
        .await
    {
        Ok(out) => String::from_utf8_lossy(&out.stdout).into_owned(),
        Err(_) => return GLASS,
    };
    match (range_max(&text, "-x "), range_max(&text, "-y ")) {
        (Some(w), Some(h)) if w > 0.0 && h > 0.0 => [w, h],
        _ => GLASS,
    }
}

/// Runs one scan into `out` (PNG, or JPEG when `preview`), reporting progress (0–100).
pub async fn scan(
    progress: impl Fn(f32) + Send + 'static,
    opts: &ScanOptions,
    out: &Path,
    preview: bool,
    cancel: tokio::sync::oneshot::Receiver<()>,
) -> Result<(), String> {
    // epsonscan2 adds the -l/-t offset to the size instead of shifting the area, so always scan
    // from the glass corner to the area's far edge; `crop` trims the offset afterwards.
    let [x, y, w, h] = opts.area;
    let right = (x + w).max(1.0);
    let bottom = (y + h).max(1.0);
    let epson = opts.device.starts_with("epsonscan2");
    let mut cmd = Command::new("scanimage");
    cmd.arg("-d")
        .arg(&opts.device)
        .arg(if preview {
            "--format=jpeg"
        } else {
            "--format=png"
        })
        .arg("--progress")
        .arg(format!("--mode={}", opts.mode))
        .arg(format!("--resolution={}", opts.dpi))
        .arg("-l")
        .arg("0")
        .arg("-t")
        .arg("0")
        .arg("-x")
        .arg(format!("{right}"))
        .arg("-y")
        .arg(format!("{bottom}"))
        .arg("-o")
        .arg(out);
    if epson {
        cmd.arg("--scan-area=Manual")
            .arg(format!("--brightness={}", opts.brightness))
            .arg(format!("--contrast={}", opts.contrast));
    }
    let mut child = cmd
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("scanimage: {e}"))?;
    let mut stderr = child.stderr.take().unwrap();
    let reader = tokio::spawn(async move {
        let mut log = String::new();
        let mut buf = [0u8; 256];
        while let Ok(n) = stderr.read(&mut buf).await {
            if n == 0 {
                break;
            }
            let chunk = String::from_utf8_lossy(&buf[..n]);
            for part in chunk
                .split(['\r', '\n'])
                .map(str::trim)
                .filter(|p| !p.is_empty())
            {
                match part
                    .strip_prefix("Progress: ")
                    .and_then(|p| p.trim_end_matches('%').parse::<f32>().ok())
                {
                    Some(v) => progress(v),
                    None => {
                        log.push_str(part);
                        log.push('\n');
                    }
                }
            }
        }
        log
    });
    let status = tokio::select! {
        s = child.wait() => s.map_err(|e| e.to_string())?,
        _ = cancel => {
            let _ = child.kill().await;
            let _ = std::fs::remove_file(out);
            return Err("cancelled".into());
        }
    };
    let log = reader.await.unwrap_or_default();
    if !status.success() {
        return Err(if log.is_empty() {
            "scan failed".into()
        } else {
            log.trim().to_string()
        });
    }
    Ok(())
}

/// Finishes a scan that started at the glass corner (see `scan`): cuts out the requested area and
/// encodes it for embedding in a PDF, with a single decode. Color and gray become JPEG (pdfium
/// embeds PNGs as raw bitmaps); black & white stays lossless PNG. Returns the file to embed.
pub fn finish(png: &Path, opts: &ScanOptions) -> Result<std::path::PathBuf, String> {
    let [x, y, w, h] = opts.area;
    let crop = x >= 0.5 || y >= 0.5;
    if opts.mode == "Monochrome" && !crop {
        return Ok(png.to_path_buf());
    }
    let mut img = image::open(png).map_err(|e| e.to_string())?;
    if crop {
        let px = |mm: f32| (mm / 25.4 * opts.dpi as f32).round() as u32;
        let (cx, cy) = (px(x).min(img.width() - 1), px(y).min(img.height() - 1));
        let cw = px(w).min(img.width() - cx).max(1);
        let ch = px(h).min(img.height() - cy).max(1);
        img = img.crop_imm(cx, cy, cw, ch);
    }
    if opts.mode == "Monochrome" {
        img.save(png).map_err(|e| e.to_string())?;
        return Ok(png.to_path_buf());
    }
    let jpg = png.with_extension("jpg");
    let file = std::fs::File::create(&jpg).map_err(|e| e.to_string())?;
    let mut enc =
        image::codecs::jpeg::JpegEncoder::new_with_quality(std::io::BufWriter::new(file), 90);
    if opts.mode == "Grayscale" {
        enc.encode_image(&img.to_luma8())
    } else {
        enc.encode_image(&img.to_rgb8())
    }
    .map_err(|e| e.to_string())?;
    Ok(jpg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_glass_from_options() {
        let text = "  Geometry:\n    -l 0..215.9mm [0]\n    -x 0..215.9mm [215.9]\n    -y 0..297.18mm [297.18]\n";
        assert_eq!(range_max(text, "-x "), Some(215.9));
        assert_eq!(range_max(text, "-y "), Some(297.18));
        assert_eq!(range_max("    -x 320..767pel [inactive]", "-x "), None);
    }
}
