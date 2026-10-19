//! Thin wrapper over the CUPS command line tools (`lp`, `lpstat`, `lpoptions`, `cancel`).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use tokio::process::Command;

use crate::ipp;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Printer {
    pub name: String,
    pub is_default: bool,
    pub state: String,
}

#[derive(Serialize)]
pub struct PrinterOption {
    pub key: String,
    pub label: String,
    pub choices: Vec<String>,
    /// The PPD's human names for `choices`, same order (the choice itself when it has none).
    pub labels: Vec<String>,
    pub default: String,
}

#[derive(Deserialize)]
pub struct PrintRequest {
    pub printer: String,
    pub title: String,
    pub copies: u32,
    pub collate: bool,
    /// PPD options passed as `-o key=value`.
    pub options: Vec<(String, String)>,
}

async fn run(cmd: &str, args: &[&str]) -> Result<String, String> {
    let out = Command::new(cmd)
        .args(args)
        .env("LC_ALL", "C")
        .output()
        .await
        .map_err(|e| format!("{cmd}: {e}"))?;
    if !out.status.success() {
        let msg = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(if msg.is_empty() {
            format!("{cmd} failed")
        } else {
            msg
        });
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

pub async fn printers() -> Result<Vec<Printer>, String> {
    let default = run("lpstat", &["-d"]).await.unwrap_or_default();
    let default = default.rsplit(':').next().unwrap_or("").trim().to_string();
    let list = run("lpstat", &["-p"]).await.unwrap_or_default();
    Ok(list
        .lines()
        .filter_map(|l| l.strip_prefix("printer "))
        .map(|l| {
            let name = l.split_whitespace().next().unwrap_or("").to_string();
            let state = if l.contains("disabled") {
                "disabled"
            } else if l.contains("now printing") {
                "printing"
            } else {
                "idle"
            };
            Printer {
                is_default: name == default,
                name,
                state: state.into(),
            }
        })
        .collect())
}

pub async fn options(printer: &str) -> Result<Vec<PrinterOption>, String> {
    let out = run("lpoptions", &["-p", printer, "-l"]).await?;
    Ok(out
        .lines()
        .filter_map(|line| {
            let (head, rest) = line.split_once(": ")?;
            let (key, label) = head.split_once('/').unwrap_or((head, head));
            let mut default = String::new();
            let choices: Vec<String> = rest
                .split_whitespace()
                .map(|c| match c.strip_prefix('*') {
                    Some(c) => {
                        default = c.to_string();
                        c.to_string()
                    }
                    None => c.to_string(),
                })
                .collect();
            Some(PrinterOption {
                key: key.into(),
                label: label.into(),
                labels: choices.clone(),
                choices,
                default,
            })
        })
        .collect())
}

/// Submits a PDF file; returns the CUPS job id (e.g. `L3210-Series-42`).
pub async fn submit(req: &PrintRequest, file: &str) -> Result<String, String> {
    let mut args: Vec<String> = vec![
        "-d".into(),
        req.printer.clone(),
        "-t".into(),
        req.title.clone(),
        "-n".into(),
        req.copies.max(1).to_string(),
        "-o".into(),
        format!("collate={}", req.collate),
        // Sheets are already composed at paper size; stop CUPS from rescaling them.
        "-o".into(),
        "print-scaling=none".into(),
    ];
    for (k, v) in &req.options {
        args.push("-o".into());
        args.push(format!("{k}={v}"));
    }
    args.push(file.into());
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let out = run("lp", &refs).await?;
    // "request id is L3210-Series-42 (1 file(s))"
    out.split_whitespace()
        .skip_while(|w| *w != "is")
        .nth(1)
        .map(str::to_string)
        .ok_or_else(|| format!("unexpected lp output: {out}"))
}

/// True while the job is still queued or printing. Errors when CUPS can't be asked, so callers
/// don't mistake a failed query for a finished job.
pub async fn job_active(printer: &str, job: &str) -> Result<bool, String> {
    let out = run("lpstat", &["-o", printer]).await?;
    Ok(out
        .lines()
        .any(|l| l.split_whitespace().next() == Some(job)))
}

pub async fn cancel(job: &str) -> Result<(), String> {
    run("cancel", &[job]).await.map(|_| ())
}

#[derive(Serialize)]
pub struct Paper {
    /// PageSize choice.
    pub id: String,
    pub label: String,
    /// Portrait, mm.
    pub w: f32,
    pub h: f32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrinterOptions {
    pub options: Vec<PrinterOption>,
    /// PPD UIConstraints: [key1, choice1, key2, choice2] pairs that can't be combined. A choice of
    /// "*" means any choice except None/False/Off (the PPD left it out).
    pub constraints: Vec<[String; 4]>,
    /// Fixed sizes the printer can feed, in PPD order.
    pub papers: Vec<Paper>,
    /// Custom size range in mm: [min width, max width, min height, max height].
    pub custom: Option<[f32; 4]>,
    /// Prints both sides by itself.
    pub duplex: bool,
    /// Sheets land printed side up, so the last page ends on top.
    pub face_up: bool,
}

async fn ppd(printer: &str) -> String {
    if let Ok(s) = tokio::fs::read(format!("/etc/cups/ppd/{printer}.ppd")).await {
        return String::from_utf8_lossy(&s).into_owned();
    }
    // The PPD is root:lp 0640 on most systems; cupsd serves it to anyone.
    let url = format!("http://localhost:631/printers/{printer}.ppd");
    run("curl", &["-fsS", "--max-time", "3", &url])
        .await
        .unwrap_or_default()
}

const PT_MM: f32 = 25.4 / 72.0;

/// `*Key Choice/Label: ...` → (key, choice, label).
fn ppd_entry(line: &str) -> Option<(&str, &str, &str)> {
    let (head, _) = line.strip_prefix('*')?.split_once(':')?;
    let (key, rest) = head.split_once(' ')?;
    let (choice, label) = rest.split_once('/').unwrap_or((rest, rest));
    Some((key, choice.trim(), label.trim()))
}

fn constraint(rest: &str) -> Option<[String; 4]> {
    let parts: Vec<&str> = rest.split_whitespace().collect();
    // Option names start with '*'; a word without one is a choice.
    let key = |w: &&str| w.starts_with('*');
    let (k1, v1, k2, v2) = match parts.as_slice() {
        [k1, v1, k2, v2] => (*k1, *v1, *k2, *v2),
        [k1, v1, k2] if key(k2) => (*k1, *v1, *k2, "*"),
        [k1, k2, v2] if key(k2) => (*k1, "*", *k2, *v2),
        [k1, k2] => (*k1, "*", *k2, "*"),
        _ => return None,
    };
    Some([
        k1.trim_start_matches('*').to_string(),
        v1.to_string(),
        k2.trim_start_matches('*').to_string(),
        v2.to_string(),
    ])
}

/// Constraints, fixed paper sizes and the custom size range from a PPD; fills in choice labels.
fn read_ppd(
    ppd: &str,
    options: &mut [PrinterOption],
) -> (Vec<[String; 4]>, Vec<Paper>, Option<[f32; 4]>) {
    let mut constraints = Vec::new();
    let mut papers = Vec::new();
    let (mut width, mut height) = (None, None);
    for line in ppd.lines() {
        if let Some(rest) = line.strip_prefix("*UIConstraints:") {
            constraints.extend(constraint(rest));
        } else if let Some(rest) = line.strip_prefix("*ParamCustomPageSize ") {
            // "Width: 1 points 153.07 612.00"
            let w: Vec<&str> = rest.split_whitespace().collect();
            let range = match w.as_slice() {
                [_, _, "points", min, max] => min.parse::<f32>().ok().zip(max.parse::<f32>().ok()),
                _ => None,
            };
            match w.first() {
                Some(&"Width:") => width = range,
                Some(&"Height:") => height = range,
                _ => {}
            }
        } else if let Some((key, choice, label)) = ppd_entry(line) {
            if key == "PaperDimension" {
                // `*PaperDimension A4/A4 210 x 297 mm: "595.3 841.9"`
                let dims: Vec<f32> = line
                    .rsplit('"')
                    .nth(1)
                    .unwrap_or("")
                    .split_whitespace()
                    .filter_map(|n| n.parse().ok())
                    .collect();
                if let [w, h] = dims[..] {
                    papers.push(Paper {
                        id: choice.into(),
                        label: label.into(),
                        w: (w.min(h) * PT_MM * 10.0).round() / 10.0,
                        h: (w.max(h) * PT_MM * 10.0).round() / 10.0,
                    });
                }
            } else if let Some(opt) = options.iter_mut().find(|o| o.key == key)
                && let Some(i) = opt.choices.iter().position(|c| c == choice)
            {
                opt.labels[i] = label.into();
            }
        }
    }
    let custom = width
        .zip(height)
        .map(|((w0, w1), (h0, h1))| [w0, w1, h0, h1].map(|pt| (pt * PT_MM * 10.0).round() / 10.0));
    (constraints, papers, custom)
}

/// Duplexer and output side. Without IPP, falls back to the PPD's Duplex option, and to face up
/// (what cupsd assumes too).
fn output_traits(attrs: &HashMap<String, Vec<String>>, options: &[PrinterOption]) -> (bool, bool) {
    let ppd_duplex = options
        .iter()
        .any(|o| o.key == "Duplex" && o.choices.iter().any(|c| c.starts_with("Duplex")));
    let duplex = match attrs.get("sides-supported") {
        Some(v) => v.iter().any(|s| s.starts_with("two-sided")),
        None => ppd_duplex,
    };
    let face_up = attrs
        .get("output-bin-default")
        .and_then(|v| v.first())
        .is_none_or(|b| b != "face-down");
    (duplex, face_up)
}

pub async fn options_with_constraints(printer: &str) -> Result<PrinterOptions, String> {
    let (options, ppd, attrs) = tokio::join!(
        options(printer),
        ppd(printer),
        ipp::attributes(printer, &["sides-supported", "output-bin-default"])
    );
    let mut options = options?;
    let (constraints, papers, custom) = read_ppd(&ppd, &mut options);
    let (duplex, face_up) = output_traits(&attrs, &options);
    Ok(PrinterOptions {
        options,
        constraints,
        papers,
        custom,
        duplex,
        face_up,
    })
}

#[derive(Serialize)]
pub struct Job {
    pub id: String,
    pub user: String,
    pub size: u64,
    pub time: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrinterInfo {
    pub name: String,
    pub model: String,
    pub uri: String,
    pub location: String,
    pub state: String,
    pub reasons: Vec<String>,
    pub accepting: bool,
    pub jobs: Vec<Job>,
    pub completed: Vec<Job>,
}

/// Splits `lpoptions -p` output (`k=v k='quoted value'`) into pairs.
fn parse_attrs(s: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    for ch in s.trim().chars().chain(std::iter::once(' ')) {
        match ch {
            '\'' => quoted = !quoted,
            ' ' if !quoted => {
                if let Some((k, v)) = cur.split_once('=') {
                    out.push((k.to_string(), v.to_string()));
                }
                cur.clear();
            }
            c => cur.push(c),
        }
    }
    out
}

fn parse_jobs(s: &str) -> Vec<Job> {
    s.lines()
        .filter_map(|l| {
            let mut it = l.split_whitespace();
            let id = it.next()?.to_string();
            let user = it.next()?.to_string();
            let size = it.next()?.parse().ok()?;
            let time = it.collect::<Vec<_>>().join(" ");
            Some(Job {
                id,
                user,
                size,
                time,
            })
        })
        .collect()
}

pub async fn info(printer: &str) -> Result<PrinterInfo, String> {
    let attrs: std::collections::HashMap<_, _> =
        parse_attrs(&run("lpoptions", &["-p", printer]).await?)
            .into_iter()
            .collect();
    let get = |k: &str| attrs.get(k).cloned().unwrap_or_default();
    let state = match get("printer-state").as_str() {
        "4" => "printing",
        "5" => "stopped",
        _ => "idle",
    };
    let jobs = parse_jobs(&run("lpstat", &["-o", printer]).await.unwrap_or_default());
    let mut completed = parse_jobs(
        &run("lpstat", &["-W", "completed", "-o", printer])
            .await
            .unwrap_or_default(),
    );
    completed.truncate(10);
    Ok(PrinterInfo {
        name: printer.to_string(),
        model: get("printer-make-and-model"),
        uri: get("device-uri"),
        location: get("printer-location"),
        state: state.into(),
        reasons: get("printer-state-reasons")
            .split(',')
            .filter(|r| *r != "none" && !r.is_empty())
            .map(str::to_string)
            .collect(),
        accepting: get("printer-is-accepting-jobs") == "true",
        jobs,
        completed,
    })
}

/// Resumes a stopped queue and makes it accept jobs again.
pub async fn resume(printer: &str) -> Result<(), String> {
    run("cupsenable", &[printer]).await?;
    run("cupsaccept", &[printer]).await.map(|_| ())
}

pub async fn cancel_all(printer: &str) -> Result<(), String> {
    run("cancel", &["-a", printer]).await.map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    const PPD: &str = r#"*OpenUI *MediaType/Type: PickOne
*MediaType PLAIN/plain papers: "<</HWResolution[720 720]>>setpagedevice"
*MediaType EPQIJ/Epson Photo Quality Ink Jet: ""
*ParamCustomPageSize Width: 1 points 153.07 612.00
*ParamCustomPageSize Height: 2 points 243.78 3401.57
*PaperDimension A4/A4 210 x 297 mm: "595.3 841.9"
*PaperDimension ENVELOPE_10/Envelope #10 4 1/8 x 9 1/2 in: "297.1 684.0"
*UIConstraints: *MediaType EPQIJ *PrintQuality Draft
"#;

    #[test]
    fn reads_papers_limits_and_labels() {
        let mut opts = vec![PrinterOption {
            key: "MediaType".into(),
            label: "Type".into(),
            choices: vec!["PLAIN".into(), "EPQIJ".into(), "ENVELOPE".into()],
            labels: vec!["PLAIN".into(), "EPQIJ".into(), "ENVELOPE".into()],
            default: "PLAIN".into(),
        }];
        let (constraints, papers, custom) = read_ppd(PPD, &mut opts);
        assert_eq!(
            opts[0].labels,
            ["plain papers", "Epson Photo Quality Ink Jet", "ENVELOPE"]
        );
        assert_eq!(
            constraints,
            [["MediaType", "EPQIJ", "PrintQuality", "Draft"].map(String::from)]
        );
        assert_eq!(papers[0].id, "A4");
        assert_eq!((papers[0].w, papers[0].h), (210.0, 297.0));
        assert_eq!(papers[1].label, "Envelope #10 4 1/8 x 9 1/2 in");
        assert_eq!(custom, Some([54.0, 215.9, 86.0, 1200.0]));
    }

    fn option(key: &str, choices: &[&str]) -> PrinterOption {
        let choices: Vec<String> = choices.iter().map(|c| c.to_string()).collect();
        PrinterOption {
            key: key.into(),
            label: key.into(),
            labels: choices.clone(),
            default: choices[0].clone(),
            choices,
        }
    }

    #[test]
    fn empty_ppd_reads_as_nothing() {
        let mut opts = vec![option("Duplex", &["None", "DuplexNoTumble"])];
        let (constraints, papers, custom) = read_ppd("", &mut opts);
        assert!(constraints.is_empty() && papers.is_empty() && custom.is_none());
    }

    #[test]
    fn output_traits_fall_back_without_ipp() {
        let none = HashMap::new();
        let duplexer = [option("Duplex", &["None", "DuplexNoTumble"])];
        assert_eq!(output_traits(&none, &duplexer), (true, true));
        assert_eq!(output_traits(&none, &[]), (false, true));
        let ipp = HashMap::from([
            ("sides-supported".to_string(), vec!["one-sided".to_string()]),
            (
                "output-bin-default".to_string(),
                vec!["face-down".to_string()],
            ),
        ]);
        assert_eq!(output_traits(&ipp, &duplexer), (false, false));
    }
}
