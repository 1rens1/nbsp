//! Loopback HTTP server for page thumbnails.
//!
//! WebKitGTK won't load a custom URI scheme from the dev server's http:// page (wry registers
//! schemes as secure but not CORS-enabled), so images come from 127.0.0.1 instead. Plain HTTP
//! keeps the webview's image cache and lazy loading working. A random token in the path keeps
//! other local processes out.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc;

use crate::engine::{Engine, RenderKey};

/// Starts the server; returns the base URL (`http://127.0.0.1:<port>/<token>`).
pub fn serve(engine: Engine) -> std::io::Result<String> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    let token = token()?;
    let base = format!("http://127.0.0.1:{port}/{token}");
    std::thread::Builder::new()
        .name("thumbs".into())
        .spawn(move || {
            for stream in listener.incoming().flatten() {
                let engine = engine.clone();
                let token = token.clone();
                std::thread::spawn(move || {
                    let _ = handle(stream, &engine, &token);
                });
            }
        })?;
    Ok(base)
}

/// 128 random bits; refusing to start beats serving pages behind a guessable token.
fn token() -> std::io::Result<String> {
    let mut buf = [0u8; 16];
    std::io::Read::read_exact(&mut std::fs::File::open("/dev/urandom")?, &mut buf)?;
    Ok(buf.iter().map(|b| format!("{b:02x}")).collect())
}

/// Serves keep-alive GET requests: `/<token>/page/<src>/<index>?w=<px>&r=<deg>&g=<0|1>`.
fn handle(stream: TcpStream, engine: &Engine, token: &str) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut out = stream;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            return Ok(());
        }
        // Skip headers.
        loop {
            let mut h = String::new();
            if reader.read_line(&mut h)? == 0 || h == "\r\n" || h == "\n" {
                break;
            }
        }
        let target = line.split_whitespace().nth(1).unwrap_or("");
        let body = parse(target, token).and_then(|key| {
            let (tx, rx) = mpsc::channel();
            engine.send(move |ctx| {
                let _ = tx.send(ctx.render(key));
            });
            rx.recv().ok()?.ok()
        });
        match body {
            Some(bytes) => {
                write!(
                    out,
                    "HTTP/1.1 200 OK\r\nContent-Type: image/jpeg\r\nContent-Length: {}\r\nCache-Control: max-age=31536000, immutable\r\nAccess-Control-Allow-Origin: *\r\n\r\n",
                    bytes.len()
                )?;
                out.write_all(&bytes)?;
            }
            None => write!(out, "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n")?,
        }
        out.flush()?;
    }
}

fn parse(target: &str, token: &str) -> Option<RenderKey> {
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let mut parts = path.trim_matches('/').split('/');
    if parts.next()? != token || parts.next()? != "page" {
        return None;
    }
    let mut key = RenderKey {
        src: parts.next()?.parse().ok()?,
        index: parts.next()?.parse().ok()?,
        width: 200,
        rot: 0,
        gray: false,
    };
    for (k, v) in query.split('&').filter_map(|kv| kv.split_once('=')) {
        match k {
            "w" => key.width = v.parse::<u16>().unwrap_or(200).clamp(16, 4000),
            "r" => key.rot = v.parse::<i32>().unwrap_or(0).rem_euclid(360) as u16,
            "g" => key.gray = v == "1",
            _ => {}
        }
    }
    Some(key)
}
