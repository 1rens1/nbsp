mod cli;
mod cups;
mod engine;
mod ipp;
mod headless;
mod launch;
mod maintenance;
mod reflow;
mod scan;
mod thumbs;

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};

use clap::Parser;
use engine::{Engine, Sheet, SourceInfo};
use launch::Launch;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_store::StoreExt;

const INBOX: &str = "/var/spool/nbsp";
const IMAGE_EXT: &[&str] = &["png", "jpg", "jpeg", "webp"];
const OFFICE_EXT: &[&str] = &[
    "doc", "docx", "odt", "rtf", "txt", "xls", "xlsx", "ods", "csv", "ppt", "pptx", "odp",
];

struct App {
    engine: Engine,
    thumbs: String,
    cache: PathBuf,
    resources: PathBuf,
    pending: Mutex<Option<Launch>>,
    scan_cancel: Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
    /// Scans this session, for "Scan 1", "Scan 2"…
    scans: AtomicU32,
    /// One LibreOffice conversion at a time: they share a profile, which soffice locks.
    converting: tokio::sync::Mutex<()>,
    watcher: Mutex<Option<notify::RecommendedWatcher>>,
}

fn millis() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

/// Waits for a launched program in the background so it doesn't linger as a zombie.
fn reap(child: std::io::Result<tokio::process::Child>) -> Result<(), String> {
    let mut child = child.map_err(|e| e.to_string())?;
    tokio::spawn(async move {
        let _ = child.wait().await;
    });
    Ok(())
}

#[derive(Serialize)]
struct Opened {
    sources: Vec<SourceInfo>,
    errors: Vec<String>,
}

fn ext(p: &Path) -> String {
    p.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}

fn file_name(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Converts an office document to PDF in `outdir` (which the caller removes afterwards).
/// With `paper_mm`, a DOCX/ODT is first copied with its page size changed so the text reflows.
async fn convert_office(
    path: &Path,
    outdir: &Path,
    cache: &Path,
    paper_mm: Option<[f64; 2]>,
) -> Result<PathBuf, String> {
    // A private profile: with the user's LibreOffice already open, --headless would hand the
    // request to that instance and silently produce nothing. Kept in the cache: creating one
    // takes seconds.
    let profile = format!(
        "-env:UserInstallation=file://{}",
        cache.join("lo-profile").display()
    );
    std::fs::create_dir_all(outdir).map_err(|e| e.to_string())?;
    let e = ext(path);
    let input = match paper_mm {
        Some(paper) if reflow::supported(&e) => {
            let copy = outdir.join(file_name(path));
            let (src, dst) = (path.to_owned(), copy.clone());
            tokio::task::spawn_blocking(move || reflow::reflow(&src, &dst, paper, &e))
                .await
                .map_err(|e| e.to_string())??;
            copy
        }
        _ => path.to_owned(),
    };
    // The patched copy lives in `outdir` too; the PDF gets its own subfolder so they can't collide.
    let pdfdir = outdir.join("pdf");
    let out = tokio::process::Command::new("soffice")
        .arg(profile)
        .args(["--headless", "--convert-to", "pdf", "--outdir"])
        .arg(&pdfdir)
        .arg(&input)
        .output()
        .await
        .map_err(|e| format!("LibreOffice not available: {e}"))?;
    let pdf = pdfdir.join(Path::new(&file_name(path)).with_extension("pdf"));
    if !out.status.success() || !pdf.exists() {
        return Err(format!("could not convert {}", file_name(path)));
    }
    Ok(pdf)
}

/// Converts and opens one office document. Own output dir per call: same-named files from
/// different folders can't collide.
async fn open_office(
    app: &App,
    path: &Path,
    name: String,
    paper_mm: Option<[f64; 2]>,
) -> Result<SourceInfo, String> {
    let _one = app.converting.lock().await;
    let outdir = app.cache.join(format!("convert-{}", millis()));
    let res = match convert_office(path, &outdir, &app.cache, paper_mm).await {
        // open_pdf reads the file into memory, so the dir can go right after.
        Ok(pdf) => app.engine.call(move |ctx| ctx.open_pdf(&pdf, name)).await,
        Err(e) => Err(e),
    };
    let _ = std::fs::remove_dir_all(&outdir);
    res.map(|mut s| {
        s.path = path.to_string_lossy().into_owned();
        s
    })
}

/// Re-lays out a DOCX/ODT for another paper size (mm, portrait). The new source replaces the
/// old one in the UI.
#[tauri::command]
async fn reflow_office(
    app: State<'_, App>,
    path: String,
    paper: [f64; 2],
) -> Result<SourceInfo, String> {
    let path = PathBuf::from(path);
    if !reflow::supported(&ext(&path)) {
        return Err(format!("{} can't be reflowed", file_name(&path)));
    }
    open_office(&app, &path, file_name(&path), Some(paper)).await
}

/// Opens PDFs, images and (via LibreOffice) office documents. `consume` deletes the file afterwards
/// (used for virtual-printer jobs).
#[tauri::command]
async fn open_files(
    app: State<'_, App>,
    paths: Vec<String>,
    consume: Option<bool>,
) -> Result<Opened, String> {
    let mut sources = Vec::new();
    let mut errors = Vec::new();
    for p in paths {
        let path = PathBuf::from(&p);
        let name = file_name(&path);
        let e = ext(&path);
        let res = if IMAGE_EXT.contains(&e.as_str()) {
            let path = path.clone();
            app.engine
                .call(move |ctx| ctx.open_image(&path, name, None))
                .await
        } else if OFFICE_EXT.contains(&e.as_str()) {
            open_office(&app, &path, name, None).await
        } else {
            let path = path.clone();
            app.engine.call(move |ctx| ctx.open_pdf(&path, name)).await
        };
        match res {
            Ok(s) => {
                if consume == Some(true) {
                    let _ = std::fs::remove_file(&path);
                }
                sources.push(s)
            }
            Err(e) => errors.push(format!("{}: {e}", file_name(&path))),
        }
    }
    Ok(Opened { sources, errors })
}

/// Stores pasted bytes (raw body, name in the `x-name` header) as a temp file and returns its path,
/// ready for `open_files` with `consume`.
#[tauri::command]
fn save_pasted(request: tauri::ipc::Request<'_>) -> Result<String, String> {
    static SEQ: AtomicU32 = AtomicU32::new(0);
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else {
        return Err("No data".into());
    };
    let name = request
        .headers()
        .get("x-name")
        .and_then(|v| v.to_str().ok())
        .map(urlencoding_decode)
        .unwrap_or_default();
    let name = Path::new(&name)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| "pasted".into());
    let dir = std::env::temp_dir().join(format!(
        "nbsp-paste-{}-{}",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(name);
    std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().into_owned())
}

/// Percent-decoding for the `x-name` header (HTTP headers can't carry raw UTF-8).
fn urlencoding_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && i + 2 < b.len()
            && let Some(v) = std::str::from_utf8(&b[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok())
        {
            out.push(v);
            i += 3;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[tauri::command]
async fn close_source(app: State<'_, App>, id: u32) -> Result<(), String> {
    app.engine
        .call(move |ctx| {
            ctx.close(id);
            Ok(())
        })
        .await
}

#[tauri::command]
async fn duplex_test_pages(app: State<'_, App>) -> Result<SourceInfo, String> {
    app.engine.call(|ctx| ctx.duplex_test()).await
}

#[tauri::command]
async fn export_pdf(app: State<'_, App>, sheets: Vec<Sheet>, path: String) -> Result<(), String> {
    let bytes = app.engine.call(move |ctx| ctx.compose(&sheets)).await?;
    std::fs::write(&path, bytes).map_err(|e| e.to_string())
}

#[tauri::command]
async fn print_sheets(
    app: State<'_, App>,
    sheets: Vec<Sheet>,
    request: cups::PrintRequest,
) -> Result<String, String> {
    let bytes = app.engine.call(move |ctx| ctx.compose(&sheets)).await?;
    let file = app.cache.join(format!("job-{}.pdf", millis()));
    std::fs::write(&file, bytes).map_err(|e| e.to_string())?;
    let res = cups::submit(&request, &file.to_string_lossy()).await;
    // lp copies the file into the CUPS spool synchronously, so ours can go.
    let _ = std::fs::remove_file(&file);
    res
}

#[tauri::command]
async fn printers() -> Result<Vec<cups::Printer>, String> {
    cups::printers().await
}

#[tauri::command]
async fn printer_options(printer: String) -> Result<cups::PrinterOptions, String> {
    cups::options_with_constraints(&printer).await
}

#[tauri::command]
async fn printer_info(printer: String) -> Result<cups::PrinterInfo, String> {
    cups::info(&printer).await
}

#[tauri::command]
async fn printer_device_status(model: String) -> maintenance::DeviceStatus {
    maintenance::status(&model).await
}

/// Nozzle check or head cleaning. Refused while jobs are queued so raw commands can't interleave.
#[tauri::command]
async fn printer_maintenance(printer: String, action: String) -> Result<(), String> {
    let info = cups::info(&printer).await?;
    if !info.jobs.is_empty() {
        return Err("Wait until the print queue is empty".into());
    }
    maintenance::run(&info.model, &action).await
}

#[tauri::command]
async fn printer_resume(printer: String) -> Result<(), String> {
    cups::resume(&printer).await
}

#[tauri::command]
async fn cancel_all_jobs(printer: String) -> Result<(), String> {
    cups::cancel_all(&printer).await
}

/// Opens an http(s) URL in the user's default browser: the desktop entry `xdg-mime` reports,
/// launched directly, so wrappers that pick some other browser are bypassed. Falls back to xdg-open.
#[tauri::command]
async fn open_url(url: String) -> Result<(), String> {
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err("only http(s) links".into());
    }
    let scheme = if url.starts_with("https") {
        "x-scheme-handler/https"
    } else {
        "x-scheme-handler/http"
    };
    let desktop = tokio::process::Command::new("xdg-mime")
        .args(["query", "default", scheme])
        .output()
        .await
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    if let Some(id) = desktop.strip_suffix(".desktop") {
        let ok = tokio::process::Command::new("gtk-launch")
            .arg(id)
            .arg(&url)
            .status()
            .await
            .is_ok_and(|s| s.success());
        if ok {
            return Ok(());
        }
    }
    reap(tokio::process::Command::new("xdg-open").arg(&url).spawn())
}

#[tauri::command]
async fn open_epson_utility() -> Result<(), String> {
    reap(
        tokio::process::Command::new("/opt/epson-printer-utility/bin/epson-printer-utility")
            .spawn(),
    )
}

#[tauri::command]
async fn job_active(printer: String, job: String) -> Result<bool, String> {
    cups::job_active(&printer, &job).await
}

#[tauri::command]
async fn cancel_job(job: String) -> Result<(), String> {
    cups::cancel(&job).await
}

#[tauri::command]
async fn scanners() -> Result<Vec<scan::Scanner>, String> {
    scan::devices().await
}

#[tauri::command]
async fn scanner_glass(device: String) -> [f32; 2] {
    scan::glass(&device).await
}

#[tauri::command]
async fn scan_page(
    handle: AppHandle,
    app: State<'_, App>,
    options: scan::ScanOptions,
) -> Result<SourceInfo, String> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    *app.scan_cancel.lock().unwrap() = Some(tx);
    let out = app.cache.join(format!("scan-{}.png", millis()));
    let h = handle.clone();
    let res = scan::scan(
        move |p| drop(h.emit("scan-progress", p)),
        &options,
        &out,
        false,
        rx,
    )
    .await;
    app.scan_cancel.lock().unwrap().take();
    let info = match res.and_then(|()| scan::finish(&out, &options)) {
        Ok(embed) => {
            let dpi = options.dpi as f32;
            let name = format!("Scan {}", app.scans.fetch_add(1, Ordering::Relaxed) + 1);
            let path = embed.clone();
            app.engine
                .call(move |ctx| ctx.open_image(&path, name, Some(dpi)))
                .await
        }
        Err(e) => Err(e),
    };
    // The document holds its own copy; temp files go whatever happened.
    let _ = std::fs::remove_file(&out);
    let _ = std::fs::remove_file(out.with_extension("jpg"));
    info
}

/// Quick low-resolution scan of the whole glass; returns the JPEG bytes.
#[tauri::command]
async fn scan_preview(
    handle: AppHandle,
    app: State<'_, App>,
    mut options: scan::ScanOptions,
) -> Result<tauri::ipc::Response, String> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    *app.scan_cancel.lock().unwrap() = Some(tx);
    options.dpi = 75;
    let [w, h] = scan::glass(&options.device).await;
    options.area = [0.0, 0.0, w, h];
    if options.mode == "Monochrome" {
        options.mode = "Grayscale".into();
    }
    let out = app.cache.join("scan-preview.jpg");
    let h = handle.clone();
    let res = scan::scan(
        move |p| drop(h.emit("scan-progress", p)),
        &options,
        &out,
        true,
        rx,
    )
    .await;
    app.scan_cancel.lock().unwrap().take();
    res?;
    let bytes = std::fs::read(&out).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(&out);
    Ok(tauri::ipc::Response::new(bytes))
}

#[tauri::command]
fn cancel_scan(app: State<'_, App>) {
    if let Some(tx) = app.scan_cancel.lock().unwrap().take() {
        let _ = tx.send(());
    }
}

/// Starts a fresh NBSP once this one has exited (the delay lets the single-instance lock go first).
#[tauri::command]
fn restart_app(app: AppHandle) {
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::process::Command::new("sh")
            .arg("-c")
            .arg("sleep 0.4; exec \"$0\"")
            .arg(exe)
            .spawn();
    }
    app.exit(0);
}

#[tauri::command]
fn thumb_base(app: State<'_, App>) -> String {
    app.thumbs.clone()
}

/// The launch request from this process's own command line (taken once by the UI).
#[tauri::command]
fn take_launch(app: State<'_, App>) -> Option<Launch> {
    app.pending.lock().unwrap().take()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Capabilities {
    office: bool,
    virtual_printer: bool,
    epson_utility: bool,
    /// The virtual printer installer (a CUPS backend + pkexec) only exists for Linux.
    installer: bool,
    /// scanimage (SANE) is installed.
    scan: bool,
    /// Required command line tools that aren't installed.
    missing: Vec<&'static str>,
}

/// What printing needs: CUPS' client tools, and curl for the PPD and IPP queries.
const REQUIRED: [&str; 4] = ["lp", "lpstat", "lpoptions", "curl"];

fn has_command(cmd: &str) -> bool {
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|d| d.join(cmd).is_file()))
        .unwrap_or(false)
}

#[tauri::command]
async fn capabilities() -> Capabilities {
    let vp = cups::printers()
        .await
        .map(|l| l.iter().any(|p| p.name == "NBSP"))
        .unwrap_or(false);
    Capabilities {
        office: has_command("soffice"),
        virtual_printer: vp,
        epson_utility: Path::new("/opt/epson-printer-utility/bin/epson-printer-utility").exists(),
        installer: cfg!(target_os = "linux"),
        scan: has_command("scanimage"),
        missing: REQUIRED.into_iter().filter(|c| !has_command(c)).collect(),
    }
}

/// A plain-text report for bug reports: versions, tools and what CUPS says about the printer.
/// Leaves out the queue name and URI, which can identify the user or their network.
#[tauri::command]
async fn diagnostics(handle: AppHandle, printer: Option<String>) -> String {
    let os = tokio::fs::read_to_string("/etc/os-release")
        .await
        .ok()
        .and_then(|t| {
            t.lines()
                .find_map(|l| l.strip_prefix("PRETTY_NAME="))
                .map(|v| v.trim_matches('"').to_string())
        })
        .unwrap_or_else(|| std::env::consts::OS.to_string());
    let mut out = format!(
        "NBSP {}\nOS: {os} ({})\n",
        handle.package_info().version,
        std::env::consts::ARCH
    );
    let tools = ["lp", "lpstat", "lpoptions", "curl", "scanimage", "escputil", "soffice"];
    let tools: Vec<String> = tools
        .iter()
        .map(|t| format!("{t}={}", if has_command(t) { "yes" } else { "no" }))
        .collect();
    out += &format!("Tools: {}\n", tools.join(" "));
    if let Some(name) = printer {
        if let Ok(info) = cups::info(&name).await {
            out += &format!("Printer: {} ({})\n", info.model, info.state);
        }
        match cups::options_with_constraints(&name).await {
            Ok(o) => {
                let keys: Vec<&str> = o.options.iter().map(|o| o.key.as_str()).collect();
                out += &format!(
                    "Detected: duplex={} faceUp={} papers={} custom={:?}\nOptions: {}\n",
                    o.duplex,
                    o.face_up,
                    o.papers.len(),
                    o.custom,
                    keys.join(" ")
                );
            }
            Err(e) => out += &format!("Options failed: {e}\n"),
        }
    }
    out
}

#[tauri::command]
async fn setup_virtual_printer(
    handle: AppHandle,
    app: State<'_, App>,
    remove: bool,
) -> Result<(), String> {
    let script = app.resources.join("install.sh");
    let mut cmd = tokio::process::Command::new("pkexec");
    cmd.arg("sh").arg(&script);
    if remove {
        cmd.arg("remove");
    }
    let out = cmd.output().await.map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    if !remove {
        watch_inbox(&handle);
    }
    Ok(())
}

/// Watches the virtual printer's spool dir and forwards new jobs to the UI.
fn watch_inbox(handle: &AppHandle) {
    use notify::{EventKind, RecursiveMode, Watcher};
    let app = handle.state::<App>();
    let mut slot = app.watcher.lock().unwrap();
    if slot.is_some() || !Path::new(INBOX).is_dir() {
        return;
    }
    let h = handle.clone();
    let watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        let Ok(ev) = res else { return };
        if !matches!(
            ev.kind,
            EventKind::Create(_) | EventKind::Modify(notify::event::ModifyKind::Name(_))
        ) {
            return;
        }
        let files: Vec<String> = ev
            .paths
            .iter()
            .filter(|p| ext(p) == "pdf" && !file_name(p).starts_with('.') && p.exists())
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        if !files.is_empty() {
            if let Some(w) = h.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
            let _ = h.emit("inbox-files", files);
        }
    });
    if let Ok(mut w) = watcher
        && w.watch(Path::new(INBOX), RecursiveMode::NonRecursive)
            .is_ok()
    {
        *slot = Some(w);
    }
}

/// Sizes the window to ~90% of the monitor's work area (capped), so it fits 720p screens too.
fn fit_window(handle: &AppHandle) {
    let Some(w) = handle.get_webview_window("main") else {
        return;
    };
    let Ok(Some(m)) = w.current_monitor().or_else(|_| w.primary_monitor()) else {
        return;
    };
    let area = m.work_area();
    let sf = m.scale_factor();
    let (aw, ah) = (area.size.width as f64 / sf, area.size.height as f64 / sf);
    let width = (aw * 0.92).min(1400.0);
    let height = (ah * 0.92).min(900.0);
    let _ = w.set_size(tauri::LogicalSize::new(width, height));
    let _ = w.center();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let cli = cli::Cli::parse();
    if let Some(code) = headless::run(&cli) {
        std::process::exit(code);
    }
    let cwd = std::env::current_dir().unwrap_or_default();
    let mut launch = launch::from_cli(&cli, &cwd, true);
    // Jobs printed to the virtual printer while NBSP was closed.
    if let Ok(rd) = std::fs::read_dir(INBOX) {
        launch.inbox = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| ext(p) == "pdf" && !file_name(p).starts_with('.'))
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
    }

    let mut builder = tauri::Builder::default();
    // Debug builds can run beside the main instance (UI tests via tauri-driver).
    let single = !(cfg!(debug_assertions) && std::env::var_os("NBSP_MULTI_INSTANCE").is_some());
    if single {
        // A second `nbsp …` while NBSP runs: hand its request to this window.
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, args, cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
            if let Ok(cli) = cli::Cli::try_parse_from(&args) {
                let launch = launch::from_cli(&cli, Path::new(&cwd), false);
                if !launch.is_empty() {
                    let _ = app.emit("launch", launch);
                }
            }
        }));
    }
    builder
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .setup(move |app| {
            let resource_dir = app.path().resource_dir().unwrap_or_default();
            let dev_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            let mut lib_dirs = vec![resource_dir.join("lib")];
            lib_dirs.extend(headless::pdfium_dirs());
            let engine = Engine::start(lib_dirs)?;
            let thumbs = thumbs::serve(engine.clone())?;
            let cache = app.path().app_cache_dir()?;
            std::fs::create_dir_all(&cache)?;
            let resources = [resource_dir.join("cups"), dev_dir.join("resources/cups")]
                .into_iter()
                .find(|d| d.join("install.sh").exists())
                .unwrap_or_default();
            app.manage(App {
                engine,
                thumbs,
                cache,
                resources,
                pending: Mutex::new(Some(launch)),
                scan_cancel: Mutex::new(None),
                scans: AtomicU32::new(0),
                converting: tokio::sync::Mutex::new(()),
                watcher: Mutex::new(None),
            });
            watch_inbox(app.handle());
            // Built here rather than in tauri.conf.json: a compositor-drawn title bar can only be
            // dropped at creation, so the saved setting decides `decorations` before the window exists.
            let native = app
                .store("settings.json")
                .ok()
                .and_then(|s| s.get("systemTitleBar"))
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            tauri::WebviewWindowBuilder::new(app, "main", tauri::WebviewUrl::default())
                .title("NBSP")
                .inner_size(1280.0, 800.0)
                .min_inner_size(760.0, 540.0)
                .visible(false)
                .decorations(native)
                .transparent(false)
                .center()
                .build()?;
            fit_window(app.handle());
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.set_icon(tauri::include_image!("icons/128x128.png"));
            }
            // The page reveals the window after its first themed paint; this is the fallback.
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_secs(3));
                if let Some(w) = handle.get_webview_window("main")
                    && !w.is_visible().unwrap_or(true)
                {
                    let _ = w.show();
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            open_files,
            save_pasted,
            reflow_office,
            restart_app,
            close_source,
            export_pdf,
            duplex_test_pages,
            print_sheets,
            printers,
            printer_options,
            printer_info,
            printer_device_status,
            printer_maintenance,
            printer_resume,
            cancel_all_jobs,
            open_epson_utility,
            open_url,
            job_active,
            cancel_job,
            scanners,
            scanner_glass,
            scan_page,
            scan_preview,
            cancel_scan,
            take_launch,
            thumb_base,
            capabilities,
            diagnostics,
            setup_virtual_printer,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
