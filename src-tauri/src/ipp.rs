//! A minimal IPP Get-Printer-Attributes client for the facts CUPS knows but the command line tools
//! don't print (output tray, duplexer). Posts through `curl`, like the PPD fetch in `cups.rs`.

use std::collections::HashMap;
use std::process::Stdio;

use tokio::io::AsyncWriteExt;
use tokio::process::Command;

const GET_PRINTER_ATTRIBUTES: u16 = 0x000B;

fn attr(out: &mut Vec<u8>, tag: u8, name: &str, value: &str) {
    out.push(tag);
    out.extend((name.len() as u16).to_be_bytes());
    out.extend(name.as_bytes());
    out.extend((value.len() as u16).to_be_bytes());
    out.extend(value.as_bytes());
}

fn request(uri: &str, names: &[&str]) -> Vec<u8> {
    let mut out = vec![2, 0];
    out.extend(GET_PRINTER_ATTRIBUTES.to_be_bytes());
    out.extend(1u32.to_be_bytes());
    out.push(0x01); // operation-attributes
    attr(&mut out, 0x47, "attributes-charset", "utf-8");
    attr(&mut out, 0x48, "attributes-natural-language", "en");
    attr(&mut out, 0x45, "printer-uri", uri);
    for (i, n) in names.iter().enumerate() {
        // Additional values of the same attribute have an empty name.
        attr(
            &mut out,
            0x44,
            if i == 0 { "requested-attributes" } else { "" },
            n,
        );
    }
    out.push(0x03); // end
    out
}

/// String-valued attributes (keyword, name, text, uri...) of a response; other types are skipped.
fn parse(buf: &[u8]) -> Option<HashMap<String, Vec<String>>> {
    let status = u16::from_be_bytes(buf.get(2..4)?.try_into().ok()?);
    if status >= 0x0100 {
        return None;
    }
    let mut out: HashMap<String, Vec<String>> = HashMap::new();
    let mut name = String::new();
    let mut i = 8;
    let u16_at = |i: usize| {
        buf.get(i..i + 2)
            .map(|b| u16::from_be_bytes([b[0], b[1]]) as usize)
    };
    while let Some(&tag) = buf.get(i) {
        i += 1;
        if tag == 0x03 {
            break;
        }
        if tag < 0x10 {
            continue;
        }
        let n = u16_at(i)?;
        let key = buf.get(i + 2..i + 2 + n)?;
        i += 2 + n;
        let v = u16_at(i)?;
        let value = buf.get(i + 2..i + 2 + v)?;
        i += 2 + v;
        if n > 0 {
            name = String::from_utf8_lossy(key).into_owned();
        }
        if (0x41..=0x49).contains(&tag) {
            out.entry(name.clone())
                .or_default()
                .push(String::from_utf8_lossy(value).into_owned());
        }
    }
    Some(out)
}

/// The named attributes of a CUPS queue; empty when cupsd can't be reached.
pub async fn attributes(printer: &str, names: &[&str]) -> HashMap<String, Vec<String>> {
    let url = format!("http://localhost:631/printers/{printer}");
    let body = request(&url.replacen("http", "ipp", 1), names);
    let Ok(mut child) = Command::new("curl")
        .args([
            "-fsS",
            "--max-time",
            "3",
            "-H",
            "Content-Type: application/ipp",
        ])
        .args(["--data-binary", "@-", &url])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
    else {
        return HashMap::new();
    };
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(&body).await;
    }
    match child.wait_with_output().await {
        Ok(out) if out.status.success() => parse(&out.stdout).unwrap_or_default(),
        _ => HashMap::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_strings_and_additional_values() {
        let mut buf = vec![2, 0, 0, 0, 0, 0, 0, 1, 0x04];
        attr(&mut buf, 0x44, "sides-supported", "one-sided");
        attr(&mut buf, 0x44, "", "two-sided-long-edge");
        attr(&mut buf, 0x21, "copies-default", "\0\0\0\x01");
        attr(&mut buf, 0x44, "output-bin-default", "face-down");
        buf.push(0x03);
        let a = parse(&buf).unwrap();
        assert_eq!(a["sides-supported"], ["one-sided", "two-sided-long-edge"]);
        assert_eq!(a["output-bin-default"], ["face-down"]);
        assert!(!a.contains_key("copies-default"));
    }

    #[test]
    fn rejects_error_status() {
        assert!(parse(&[2, 0, 0x04, 0x06, 0, 0, 0, 1, 0x03]).is_none());
    }
}
