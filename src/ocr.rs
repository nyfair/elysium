use anyhow::{anyhow, bail, Context, Result};
use image::{DynamicImage, ImageBuffer, Rgb, Rgba};
use windows_link::link;
use std::ffi::{CStr, CString, c_char, c_void, OsStr};
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, OnceLock};
use std::thread::{self, JoinHandle};

link!("kernel32.dll" "system" fn LoadLibraryExW(path: *const u16, file: *mut c_void, flags: u32) -> *mut c_void);
link!("kernel32.dll" "system" fn GetProcAddress(module: *mut c_void, name: *const u8) -> *const c_void);
link!("kernel32.dll" "system" fn GetLastError() -> u32);

const LOAD_WITH_ALTERED_SEARCH_PATH: u32 = 0x8;
const MODEL_FILE: &str = "oneocr.onemodel";
const MODEL_KEY: &str = r#"kj)TGtrK>f]b[Piow.gU+nC@s""""""4"#;

pub struct OcrLine {
    pub text: String,
    pub score: f32,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

enum Msg {
    Run(ImageBuffer<Rgba<u8>, Vec<u8>>, Sender<Result<Vec<OcrLine>, String>>),
}

pub struct Ocr {
    tx: Option<Sender<Msg>>,
    handle: Option<JoinHandle<()>>,
}

static GLOBAL: OnceLock<Arc<Ocr>> = OnceLock::new();

impl Ocr {
    pub fn global() -> Result<Arc<Ocr>> {
        if let Some(o) = GLOBAL.get() {
            return Ok(o.clone());
        }
        let o = Arc::new(Ocr::new()?);
        match GLOBAL.set(o.clone()) {
            Ok(()) => Ok(o),
            Err(existing) => Ok(existing.clone()),
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Img {
    t: i32,
    col: i32,
    row: i32,
    _unk: i32,
    step: i64,
    data_ptr: i64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct RawBBox {
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
    x3: f32,
    y3: f32,
    x4: f32,
    y4: f32,
}

struct OcrFns {
    _module: *mut c_void,
    create_ocr_init_options: unsafe extern "C" fn(*mut i64) -> i64,
    ocr_init_options_set_use_model_delay_load: unsafe extern "C" fn(i64, u8) -> i64,
    create_ocr_pipeline: unsafe extern "C" fn(*const c_char, *const c_char, i64, *mut i64) -> i64,
    create_ocr_process_options: unsafe extern "C" fn(*mut i64) -> i64,
    ocr_process_options_set_max_recognition_line_count: unsafe extern "C" fn(i64, i64) -> i64,
    run_ocr_pipeline: unsafe extern "C" fn(i64, *const Img, i64, *mut i64) -> i64,
    get_ocr_line_count: unsafe extern "C" fn(i64, *mut i64) -> i64,
    get_ocr_line: unsafe extern "C" fn(i64, i64, *mut i64) -> i64,
    get_ocr_line_content: unsafe extern "C" fn(i64, *mut i64) -> i64,
    get_ocr_line_bounding_box: unsafe extern "C" fn(i64, *mut i64) -> i64,
    get_ocr_line_word_count: unsafe extern "C" fn(i64, *mut i64) -> i64,
    get_ocr_word: unsafe extern "C" fn(i64, i64, *mut i64) -> i64,
    get_ocr_word_confidence: unsafe extern "C" fn(i64, *mut f32) -> i64,
}

unsafe impl Send for OcrFns {}
unsafe impl Sync for OcrFns {}

static FNS: OnceLock<Result<OcrFns, String>> = OnceLock::new();

fn find_package_dir() -> Result<PathBuf> {
    let program_files = std::env::var("ProgramFiles").unwrap_or_else(|_| "C:\\Program Files".into());
    let windows_apps = PathBuf::from(program_files).join("WindowsApps");
    let rd = std::fs::read_dir(&windows_apps)
        .with_context(|| format!("无法读取 {}（需要管理员权限）", windows_apps.display()))?;
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if name.starts_with("Microsoft.ScreenSketch_") {
            return Ok(e.path());
        }
    }
    anyhow::bail!("未找到Windows OCR工具，请确认系统为 Win10/11 预装截图工具未卸载");
}

fn find_files_dir(pkg: &Path) -> Result<PathBuf> {
    for sub in ["", "SnippingTool"] {
        let dir = if sub.is_empty() {
            pkg.to_path_buf()
        } else {
            pkg.join(sub)
        };
        if dir.join("oneocr.dll").exists() && dir.join(MODEL_FILE).exists() {
            return Ok(dir);
        }
    }
    anyhow::bail!(
        "在 {} 下未找到 oneocr.dll/{}",
        pkg.display(),
        MODEL_FILE
    )
}

fn ensure_ocr_files() -> Result<PathBuf> {
    let exe_dir = std::env::current_exe()?
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_default();
    if exe_dir.join("oneocr.dll").exists() && exe_dir.join(MODEL_FILE).exists() {
        return Ok(exe_dir);
    }
    let pkg = find_package_dir()?;
    let src = find_files_dir(&pkg)?;
    let cache = std::env::temp_dir().join("oneocr");
    if cache.join("oneocr.dll").exists() && cache.join(MODEL_FILE).exists() {
        return Ok(cache);
    }
    std::fs::create_dir_all(&cache)?;
    for f in [MODEL_FILE, "oneocr.dll", "onnxruntime.dll"] {
        let dst = cache.join(f);
        if !dst.exists() {
            std::fs::copy(src.join(f), &dst)
                .with_context(|| format!("复制 {} 失败", src.join(f).display()))?;
        }
    }
    Ok(cache)
}

fn load_fns() -> Result<&'static OcrFns> {
    let fns = FNS.get_or_init(|| {
        let dir = ensure_ocr_files().map_err(|e| format!("{e:#}"))?;
        let dll = dir.join("oneocr.dll");
        let wide: Vec<u16> = OsStr::new(&dll).encode_wide().chain(std::iter::once(0)).collect();
        let module = unsafe {
            LoadLibraryExW(wide.as_ptr(), std::ptr::null_mut(), LOAD_WITH_ALTERED_SEARCH_PATH)
        };
        if module.is_null() {
            let err = unsafe { GetLastError() };
            return Err(format!("加载 oneocr.dll 失败：{}（错误码 {err}）", dll.display()));
        }
        macro_rules! proc {
            ($name:literal, $t:ty) => {
                unsafe {
                    get_proc::<$t>(module, concat!($name, "\0").as_bytes())
                        .ok_or_else(|| format!("oneocr.dll 缺少导出函数 {}", $name))?
                }
            };
        }
        Ok(OcrFns {
            _module: module,
            create_ocr_init_options: proc!("CreateOcrInitOptions", unsafe extern "C" fn(*mut i64) -> i64),
            ocr_init_options_set_use_model_delay_load: proc!("OcrInitOptionsSetUseModelDelayLoad", unsafe extern "C" fn(i64, u8) -> i64),
            create_ocr_pipeline: proc!("CreateOcrPipeline", unsafe extern "C" fn(*const c_char, *const c_char, i64, *mut i64) -> i64),
            create_ocr_process_options: proc!("CreateOcrProcessOptions", unsafe extern "C" fn(*mut i64) -> i64),
            ocr_process_options_set_max_recognition_line_count: proc!("OcrProcessOptionsSetMaxRecognitionLineCount", unsafe extern "C" fn(i64, i64) -> i64),
            run_ocr_pipeline: proc!("RunOcrPipeline", unsafe extern "C" fn(i64, *const Img, i64, *mut i64) -> i64),
            get_ocr_line_count: proc!("GetOcrLineCount", unsafe extern "C" fn(i64, *mut i64) -> i64),
            get_ocr_line: proc!("GetOcrLine", unsafe extern "C" fn(i64, i64, *mut i64) -> i64),
            get_ocr_line_content: proc!("GetOcrLineContent", unsafe extern "C" fn(i64, *mut i64) -> i64),
            get_ocr_line_bounding_box: proc!("GetOcrLineBoundingBox", unsafe extern "C" fn(i64, *mut i64) -> i64),
            get_ocr_line_word_count: proc!("GetOcrLineWordCount", unsafe extern "C" fn(i64, *mut i64) -> i64),
            get_ocr_word: proc!("GetOcrWord", unsafe extern "C" fn(i64, i64, *mut i64) -> i64),
            get_ocr_word_confidence: proc!("GetOcrWordConfidence", unsafe extern "C" fn(i64, *mut f32) -> i64),
        })
    });
    match fns {
        Ok(f) => Ok(f),
        Err(e) => Err(anyhow!("{e}")),
    }
}

fn fns() -> &'static OcrFns {
    FNS.get()
        .and_then(|r| r.as_ref().ok())
        .expect("oneocr fns not loaded")
}

unsafe fn get_proc<T>(module: *mut c_void, name: &[u8]) -> Option<T> {
    let p = unsafe { GetProcAddress(module, name.as_ptr()) };
    if p.is_null() {
        None
    } else {
        Some(unsafe { std::mem::transmute_copy::<usize, T>(&(p as usize)) })
    }
}

struct Engine {
    pipeline: i64,
    opt: i64,
}

fn create_engine() -> Result<Engine> {
    let fns = load_fns()?;
    let mut ctx: i64 = 0;
    let res = unsafe { (fns.create_ocr_init_options)(&mut ctx) };
    if res != 0 {
        bail!("CreateOcrInitOptions 失败：{res}");
    }
    let res = unsafe { (fns.ocr_init_options_set_use_model_delay_load)(ctx, 0) };
    if res != 0 {
        bail!("OcrInitOptionsSetUseModelDelayLoad 失败：{res}");
    }
    let dir = ensure_ocr_files()?;
    let model = dir.join(MODEL_FILE);
    let model_cstr = CString::new(model.to_string_lossy().into_owned())
        .context("模型路径包含 NUL")?;
    let key_cstr = CString::new(MODEL_KEY).context("模型密钥包含 NUL")?;
    let mut pipeline: i64 = 0;
    let res = unsafe {
        (fns.create_ocr_pipeline)(model_cstr.as_ptr(), key_cstr.as_ptr(), ctx, &mut pipeline)
    };
    if res != 0 {
        bail!("CreateOcrPipeline 失败：{res}（模型文件与密钥不匹配？）");
    }
    let mut opt: i64 = 0;
    let res = unsafe { (fns.create_ocr_process_options)(&mut opt) };
    if res != 0 {
        bail!("CreateOcrProcessOptions 失败：{res}");
    }
    let res = unsafe { (fns.ocr_process_options_set_max_recognition_line_count)(opt, 1000) };
    if res != 0 {
        bail!("OcrProcessOptionsSetMaxRecognitionLineCount 失败：{res}");
    }
    Ok(Engine { pipeline, opt })
}

fn run_ocr(fns: &OcrFns, engine: &Engine, img: &ImageBuffer<Rgba<u8>, Vec<u8>>) -> Result<Vec<OcrLine>> {
    let image = Img {
        t: 3,
        col: img.width() as i32,
        row: img.height() as i32,
        _unk: 0,
        step: img.sample_layout().height_stride as i64,
        data_ptr: img.as_ptr() as i64,
    };
    let mut instance: i64 = 0;
    let res = unsafe { (fns.run_ocr_pipeline)(engine.pipeline, &image, engine.opt, &mut instance) };
    if res != 0 {
        bail!("RunOcrPipeline 失败：{res}");
    }
    let mut lc: i64 = 0;
    let res = unsafe { (fns.get_ocr_line_count)(instance, &mut lc) };
    if res != 0 {
        bail!("GetOcrLineCount 失败：{res}");
    }
    let mut lines = Vec::new();
    for i in 0..lc {
        let mut line: i64 = 0;
        if unsafe { (fns.get_ocr_line)(instance, i, &mut line) } != 0 || line == 0 {
            continue;
        }
        let mut content: i64 = 0;
        let text = if unsafe { (fns.get_ocr_line_content)(line, &mut content) } == 0 && content != 0 {
            unsafe { CStr::from_ptr(content as *const c_char) }
                .to_string_lossy()
                .into_owned()
        } else {
            String::new()
        };
        let mut bbox: i64 = 0;
        let (x, y, w, h) =
            if unsafe { (fns.get_ocr_line_bounding_box)(line, &mut bbox) } == 0 && bbox != 0 {
                let rb = unsafe { &*(bbox as *const RawBBox) };
                let xs = [rb.x1, rb.x2, rb.x3, rb.x4];
                let ys = [rb.y1, rb.y2, rb.y3, rb.y4];
                let min_x = xs.iter().cloned().fold(f32::INFINITY, f32::min);
                let max_x = xs.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
                let min_y = ys.iter().cloned().fold(f32::INFINITY, f32::min);
                let max_y = ys.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
                (min_x, min_y, max_x - min_x, max_y - min_y)
            } else {
                (0., 0., 0., 0.)
            };
        let mut wc: i64 = 0;
        let mut score = 0.;
        if unsafe { (fns.get_ocr_line_word_count)(line, &mut wc) } == 0 && wc > 0 {
            let mut sum = 0.;
            for j in 0..wc {
                let mut word: i64 = 0;
                if unsafe { (fns.get_ocr_word)(line, j, &mut word) } != 0 || word == 0 {
                    continue;
                }
                let mut conf: f32 = 0.;
                if unsafe { (fns.get_ocr_word_confidence)(word, &mut conf) } == 0 {
                    sum += conf;
                }
            }
            score = sum / wc as f32;
        }
        lines.push(OcrLine { text, score, x, y, w, h });
    }
    Ok(lines)
}

impl Ocr {
    pub fn new() -> Result<Self> {
        let (tx, rx) = channel::<Msg>();
        let (init_tx, init_rx) = channel();
        let handle = thread::Builder::new()
            .name("ocr".into())
            .spawn(move || {
                let engine = match create_engine() {
                    Ok(e) => e,
                    Err(e) => {
                        let _ = init_tx.send(Err(format!("{e:#}")));
                        return;
                    }
                };
                let _ = init_tx.send(Ok(()));
                for msg in rx {
                    match msg {
                        Msg::Run(img, resp) => {
                            let r = run_ocr(fns(), &engine, &img).map_err(|e| format!("{e:#}"));
                            let _ = resp.send(r);
                        }
                    }
                }
            })?;
        init_rx
            .recv()
            .map_err(|_| anyhow!("OCR 线程意外退出"))?
            .map_err(anyhow::Error::msg)?;
        Ok(Self { tx: Some(tx), handle: Some(handle) })
    }

    pub fn recognize_roi(
        &self,
        frame: &ImageBuffer<Rgb<u8>, Vec<u8>>,
        roi: (u32, u32, u32, u32),
        scale: f32,
    ) -> Result<Vec<OcrLine>> {
        let (x, y, w, h) = roi;
        if x + w > frame.width() || y + h > frame.height() {
            anyhow::bail!("OCR ROI 越界：({x},{y},{w},{h})");
        }
        let crop = image::imageops::crop_imm(frame, x, y, w, h).to_image();
        let img = DynamicImage::ImageRgb8(crop)
            .resize_exact(
                (w as f32 * scale) as u32,
                (h as f32 * scale) as u32,
                image::imageops::FilterType::Triangle,
            )
            .to_rgba8();
        let (tx, rx) = channel();
        self.tx.as_ref().unwrap().send(Msg::Run(img, tx))?;
        let mut lines = rx
            .recv()
            .map_err(|_| anyhow!("OCR 线程已退出"))?
            .map_err(anyhow::Error::msg)?;
        if scale != 1. {
            for l in &mut lines {
                l.x /= scale;
                l.y /= scale;
                l.w /= scale;
                l.h /= scale;
            }
        }
        Ok(lines)
    }
}

impl Drop for Ocr {
    fn drop(&mut self) {
        self.tx.take();
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}
