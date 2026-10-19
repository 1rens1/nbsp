//! Rewrites the page size inside a DOCX or ODT so LibreOffice lays the text out for that paper,
//! instead of the finished PDF page being scaled.

use std::io::{Read, Write};
use std::path::Path;
use zip::write::SimpleFileOptions;
use zip::{ZipArchive, ZipWriter};

/// Extensions `reflow` can patch.
pub fn supported(ext: &str) -> bool {
    matches!(ext, "docx" | "odt")
}

/// Writes a copy of `src` to `dst` with every section's page set to `paper_mm` (portrait w x h).
/// Landscape sections stay landscape.
pub fn reflow(src: &Path, dst: &Path, paper_mm: [f64; 2], ext: &str) -> Result<(), String> {
    let part = if ext == "docx" { "word/document.xml" } else { "styles.xml" };
    let mut zin = ZipArchive::new(std::fs::File::open(src).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let mut zout = ZipWriter::new(std::fs::File::create(dst).map_err(|e| e.to_string())?);
    for i in 0..zin.len() {
        let name = zin
            .by_index_raw(i)
            .map_err(|e| e.to_string())?
            .name()
            .map_err(|e| e.to_string())?
            .into_owned();
        if name != part {
            // Raw copy keeps ODT's stored `mimetype` entry first and untouched.
            let file = zin.by_index_raw(i).map_err(|e| e.to_string())?;
            zout.raw_copy_file(file).map_err(|e| e.to_string())?;
            continue;
        }
        let mut xml = String::new();
        zin.by_index(i)
            .map_err(|e| e.to_string())?
            .read_to_string(&mut xml)
            .map_err(|e| e.to_string())?;
        let xml = if ext == "docx" { patch_docx(&xml, paper_mm) } else { patch_odt(&xml, paper_mm) };
        zout.start_file(&name, SimpleFileOptions::default())
            .map_err(|e| e.to_string())?;
        zout.write_all(xml.as_bytes()).map_err(|e| e.to_string())?;
    }
    zout.finish().map_err(|e| e.to_string())?;
    Ok(())
}

/// Calls `f` on every `<tag ...>` whose name starts with `open`, replacing it with the result.
fn map_tags(xml: &str, open: &str, f: impl Fn(&str) -> String) -> String {
    let mut out = String::with_capacity(xml.len());
    let mut rest = xml;
    while let Some(at) = rest.find(open) {
        // `<w:pgSz` must not match `<w:pgSzExtra`.
        let after = rest[at + open.len()..].chars().next();
        let end = rest[at..].find('>').map(|e| at + e + 1);
        match (after, end) {
            (Some(c), Some(end)) if c.is_whitespace() || c == '/' || c == '>' => {
                out.push_str(&rest[..at]);
                out.push_str(&f(&rest[at..end]));
                rest = &rest[end..];
            }
            _ => {
                out.push_str(&rest[..at + open.len()]);
                rest = &rest[at + open.len()..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn get_attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let key = format!("{name}=\"");
    let start = tag.find(&key)? + key.len();
    let len = tag[start..].find('"')?;
    Some(&tag[start..start + len])
}

/// Replaces (or appends) `name="value"`.
fn set_attr(tag: &str, name: &str, value: &str) -> String {
    let key = format!("{name}=\"");
    if let Some(s) = tag.find(&key) {
        let vs = s + key.len();
        if let Some(len) = tag[vs..].find('"') {
            return format!("{}{}{}", &tag[..vs], value, &tag[vs + len..]);
        }
    }
    let close = if tag.ends_with("/>") { tag.len() - 2 } else { tag.len() - 1 };
    format!("{} {name}=\"{value}\"{}", tag[..close].trim_end(), &tag[close..])
}

fn remove_attr(tag: &str, name: &str) -> String {
    let key = format!(" {name}=\"");
    match tag.find(&key) {
        Some(s) => {
            let vs = s + key.len();
            match tag[vs..].find('"') {
                Some(len) => format!("{}{}", &tag[..s], &tag[vs + len + 1..]),
                None => tag.to_owned(),
            }
        }
        None => tag.to_owned(),
    }
}

fn patch_docx(xml: &str, [w, h]: [f64; 2]) -> String {
    let twips = |mm: f64| ((mm / 25.4 * 1440.0).round() as i64).to_string();
    map_tags(xml, "<w:pgSz", |tag| {
        let num = |n: &str| get_attr(tag, n).and_then(|v| v.parse::<f64>().ok());
        let landscape = get_attr(tag, "w:orient") == Some("landscape")
            || matches!((num("w:w"), num("w:h")), (Some(a), Some(b)) if a > b);
        let (pw, ph) = if landscape { (h, w) } else { (w, h) };
        let tag = remove_attr(tag, "w:code");
        set_attr(&set_attr(&tag, "w:w", &twips(pw)), "w:h", &twips(ph))
    })
}

/// ODF lengths are `<number><unit>`; only the comparison matters, so units are normalized to mm.
fn odf_mm(v: &str) -> Option<f64> {
    let n = v.trim_end_matches(|c: char| c.is_ascii_alphabetic());
    let x: f64 = n.parse().ok()?;
    Some(match &v[n.len()..] {
        "mm" => x,
        "cm" => x * 10.0,
        "in" => x * 25.4,
        "pt" => x * 25.4 / 72.0,
        "pc" => x * 25.4 / 6.0,
        _ => return None,
    })
}

fn patch_odt(xml: &str, [w, h]: [f64; 2]) -> String {
    map_tags(xml, "<style:page-layout-properties", |tag| {
        if get_attr(tag, "fo:page-width").is_none() {
            return tag.to_owned();
        }
        let dim = |n: &str| get_attr(tag, n).and_then(odf_mm);
        let landscape = get_attr(tag, "style:print-orientation") == Some("landscape")
            || matches!((dim("fo:page-width"), dim("fo:page-height")), (Some(a), Some(b)) if a > b);
        let (pw, ph) = if landscape { (h, w) } else { (w, h) };
        set_attr(&set_attr(tag, "fo:page-width", &format!("{pw:.2}mm")), "fo:page-height", &format!("{ph:.2}mm"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn docx_pgsz() {
        let xml = r#"<w:sectPr><w:pgSz w:h="16834" w:w="11909" w:orient="portrait" w:code="9"/><w:pgMar w:top="1"/></w:sectPr>"#;
        let out = patch_docx(xml, [215.9, 330.2]);
        assert!(out.contains(r#"w:h="18720""#) && out.contains(r#"w:w="12240""#), "{out}");
        assert!(!out.contains("w:code") && out.contains("<w:pgMar"), "{out}");
    }

    #[test]
    fn docx_landscape_stays() {
        let xml = r#"<w:pgSz w:w="16834" w:h="11909" w:orient="landscape"/>"#;
        let out = patch_docx(xml, [215.9, 330.2]);
        assert!(out.contains(r#"w:w="18720""#) && out.contains(r#"w:h="12240""#), "{out}");
    }

    #[test]
    fn odt_layout() {
        let xml = r#"<style:page-layout-properties fo:page-width="21.001cm" fo:page-height="29.7cm" style:print-orientation="portrait" fo:margin-top="2cm"/>"#;
        let out = patch_odt(xml, [215.9, 330.2]);
        assert!(out.contains(r#"fo:page-width="215.90mm""#) && out.contains(r#"fo:page-height="330.20mm""#), "{out}");
        assert!(out.contains("fo:margin-top"), "{out}");
    }
}
