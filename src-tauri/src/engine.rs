//! Single-threaded pdfium engine. Pdfium is not thread-safe, so one dedicated thread owns the
//! library and every loaded document; everything else talks to it through `Engine::call`.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::sync::mpsc;

use lru::LruCache;
use pdfium_render::prelude::*;
use serde::{Deserialize, Serialize};

type Job = Box<dyn FnOnce(&mut Ctx) + Send>;

#[derive(Clone)]
pub struct Engine {
    tx: mpsc::Sender<Job>,
}

pub struct Ctx {
    pub pdfium: &'static Pdfium,
    pub docs: HashMap<u32, PdfDocument<'static>>,
    pub cache: LruCache<RenderKey, Arc<Vec<u8>>>,
    /// Total size of `cache`, kept under `CACHE_BYTES`.
    cache_bytes: usize,
    next_id: u32,
}

/// Rendered-JPEG budget: a few hundred thumbnails, or a few dozen full-zoom sheets.
const CACHE_BYTES: usize = 192 << 20;

/// Fonts with wide Unicode coverage for page-number labels that Helvetica (WinAnsi) can't show.
/// Liberation Sans first: it has Helvetica's metrics.
const LABEL_FONTS: &[&str] = &[
    "/usr/share/fonts/liberation-sans-fonts/LiberationSans-Regular.ttf",
    "/usr/share/fonts/liberation-sans/LiberationSans-Regular.ttf",
    "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
    "/usr/share/fonts/dejavu-sans-fonts/DejaVuSans.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
];

#[derive(Hash, PartialEq, Eq, Clone, Copy)]
pub struct RenderKey {
    pub src: u32,
    pub index: u16,
    pub width: u16,
    pub rot: u16,
    pub gray: bool,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SourceInfo {
    pub id: u32,
    pub name: String,
    pub path: String,
    /// Displayed page sizes in points (intrinsic /Rotate already applied).
    pub pages: Vec<[f32; 2]>,
}

/// One output sheet. Coordinates are PDF points with a top-left origin.
#[derive(Deserialize)]
pub struct Sheet {
    pub w: f32,
    pub h: f32,
    pub cells: Vec<Cell>,
    #[serde(default)]
    pub lines: Vec<[f32; 4]>,
    #[serde(default)]
    pub labels: Vec<Label>,
}

/// Text stamped on a sheet (page numbers). (x, y) is the baseline anchor, top-left origin.
#[derive(Deserialize)]
pub struct Label {
    pub text: String,
    pub x: f32,
    pub y: f32,
    pub size: f32,
    /// Clockwise degrees around the anchor.
    pub angle: f32,
    pub align: String,
}

#[derive(Deserialize)]
pub struct Cell {
    pub src: u32,
    pub index: u16,
    /// User rotation in degrees clockwise (0, 90, 180, 270), on top of the page's own /Rotate.
    pub rot: i32,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    #[serde(default)]
    pub border: bool,
}

impl Engine {
    pub fn start(lib_dirs: Vec<std::path::PathBuf>) -> Result<Engine, String> {
        let (tx, rx) = mpsc::channel::<Job>();
        let (ready_tx, ready_rx) = mpsc::channel::<Result<(), String>>();
        std::thread::Builder::new()
            .name("pdfium".into())
            .spawn(move || {
                let bindings = lib_dirs
                    .iter()
                    .find_map(|d| {
                        Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path(d))
                            .ok()
                    })
                    .map(Ok)
                    .unwrap_or_else(Pdfium::bind_to_system_library);
                let bindings = match bindings {
                    Ok(b) => b,
                    Err(e) => {
                        let _ = ready_tx.send(Err(format!("failed to load pdfium: {e}")));
                        return;
                    }
                };
                let pdfium: &'static Pdfium = Box::leak(Box::new(Pdfium::new(bindings)));
                let mut ctx = Ctx {
                    pdfium,
                    docs: HashMap::new(),
                    cache: LruCache::unbounded(),
                    cache_bytes: 0,
                    next_id: 1,
                };
                let _ = ready_tx.send(Ok(()));
                while let Ok(job) = rx.recv() {
                    job(&mut ctx);
                }
            })
            .map_err(|e| e.to_string())?;
        ready_rx.recv().map_err(|e| e.to_string())??;
        Ok(Engine { tx })
    }

    /// Fire-and-forget job (the job replies through its own channel).
    pub fn send(&self, job: impl FnOnce(&mut Ctx) + Send + 'static) {
        let _ = self.tx.send(Box::new(job));
    }

    pub async fn call<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Ctx) -> Result<T, String> + Send + 'static,
    ) -> Result<T, String> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.send(move |ctx| {
            let _ = tx.send(f(ctx));
        });
        rx.await.map_err(|_| "pdf engine stopped".to_string())?
    }
}

fn err(e: PdfiumError) -> String {
    e.to_string()
}

fn to_rotation(deg: i32) -> PdfPageRenderRotation {
    match deg.rem_euclid(360) {
        90 => PdfPageRenderRotation::Degrees90,
        180 => PdfPageRenderRotation::Degrees180,
        270 => PdfPageRenderRotation::Degrees270,
        _ => PdfPageRenderRotation::None,
    }
}

impl Ctx {
    fn insert(
        &mut self,
        doc: PdfDocument<'static>,
        name: String,
        path: String,
    ) -> Result<SourceInfo, String> {
        let pages = doc
            .pages()
            .iter()
            .map(|p| [p.width().value, p.height().value])
            .collect();
        let id = self.next_id;
        self.next_id += 1;
        self.docs.insert(id, doc);
        Ok(SourceInfo {
            id,
            name,
            path,
            pages,
        })
    }

    pub fn open_pdf(&mut self, path: &Path, name: String) -> Result<SourceInfo, String> {
        // Read into memory so the file can be moved/deleted while we hold the document.
        let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
        let doc = self
            .pdfium
            .load_pdf_from_byte_vec(bytes, None)
            .map_err(err)?;
        self.insert(doc, name, path.to_string_lossy().into_owned())
    }

    /// Wraps an image into a one-page document. `dpi` sizes the page (scans); without it the
    /// image is 150 dpi but never larger than A4, so phone photos don't become poster-sized pages.
    /// EXIF orientation is applied through the placement matrix, so JPEGs still pass through as-is.
    pub fn open_image(
        &mut self,
        path: &Path,
        name: String,
        dpi: Option<f32>,
    ) -> Result<SourceInfo, String> {
        use image::ImageDecoder;
        use image::metadata::Orientation;
        let reader = image::ImageReader::open(path)
            .and_then(|r| r.with_guessed_format())
            .map_err(|e| e.to_string())?;
        let is_jpeg = reader.format() == Some(image::ImageFormat::Jpeg);
        let mut decoder = reader.into_decoder().map_err(|e| e.to_string())?;
        let orientation = decoder.orientation().unwrap_or(Orientation::NoTransforms);
        let (pw, ph) = decoder.dimensions();
        let quarter = matches!(
            orientation,
            Orientation::Rotate90
                | Orientation::Rotate270
                | Orientation::Rotate90FlipH
                | Orientation::Rotate270FlipH
        );
        let (dw, dh) = if quarter { (ph, pw) } else { (pw, ph) };
        let dpi = dpi.unwrap_or_else(|| {
            let (long, short) = (dw.max(dh) as f32, dw.min(dh) as f32);
            150f32
                .max(long / (297.0 / 25.4))
                .max(short / (210.0 / 25.4))
        });
        let k = 72.0 / dpi;
        let (w, h) = (dw as f32 * k, dh as f32 * k);
        let mut doc = self.pdfium.create_new_pdf().map_err(err)?;
        let mut page = doc
            .pages_mut()
            .create_page_at_end(PdfPagePaperSize::new_custom(
                PdfPoints::new(w),
                PdfPoints::new(h),
            ))
            .map_err(err)?;
        let mut obj = if is_jpeg {
            // Pass JPEG data through untouched instead of re-encoding a raw bitmap.
            PdfPageImageObject::new_from_jpeg_file(&doc, path).map_err(err)?
        } else {
            let img = image::DynamicImage::from_decoder(decoder).map_err(|e| e.to_string())?;
            PdfPageImageObject::new(&doc, &img).map_err(err)?
        };
        // Maps the image's unit square (stored pixels) onto the upright page.
        let (sw, sh) = (pw as f32 * k, ph as f32 * k);
        let (a, b, c, d, e, f) = match orientation {
            Orientation::NoTransforms => (sw, 0.0, 0.0, sh, 0.0, 0.0),
            Orientation::FlipHorizontal => (-sw, 0.0, 0.0, sh, sw, 0.0),
            Orientation::Rotate180 => (-sw, 0.0, 0.0, -sh, sw, sh),
            Orientation::FlipVertical => (sw, 0.0, 0.0, -sh, 0.0, sh),
            Orientation::Rotate90FlipH => (0.0, -sw, -sh, 0.0, sh, sw),
            Orientation::Rotate90 => (0.0, -sw, sh, 0.0, 0.0, sw),
            Orientation::Rotate270FlipH => (0.0, sw, sh, 0.0, 0.0, 0.0),
            Orientation::Rotate270 => (0.0, sw, -sh, 0.0, sh, 0.0),
        };
        obj.transform(a, b, c, d, e, f).map_err(err)?;
        page.objects_mut().add_image_object(obj).map_err(err)?;
        drop(page);
        self.insert(doc, name, path.to_string_lossy().into_owned())
    }

    /// Four A4 pages (2 sheets, front/back) labelled with big text and an "up" arrow, used to
    /// check how the stack must be turned for manual duplex.
    pub fn duplex_test(&mut self) -> Result<SourceInfo, String> {
        let mut doc = self.pdfium.create_new_pdf().map_err(err)?;
        let font = doc.fonts_mut().helvetica_bold();
        let labels = [
            "Sheet 1 - FRONT",
            "Sheet 1 - BACK",
            "Sheet 2 - FRONT",
            "Sheet 2 - BACK",
        ];
        for label in labels {
            let mut page = doc
                .pages_mut()
                .create_page_at_end(PdfPagePaperSize::a4())
                .map_err(err)?;
            let (w, h) = (page.width().value, page.height().value);
            let objs = page.objects_mut();
            objs.create_text_object(
                PdfPoints::new(70.0),
                PdfPoints::new(h / 2.0),
                label,
                font,
                PdfPoints::new(40.0),
            )
            .map_err(err)?;
            objs.create_text_object(
                PdfPoints::new(70.0),
                PdfPoints::new(h / 2.0 - 40.0),
                "this edge is the TOP",
                font,
                PdfPoints::new(18.0),
            )
            .map_err(err)?;
            let c = PdfColor::BLACK;
            let lw = PdfPoints::new(6.0);
            let (ax, top) = (w / 2.0, h - 60.0);
            objs.create_path_object_line(
                PdfPoints::new(ax),
                PdfPoints::new(top - 220.0),
                PdfPoints::new(ax),
                PdfPoints::new(top),
                c,
                lw,
            )
            .map_err(err)?;
            objs.create_path_object_line(
                PdfPoints::new(ax - 50.0),
                PdfPoints::new(top - 50.0),
                PdfPoints::new(ax),
                PdfPoints::new(top),
                c,
                lw,
            )
            .map_err(err)?;
            objs.create_path_object_line(
                PdfPoints::new(ax + 50.0),
                PdfPoints::new(top - 50.0),
                PdfPoints::new(ax),
                PdfPoints::new(top),
                c,
                lw,
            )
            .map_err(err)?;
        }
        self.insert(doc, "Duplex test".into(), String::new())
    }

    pub fn close(&mut self, id: u32) {
        self.docs.remove(&id);
        let keys: Vec<_> = self
            .cache
            .iter()
            .filter(|(k, _)| k.src == id)
            .map(|(k, _)| *k)
            .collect();
        for k in keys {
            if let Some(v) = self.cache.pop(&k) {
                self.cache_bytes -= v.len();
            }
        }
    }

    pub fn render(&mut self, key: RenderKey) -> Result<Arc<Vec<u8>>, String> {
        if let Some(hit) = self.cache.get(&key) {
            return Ok(hit.clone());
        }
        let doc = self.docs.get(&key.src).ok_or("unknown source")?;
        let page = doc.pages().get(key.index as _).map_err(err)?;
        let width = key.width.max(16) as i32;
        // With rotated constraints pdfium applies the size to the unrotated page, so a quarter turn
        // constrains the height that ends up as the output's width.
        let cfg = if key.rot % 180 == 90 {
            PdfRenderConfig::new().set_target_height(width)
        } else {
            PdfRenderConfig::new().set_target_width(width)
        };
        let cfg = cfg
            .rotate(to_rotation(key.rot as i32), true)
            .render_form_data(true)
            .use_grayscale_rendering(key.gray);
        let bitmap = page.render_with_config(&cfg).map_err(err)?;
        let img = bitmap.as_image().map_err(err)?.to_rgb8();
        let mut out = Vec::with_capacity(img.len() / 8);
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 82)
            .encode_image(&img)
            .map_err(|e| e.to_string())?;
        let out = Arc::new(out);
        self.cache_bytes += out.len();
        if let Some((_, old)) = self.cache.push(key, out.clone()) {
            self.cache_bytes -= old.len();
        }
        while self.cache_bytes > CACHE_BYTES {
            let Some((_, v)) = self.cache.pop_lru() else {
                break;
            };
            self.cache_bytes -= v.len();
        }
        Ok(out)
    }

    /// Builds the output PDF: each source page is placed as a Form XObject (vector, no rasterising).
    pub fn compose(&mut self, sheets: &[Sheet]) -> Result<Vec<u8>, String> {
        let mut out = self.pdfium.create_new_pdf().map_err(err)?;
        let font = out.fonts_mut().helvetica();
        let wide = sheets
            .iter()
            .flat_map(|s| &s.labels)
            .any(|l| l.text.chars().any(|c| c as u32 > 0x7e));
        // Only non-ASCII text needs (and pays for) an embedded font.
        let label_font = LABEL_FONTS
            .iter()
            .filter(|_| wide)
            .find_map(|p| out.fonts_mut().load_true_type_from_file(p, true).ok())
            .unwrap_or(font);
        for sheet in sheets {
            let mut page = out
                .pages_mut()
                .create_page_at_end(PdfPagePaperSize::new_custom(
                    PdfPoints::new(sheet.w),
                    PdfPoints::new(sheet.h),
                ))
                .map_err(err)?;
            page.set_content_regeneration_strategy(PdfPageContentRegenerationStrategy::Manual);
            for cell in &sheet.cells {
                let src = self.docs.get_mut(&cell.src).ok_or("unknown source")?;
                let mut sp = src.pages().get(cell.index as _).map_err(err)?;
                // Form fields and markup (highlights, stamps, ink) are not part of the content
                // stream. Links and popups have nothing to print, so they don't need the slow flatten.
                let printable = sp.annotations().iter().any(|a| {
                    !matches!(
                        a.annotation_type(),
                        PdfPageAnnotationType::Link | PdfPageAnnotationType::Popup
                    )
                });
                if printable {
                    sp.flatten().map_err(err)?;
                    sp = src.pages().get(cell.index as _).map_err(err)?;
                }
                // pdfium's form XObject already maps the page's visible box (CropBox within MediaBox)
                // to [0,w0]x[0,h0] with the page's own /Rotate applied, i.e. its displayed size.
                let (w0, h0) = (sp.width().value, sp.height().value);
                // Clockwise user rotation of that box, landing back in the positive quadrant.
                let (a, b, c, d, e, f, rw) = match cell.rot.rem_euclid(360) {
                    90 => (0.0, -1.0, 1.0, 0.0, 0.0, w0, h0),
                    180 => (-1.0, 0.0, 0.0, -1.0, w0, h0, w0),
                    270 => (0.0, 1.0, -1.0, 0.0, h0, 0.0, h0),
                    _ => (1.0, 0.0, 0.0, 1.0, 0.0, 0.0, w0),
                };
                let s = cell.w / rw;
                let tx = cell.x;
                let ty = sheet.h - cell.y - cell.h;
                // SAFETY: pdfium-render ties the objects' lifetime to the `sp` borrow and requires it to
                // match the destination document's; `sp` outlives this call, so widening is sound.
                let objects: &PdfPageObjects<'static> =
                    unsafe { std::mem::transmute(sp.objects()) };
                let mut obj = objects
                    .copy_into_x_object_form_object(&mut out)
                    .map_err(err)?;
                obj.transform(s * a, s * b, s * c, s * d, s * e + tx, s * f + ty)
                    .map_err(err)?;
                page.objects_mut().add_object(obj).map_err(err)?;
                if cell.border {
                    let r = PdfRect::new_from_values(ty, tx, ty + cell.h, tx + cell.w);
                    page.objects_mut()
                        .create_path_object_rect(
                            r,
                            Some(PdfColor::new(120, 120, 120, 255)),
                            Some(PdfPoints::new(0.5)),
                            None,
                        )
                        .map_err(err)?;
                }
            }
            for l in &sheet.lines {
                page.objects_mut()
                    .create_path_object_line(
                        PdfPoints::new(l[0]),
                        PdfPoints::new(sheet.h - l[1]),
                        PdfPoints::new(l[2]),
                        PdfPoints::new(sheet.h - l[3]),
                        PdfColor::new(150, 150, 150, 255),
                        PdfPoints::new(0.3),
                    )
                    .map_err(err)?;
            }
            for l in &sheet.labels {
                let mut obj =
                    PdfPageTextObject::new(&out, &l.text, label_font, PdfPoints::new(l.size))
                        .map_err(err)?;
                obj.set_fill_color(PdfColor::new(40, 40, 40, 255))
                    .map_err(err)?;
                let width = obj.width().map_err(err)?.value;
                let dx = match l.align.as_str() {
                    "center" => width / 2.0,
                    "right" => width,
                    _ => 0.0,
                };
                // Visual clockwise turn = negative angle in PDF's y-up space.
                let (sin, cos) = l.angle.to_radians().sin_cos();
                let (a, b, c, d) = (cos, -sin, sin, cos);
                let (x, y) = (l.x, sheet.h - l.y);
                obj.transform(a, b, c, d, x - dx * a, y - dx * b)
                    .map_err(err)?;
                page.objects_mut().add_text_object(obj).map_err(err)?;
            }
            page.regenerate_content().map_err(err)?;
        }
        out.save_to_bytes().map_err(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// A page whose CropBox (blue) sits inside a red MediaBox must print as all blue, at the
    /// size the UI was told, for every /Rotate. Runs whenever src-tauri/lib has pdfium.
    #[test]
    fn compose_uses_crop_box() {
        let Ok(engine) = Engine::start(vec![PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("lib")])
        else {
            return;
        };
        let (tx, rx) = mpsc::channel();
        engine.send(move |ctx| {
            let mut bad = Vec::new();
            for rotation in [
                PdfPageRenderRotation::None,
                PdfPageRenderRotation::Degrees90,
                PdfPageRenderRotation::Degrees180,
                PdfPageRenderRotation::Degrees270,
            ] {
                for user_rot in [0, 90] {
                    let mut doc = ctx.pdfium.create_new_pdf().unwrap();
                    let mut page = doc
                        .pages_mut()
                        .create_page_at_end(PdfPagePaperSize::new_custom(
                            PdfPoints::new(600.0),
                            PdfPoints::new(800.0),
                        ))
                        .unwrap();
                    let red = PdfColor::new(255, 0, 0, 255);
                    let blue = PdfColor::new(0, 0, 255, 255);
                    let rect = |b, l, t, r| PdfRect::new_from_values(b, l, t, r);
                    page.objects_mut()
                        .create_path_object_rect(
                            rect(0.0, 0.0, 800.0, 600.0),
                            None,
                            None,
                            Some(red),
                        )
                        .unwrap();
                    page.objects_mut()
                        .create_path_object_rect(
                            rect(100.0, 100.0, 500.0, 400.0),
                            None,
                            None,
                            Some(blue),
                        )
                        .unwrap();
                    // Marks the visible top-left corner, so a wrong turn or mirror shows up.
                    page.objects_mut()
                        .create_path_object_rect(
                            rect(420.0, 100.0, 500.0, 180.0),
                            None,
                            None,
                            Some(PdfColor::new(0, 255, 0, 255)),
                        )
                        .unwrap();
                    page.boundaries_mut()
                        .set_crop(rect(100.0, 100.0, 500.0, 400.0))
                        .unwrap();
                    page.set_rotation(rotation);
                    drop(page);
                    let info = ctx.insert(doc, "t".into(), String::new()).unwrap();
                    let [w, h] = info.pages[0];
                    let (w, h) = if user_rot == 90 { (h, w) } else { (w, h) };
                    let bytes = ctx
                        .compose(&[Sheet {
                            w,
                            h,
                            cells: vec![Cell {
                                src: info.id,
                                index: 0,
                                rot: user_rot,
                                x: 0.0,
                                y: 0.0,
                                w,
                                h,
                                border: false,
                            }],
                            lines: vec![],
                            labels: vec![],
                        }])
                        .unwrap();
                    let out = ctx.pdfium.load_pdf_from_byte_vec(bytes, None).unwrap();
                    let img = out
                        .pages()
                        .get(0)
                        .unwrap()
                        .render_with_config(&PdfRenderConfig::new().set_target_width(w as i32))
                        .unwrap()
                        .as_image()
                        .unwrap()
                        .to_rgb8();
                    // The preview shows pdfium's own render of the page; print must match it.
                    let want = ctx
                        .render(RenderKey {
                            src: info.id,
                            index: 0,
                            width: w as u16,
                            rot: user_rot as u16,
                            gray: false,
                        })
                        .unwrap();
                    let want = image::load_from_memory(&want).unwrap().to_rgb8();
                    let near =
                        |a: [u8; 3], b: [u8; 3]| a.iter().zip(b).all(|(x, y)| x.abs_diff(y) < 60);
                    // Same relative spots in both renders (sizes may differ by rounding).
                    let at = |im: &image::RgbImage, i: u32, j: u32| {
                        im.get_pixel(im.width() * i / 8, im.height() * j / 8).0
                    };
                    let probes: Vec<(u32, u32)> =
                        (1..8).flat_map(|i| (1..8).map(move |j| (i, j))).collect();
                    let red = probes
                        .iter()
                        .any(|&(i, j)| at(&img, i, j)[0] > 200 && at(&img, i, j)[1] < 60);
                    let differs = want.width().abs_diff(img.width()) > 1
                        || probes
                            .iter()
                            .any(|&(i, j)| !near(at(&img, i, j), at(&want, i, j)));
                    if red || differs {
                        bad.push(format!("rotate {rotation:?} + {user_rot}"));
                    }
                }
            }
            tx.send(bad).unwrap();
        });
        let bad = rx.recv().unwrap();
        assert!(bad.is_empty(), "print differs from preview for: {bad:?}");
    }

    #[test]
    fn compose_two_up_and_render() {
        let Ok(sample) = std::env::var("NBSP_SAMPLE_PDF") else {
            return;
        };
        let out_path = std::env::var("NBSP_OUT").unwrap();
        let engine =
            Engine::start(vec![PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("lib")]).unwrap();
        let (tx, rx) = mpsc::channel();
        engine.send(move |ctx| {
            let info = ctx.open_pdf(Path::new(&sample), "s".into()).unwrap();
            assert!(info.pages.len() >= 2);
            let [w, h] = info.pages[0];
            // A4 landscape, two pages side by side, second one rotated 180.
            let (sw, sh) = (842.0, 595.0);
            let s = (sw / 2.0 / w).min(sh / h);
            let cells = (0..2)
                .map(|i| Cell {
                    src: info.id,
                    index: i,
                    rot: if i == 1 { 180 } else { 0 },
                    x: i as f32 * sw / 2.0,
                    y: 0.0,
                    w: w * s,
                    h: h * s,
                    border: true,
                })
                .collect();
            let bytes = ctx
                .compose(&[Sheet {
                    w: sw,
                    h: sh,
                    cells,
                    lines: vec![],
                    labels: vec![],
                }])
                .unwrap();
            std::fs::write(&out_path, bytes).unwrap();
            let thumb = ctx
                .render(RenderKey {
                    src: info.id,
                    index: 0,
                    width: 300,
                    rot: 90,
                    gray: false,
                })
                .unwrap();
            tx.send(thumb.len()).unwrap();
        });
        assert!(rx.recv().unwrap() > 1000);
    }
}

#[cfg(test)]
mod bench {
    use super::*;
    use std::path::PathBuf;
    use std::time::Instant;

    /// `NBSP_BENCH=a.pdf:b.pdf cargo test --release --lib bench -- --nocapture`
    #[test]
    fn bench_render_and_compose() {
        let Ok(files) = std::env::var("NBSP_BENCH") else {
            return;
        };
        let engine =
            Engine::start(vec![PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("lib")]).unwrap();
        let (tx, rx) = mpsc::channel();
        engine.send(move |ctx| {
            for f in files.split(':') {
                let t = Instant::now();
                let info = ctx.open_pdf(Path::new(f), "b".into()).unwrap();
                println!("{f}: open {} pages in {:?}", info.pages.len(), t.elapsed());
                for width in [160u16, 640, 1280] {
                    let t = Instant::now();
                    let mut bytes = 0;
                    for i in 0..info.pages.len() {
                        bytes += ctx
                            .render(RenderKey {
                                src: info.id,
                                index: i as u16,
                                width,
                                rot: 0,
                                gray: false,
                            })
                            .unwrap()
                            .len();
                    }
                    let per = t.elapsed() / info.pages.len() as u32;
                    println!(
                        "  render w={width}: {per:?}/page, {} KB/page",
                        bytes / info.pages.len() / 1024
                    );
                }
                let t = Instant::now();
                ctx.render(RenderKey {
                    src: info.id,
                    index: 0,
                    width: 640,
                    rot: 0,
                    gray: false,
                })
                .unwrap();
                println!("  cached hit: {:?}", t.elapsed());
                let [w, h] = info.pages[0];
                let sheets: Vec<Sheet> = (0..info.pages.len())
                    .collect::<Vec<_>>()
                    .chunks(2)
                    .map(|c| Sheet {
                        w: 842.0,
                        h: 595.0,
                        cells: c
                            .iter()
                            .enumerate()
                            .map(|(k, &i)| {
                                let s = (421.0 / w).min(595.0 / h);
                                Cell {
                                    src: info.id,
                                    index: i as u16,
                                    rot: 0,
                                    x: k as f32 * 421.0,
                                    y: 0.0,
                                    w: w * s,
                                    h: h * s,
                                    border: false,
                                }
                            })
                            .collect(),
                        lines: vec![],
                        labels: vec![],
                    })
                    .collect();
                let t = Instant::now();
                let out = ctx.compose(&sheets).unwrap();
                println!("  compose 2-up: {:?}, {} KB", t.elapsed(), out.len() / 1024);
            }
            tx.send(()).unwrap();
        });
        rx.recv().unwrap();
    }
}

#[cfg(test)]
mod label_tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn labels_render() {
        let Ok(out_path) = std::env::var("NBSP_LABEL_OUT") else {
            return;
        };
        let engine =
            Engine::start(vec![PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("lib")]).unwrap();
        let (tx, rx) = mpsc::channel();
        engine.send(move |ctx| {
            let l = |text: &str, x, y, angle, align: &str| Label {
                text: text.into(),
                x,
                y,
                size: 24.0,
                angle,
                align: align.into(),
            };
            let sheet = Sheet {
                w: 595.0,
                h: 842.0,
                cells: vec![],
                lines: vec![[297.5, 0.0, 297.5, 842.0], [0.0, 800.0, 595.0, 800.0]],
                labels: vec![
                    l("center 1 / 9", 297.5, 800.0, 0.0, "center"),
                    l("right", 560.0, 60.0, 0.0, "right"),
                    l("turned 270", 60.0, 421.0, 270.0, "center"),
                ],
            };
            std::fs::write(&out_path, ctx.compose(&[sheet]).unwrap()).unwrap();
            tx.send(()).unwrap();
        });
        rx.recv().unwrap();
    }
}
