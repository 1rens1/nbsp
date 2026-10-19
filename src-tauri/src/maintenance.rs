//! Direct printer maintenance through gutenprint's `escputil` on the raw USB device
//! (status query, nozzle check, head cleaning). Epson only: other printers would take the ESC/P
//! commands as data. EcoTanks share one command set; escputil doesn't list them all (the L3210
//! isn't), so a close sibling model is passed.

use serde::Serialize;
use tokio::process::Command;

const MODEL_HINT: &str = "escp2-et2750";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceStatus {
    pub available: bool,
    pub status: String,
    pub detail: String,
}

async fn escputil(device: &str, args: &[&str]) -> Result<String, String> {
    let out = Command::new("escputil")
        .args(["-q", "-m", MODEL_HINT, "-r", device])
        .args(args)
        .output()
        .await
        .map_err(|e| format!("escputil (gutenprint) not available: {e}"))?;
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(if err.is_empty() {
            "printer did not respond".into()
        } else {
            err
        });
    }
    Ok(text)
}

/// Finds the raw USB device whose identity matches `model` (e.g. "L3210").
async fn device_for(model: &str) -> Option<String> {
    let mut devices: Vec<String> = std::fs::read_dir("/dev/usb")
        .ok()?
        .flatten()
        .map(|e| e.path().to_string_lossy().into_owned())
        .filter(|p| p.rsplit('/').next().is_some_and(|n| n.starts_with("lp")))
        .collect();
    devices.sort();
    for dev in devices {
        if let Ok(id) = escputil(&dev, &["-d"]).await
            && id.contains(model)
        {
            return Some(dev);
        }
    }
    None
}

fn model_key(make_and_model: &str) -> String {
    // "Epson L3210 Series - epson-inkjet-printer …" → "L3210"
    make_and_model
        .split_whitespace()
        .nth(1)
        .unwrap_or("")
        .to_string()
}

fn is_epson(make_and_model: &str) -> bool {
    make_and_model.to_ascii_lowercase().contains("epson")
}

pub async fn status(make_and_model: &str) -> DeviceStatus {
    if !is_epson(make_and_model) {
        return DeviceStatus {
            available: false,
            status: String::new(),
            detail: "Maintenance is only available for Epson printers.".into(),
        };
    }
    let Some(dev) = device_for(&model_key(make_and_model)).await else {
        return DeviceStatus {
            available: false,
            status: String::new(),
            detail: "Maintenance needs the printer connected over USB.".into(),
        };
    };
    match escputil(&dev, &["-s"]).await {
        Ok(text) => {
            let status = text
                .lines()
                .find_map(|l| l.trim().strip_prefix("Status:"))
                .map(|s| s.trim().to_string())
                .unwrap_or_else(|| "Unknown".into());
            DeviceStatus {
                available: true,
                status,
                detail: dev,
            }
        }
        Err(e) => DeviceStatus {
            available: false,
            status: String::new(),
            detail: e,
        },
    }
}

pub async fn run(make_and_model: &str, action: &str) -> Result<(), String> {
    let flag = match action {
        "nozzle" => "-n",
        "clean" => "-c",
        _ => return Err("unknown action".into()),
    };
    if !is_epson(make_and_model) {
        return Err("Maintenance is only available for Epson printers".into());
    }
    let dev = device_for(&model_key(make_and_model))
        .await
        .ok_or("Printer not reachable over USB")?;
    escputil(&dev, &[flag]).await.map(|_| ())
}
