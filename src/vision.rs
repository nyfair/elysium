use anyhow::{anyhow, Result};
use image::imageops::FilterType::Lanczos3;
use image::{DynamicImage, GrayImage, ImageBuffer, Luma, Rgb, RgbaImage};
use rustfft::num_complex::Complex;
use rustfft::{FftDirection, FftPlanner};
use windows_capture::capture::GraphicsCaptureApiHandler;
use windows_capture::frame::Frame;
use windows_capture::graphics_capture_api::InternalCaptureControl;
use windows_capture::settings::{
    ColorFormat, CursorCaptureSettings, DirtyRegionSettings, DrawBorderSettings,
    MinimumUpdateIntervalSettings, SecondaryWindowSettings, Settings,
};
use windows_capture::window::Window;
use windows::Win32::Foundation::{HWND, LPARAM, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_EXTENDED_FRAME_BOUNDS};
use windows::Win32::Graphics::Gdi::ClientToScreen;
use windows::Win32::UI::WindowsAndMessaging::{GetClientRect, PostMessageW};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crate::k;

pub const W: u32 = 1280;
pub const H: u32 = 720;
pub type AssetMap = HashMap<String, GrayImage>;

static FFT_PLANNER: Mutex<Option<FftPlanner<f64>>> = Mutex::new(None);

struct FrameBuf {
    rgb: Vec<u8>,
    width: u32,
    height: u32,
}

static SHARED: Mutex<Option<Arc<Mutex<FrameBuf>>>> = Mutex::new(None);
static EPOCH: AtomicU64 = AtomicU64::new(0);
static CLIENT: Mutex<(u32, u32, u32, u32)> = Mutex::new((0, 0, W, H));

struct VisionHandler {
    epoch: u64,
}

impl GraphicsCaptureApiHandler for VisionHandler {
    type Flags = (i32, i32);
    type Error = Box<dyn std::error::Error + Send + Sync>;

    fn new(_ctx: windows_capture::capture::Context<(i32, i32)>) -> Result<Self, Self::Error> {
        Ok(Self { epoch: EPOCH.load(Ordering::SeqCst) })
    }

    fn on_frame_arrived(
        &mut self,
        frame: &mut Frame,
        control: InternalCaptureControl,
    ) -> Result<(), Self::Error> {
        if EPOCH.load(Ordering::SeqCst) != self.epoch {
            control.stop();
            return Ok(());
        }
        let (ox, oy, cw, ch) = *k!(CLIENT);
        let fb = frame.buffer_crop(ox, oy, ox + cw, oy + ch)?;
        let w = fb.width();
        let h = fb.height();
        let mut raw = Vec::new();
        let bytes = fb.as_nopadding_buffer(&mut raw);
        let len = (w as usize) * (h as usize) * 3;
        let mut rgb = Vec::with_capacity(len);
        for chunk in bytes.chunks_exact(4) {
            rgb.push(chunk[0]);
            rgb.push(chunk[1]);
            rgb.push(chunk[2]);
        }
        if let Some(buf) = k!(SHARED).as_ref() {
            let mut f = k!(buf);
            f.rgb = rgb;
            f.width = w;
            f.height = h;
        }
        Ok(())
    }
}

pub struct Vision {
    buffer: Arc<Mutex<FrameBuf>>,
}

pub fn activate_window(window: &Window) {
    let hwnd = window.as_raw_hwnd();
    unsafe { let _ = PostMessageW(Some(HWND(hwnd)), 6, WPARAM(1), LPARAM(0)); }
}

impl Vision {
    pub fn start(window: Window) -> Result<Self> {
        EPOCH.fetch_add(1, Ordering::SeqCst);
        let hwnd = window.as_raw_hwnd();
        let mut client_rect = RECT::default();
        unsafe {
            let _ = GetClientRect(HWND(hwnd), &mut client_rect);
        }
        let mut origin = POINT::default();
        unsafe {
            let _ = ClientToScreen(HWND(hwnd), &mut origin);
        }
        let mut win_rect = RECT::default();
        unsafe {
            let _ = DwmGetWindowAttribute(
                HWND(hwnd),
                DWMWA_EXTENDED_FRAME_BOUNDS,
                &mut win_rect as *mut _ as *mut std::ffi::c_void,
                std::mem::size_of::<RECT>() as u32,
            );
        }
        let off_x = (origin.x - win_rect.left).max(0) as u32;
        let off_y = (origin.y - win_rect.top).max(0) as u32;
        let cw = (client_rect.right - client_rect.left) as u32;
        let ch = (client_rect.bottom - client_rect.top) as u32;
        *k!(CLIENT) = (off_x, off_y, cw, ch);
        let buffer = Arc::new(Mutex::new(FrameBuf { rgb: Vec::new(), width: 0, height: 0 }));
        let b = buffer.clone();
        *k!(SHARED) = Some(buffer);
        let settings = Settings::new(
            window,
            CursorCaptureSettings::WithoutCursor,
            DrawBorderSettings::WithoutBorder,
            SecondaryWindowSettings::Default,
            MinimumUpdateIntervalSettings::Default,
            DirtyRegionSettings::Default,
            ColorFormat::Rgba8,
            (W as i32, H as i32),
        );
        thread::Builder::new()
            .name("capture".into())
            .spawn(move || {
                if let Err(e) = VisionHandler::start(settings) {
                    eprintln!("截图出错：{e}");
                }
            })?;
        for _ in 0..200 {
            if !k!(b).rgb.is_empty() {
                return Ok(Self { buffer: b });
            }
            thread::sleep(Duration::from_millis(10));
        }
        anyhow::bail!("等待截图超时")
    }

    pub fn stop(&self) {
        EPOCH.fetch_add(1, Ordering::SeqCst);
    }

    pub fn dimensions(&self) -> (u32, u32) {
        let f = k!(&self.buffer);
        (f.width, f.height)
    }

    pub fn shot(&self) -> Result<ImageBuffer<Rgb<u8>, Vec<u8>>> {
        let f = k!(&self.buffer);
        ImageBuffer::from_raw(f.width, f.height, f.rgb.clone())
            .ok_or_else(|| anyhow!("截图数据为空"))
    }

    pub fn shot_to_file(&self, path: &str) -> Result<()> {
        let f = k!(&self.buffer);
        let img: ImageBuffer<Rgb<u8>, Vec<u8>> = ImageBuffer::from_raw(f.width, f.height, f.rgb.clone())
            .ok_or_else(|| anyhow!("截图数据为空"))?;
        drop(f);
        img.save(path)?;
        Ok(())
    }
}

pub fn load_assets(game: &str, scale_height: u32) -> Result<AssetMap> {
    let path = format!("{game}/coco_annotations.json");
    let file = match std::fs::File::open(&path) {
        Ok(f) => f,
        Err(_) => return Ok(HashMap::new()),
    };
    let scale = scale_height as f64 / 2160.;
    let json: serde_json::Value = serde_json::from_reader(file)?;
    let cats: HashMap<u64, String> = json["categories"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| (c["id"].as_u64().unwrap(), c["name"].as_str().unwrap().to_string()))
        .collect();
    let imgs: HashMap<u64, String> = json["images"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| (i["id"].as_u64().unwrap(), i["file_name"].as_str().unwrap().to_string()))
        .collect();
    let mut img_cache: HashMap<String, DynamicImage> = HashMap::new();
    let mut assets = HashMap::new();

    for ann in json["annotations"].as_array().unwrap() {
        let name = cats.get(&ann["category_id"].as_u64().unwrap()).unwrap();
        let path = imgs.get(&ann["image_id"].as_u64().unwrap()).unwrap();
        if !img_cache.contains_key(path) {
            let img_path = format!("{game}/{path}");
            let raw = image::open(&img_path)?;
            let w = (raw.width() as f64 * scale) as u32;
            let h = (raw.height() as f64 * scale) as u32;
            img_cache.insert(path.clone(), raw.resize_exact(w, h, Lanczos3));
        }
        let x = (ann["bbox"][0].as_f64().unwrap() * scale) as u32;
        let y = (ann["bbox"][1].as_f64().unwrap() * scale) as u32;
        let w = (ann["bbox"][2].as_f64().unwrap() * scale) as u32;
        let h = (ann["bbox"][3].as_f64().unwrap() * scale) as u32;
        let cropped_rgb = img_cache[path.as_str()].crop_imm(x, y, w, h).to_rgb8();
        let gray = ImageBuffer::from_fn(w, h, |i, j| {
            let p = cropped_rgb.get_pixel(i, j);
            Luma([((p[0] as u32 + p[1] as u32 + p[2] as u32) / 3) as u8])
        });
        assets.insert(name.clone(), gray);
    }

    Ok(assets)
}

pub fn get_pixel(img: &ImageBuffer<Rgb<u8>, Vec<u8>>, x: u32, y: u32) -> [u8; 3] {
    let px = img.get_pixel(x, y);
    [px[0], px[1], px[2]]
}

pub fn pixel_equal(img: &ImageBuffer<Rgb<u8>, Vec<u8>>, x: u32, y: u32, r: u8, g: u8, b: u8) -> bool {
    let p = get_pixel(img, x, y);
    p[0] == r && p[1] == g && p[2] == b
}

pub fn pixel_like(img: &ImageBuffer<Rgb<u8>, Vec<u8>>, x: u32, y: u32, r: u8, g: u8, b: u8, v: u8) -> bool {
    let p = get_pixel(img, x, y);
    (p[0] as i16 - r as i16).abs() < v as i16
        && (p[1] as i16 - g as i16).abs() < v as i16
        && (p[2] as i16 - b as i16).abs() < v as i16
}

pub fn ncc_match(
    background: &ImageBuffer<Rgb<u8>, Vec<u8>>,
    template: &GrayImage,
    roi: Option<(u32, u32, u32, u32)>,
) -> (u32, u32, f64) {
    let bg_w = background.width();
    let bg_h = background.height();
    let (rx, ry, rw, rh) = match roi {
        Some((x, y, w, h)) => (x, y, w.min(bg_w.saturating_sub(x)), h.min(bg_h.saturating_sub(y))),
        None => (0, 0, bg_w, bg_h),
    };
    let tw = template.width();
    let th = template.height();
    if rw < tw || rh < th {
        return (0, 0, 0.);
    }

    let mut main = Vec::with_capacity((rw * rh) as usize);
    for y in ry..ry + rh {
        for x in rx..rx + rw {
            let p = background.get_pixel(x, y);
            main.push((p[0] as f32 + p[1] as f32 + p[2] as f32) / 3.);
        }
    }
    let mut find = Vec::with_capacity((tw * th) as usize);
    for p in template.pixels() {
        find.push(p.0[0] as f32);
    }
    let main_norm = normalize(&main);
    let find_norm = normalize(&find);

    let m = next_pow2(rh + th - 1) as usize;
    let n = next_pow2(rw + tw - 1) as usize;
    let mut a = vec![Complex::new(0., 0.); m * n];
    let mut b = vec![Complex::new(0., 0.); m * n];
    let rw_u = rw as usize;
    let tw_u = tw as usize;
    let th_u = th as usize;
    for j in 0..rh as usize {
        for i in 0..rw_u {
            a[j * n + i] = Complex::new(main_norm[j * rw_u + i] as f64, 0.);
        }
    }
    for j in 0..th_u {
        for i in 0..tw_u {
            b[j * n + i] = Complex::new(
                find_norm[(th_u - 1 - j) * tw_u + (tw_u - 1 - i)] as f64,
                0.,
            );
        }
    }

    fft2d(&mut a, m, n, false);
    fft2d(&mut b, m, n, false);
    for i in 0..m * n {
        a[i] *= b[i];
    }
    fft2d(&mut a, m, n, true);
    let inv = 1. / (m * n) as f64;
    for x in a.iter_mut() {
        *x *= inv;
    }

    let oh = (rh - th + 1) as usize;
    let ow = (rw - tw + 1) as usize;
    let mut best = (0usize, 0usize);
    let mut best_val = f64::NEG_INFINITY;
    for j in 0..oh {
        for i in 0..ow {
            let v = a[(j + th_u - 1) * n + (i + tw_u - 1)].re;
            if v > best_val {
                best_val = v;
                best = (i, j);
            }
        }
    }

    let cx = best.0 as u32 + tw / 2 + rx;
    let cy = best.1 as u32 + th / 2 + ry;
    let score = best_val / (tw * th) as f64;
    (cx, cy, score)
}

fn next_pow2(n: u32) -> u32 {
    let mut v = n - 1;
    v |= v >> 1;
    v |= v >> 2;
    v |= v >> 4;
    v |= v >> 8;
    v |= v >> 16;
    v + 1
}

fn normalize(data: &[f32]) -> Vec<f32> {
    let n = data.len() as f32;
    let mean = data.iter().sum::<f32>() / n;
    let var = data.iter().map(|x| (x - mean).powi(2)).sum::<f32>() / n;
    let std = var.sqrt() + 1e-8;
    data.iter().map(|x| (x - mean) / std).collect()
}

fn fft2d(data: &mut [Complex<f64>], rows: usize, cols: usize, inverse: bool) {
    let direction = if inverse {
        FftDirection::Inverse
    } else {
        FftDirection::Forward
    };
    let mut planner = k!(FFT_PLANNER);
    let planner = planner.get_or_insert_with(FftPlanner::new);
    {
        let fft = planner.plan_fft(cols, direction);
        for r in 0..rows {
            fft.process(&mut data[r * cols..(r + 1) * cols]);
        }
    }
    {
        let fft = planner.plan_fft(rows, direction);
        let mut col = vec![Complex::new(0., 0.); rows];
        for c in 0..cols {
            for r in 0..rows {
                col[r] = data[r * cols + c];
            }
            fft.process(&mut col);
            for r in 0..rows {
                data[r * cols + c] = col[r];
            }
        }
    }
}

const GRAY_WEIGHT: f32 = 1.;
const HSV_WEIGHT: f32 = 0.6;

pub struct TopMatch {
    pub name: String,
    pub file: String,
    pub score: f32,
}

pub struct MatchReport {
    pub var: f32,
    pub top: Vec<TopMatch>,
    pub verdict: Option<(String, f32)>,
}

struct RefItem {
    name: String,
    file: String,
    vec: Vec<f32>,
}

pub struct TemplateSet {
    size: u32,
    var_min: f32,
    score_min: f32,
    score_gap: f32,
    refs: Vec<RefItem>,
    query_mask: GrayImage,
}

impl TemplateSet {
    pub fn new(
        size: u32,
        radius_ratio: f32,
        var_min: f32,
        score_min: f32,
        score_gap: f32,
    ) -> Self {
        Self {
            size,
            var_min,
            score_min,
            score_gap,
            refs: Vec::new(),
            query_mask: circle_mask(size, radius_ratio),
        }
    }

    pub fn add_alpha_ref(&mut self, name: &str, file: &str, img: &RgbaImage) {
        let resized = image::imageops::resize(
            img,
            self.size,
            self.size,
            image::imageops::FilterType::Triangle,
        );
        let mask = GrayImage::from_fn(self.size, self.size, |x, y| {
            Luma([if resized.get_pixel(x, y).0[3] > 128 { 255 } else { 0 }])
        });
        self.refs.push(RefItem {
            name: name.to_string(),
            file: file.to_string(),
            vec: feature(&resized, &mask),
        });
    }

    pub fn is_empty(&self) -> bool {
        self.refs.is_empty()
    }

    pub fn match_roi(
        &self,
        frame: &ImageBuffer<Rgb<u8>, Vec<u8>>,
        roi: (u32, u32, u32, u32),
    ) -> Option<MatchReport> {
        let (x, y, w, h) = roi;
        if x + w > frame.width() || y + h > frame.height() {
            return None;
        }
        let crop = image::imageops::crop_imm(frame, x, y, w, h).to_image();
        let q = DynamicImage::ImageRgb8(crop)
            .resize_exact(self.size, self.size, image::imageops::FilterType::Triangle)
            .to_rgba8();
        let (_, var) = masked_gray_stats(&q, &self.query_mask);
        let qv = feature(&q, &self.query_mask);

        let mut scored: Vec<(usize, f32)> = self
            .refs
            .iter()
            .enumerate()
            .map(|(i, r)| (i, cosine(&qv, &r.vec)))
            .collect();
        scored.sort_by(|a, b| b.1.total_cmp(&a.1));

        let mut top = Vec::new();
        for &(i, s) in scored.iter().take(3) {
            top.push(TopMatch {
                name: self.refs[i].name.clone(),
                file: self.refs[i].file.clone(),
                score: s,
            });
        }

        let verdict = if var < self.var_min || scored.is_empty() {
            None
        } else {
            let (i, s) = scored[0];
            let top2 = scored.get(1).map(|x| x.1).unwrap_or(0.);
            if s < self.score_min || s - top2 < self.score_gap {
                None
            } else {
                Some((self.refs[i].name.clone(), s))
            }
        };
        Some(MatchReport { var, top, verdict })
    }
}

fn feature(rgba: &RgbaImage, mask: &GrayImage) -> Vec<f32> {
    let (w, h) = rgba.dimensions();
    let n = (w * h) as usize;
    let mut gray = vec![0f32; n];
    for (x, y, p) in rgba.enumerate_pixels() {
        if mask.get_pixel(x, y).0[0] > 0 {
            let i = (y * w + x) as usize;
            gray[i] = (p.0[0] as f32 * 0.299 + p.0[1] as f32 * 0.587 + p.0[2] as f32 * 0.114)
                / 255.;
        }
    }
    l2_normalize(&mut gray);
    let mut hist = hsv_hist(rgba, mask);
    l2_normalize(&mut hist);
    let mut v = Vec::with_capacity(n + hist.len());
    v.extend(gray.into_iter().map(|x| x * GRAY_WEIGHT));
    v.extend(hist.into_iter().map(|x| x * HSV_WEIGHT));
    v
}

fn circle_mask(size: u32, radius_ratio: f32) -> GrayImage {
    let c = (size as f32 / 2.) as i32;
    let r = (size as f32 * radius_ratio) as i32;
    GrayImage::from_fn(size, size, |x, y| {
        let dx = x as i32 - c;
        let dy = y as i32 - c;
        Luma([if dx * dx + dy * dy <= r * r { 255 } else { 0 }])
    })
}

fn masked_gray_stats(rgba: &RgbaImage, mask: &GrayImage) -> (f32, f32) {
    let mut sum = 0.;
    let mut sum2 = 0.;
    let mut cnt = 0.;
    for (x, y, p) in rgba.enumerate_pixels() {
        if mask.get_pixel(x, y).0[0] > 0 {
            let g = (p.0[0] as f32 * 0.299 + p.0[1] as f32 * 0.587 + p.0[2] as f32 * 0.114)
                as f64
                / 255.;
            sum += g;
            sum2 += g * g;
            cnt += 1.;
        }
    }
    if cnt == 0. {
        return (0., 0.);
    }
    let mean = sum / cnt;
    (mean as f32, (sum2 / cnt - mean * mean) as f32)
}

fn hsv_hist(img: &RgbaImage, mask: &GrayImage) -> Vec<f32> {
    const H_BINS: usize = 16;
    const S_BINS: usize = 8;
    const V_BINS: usize = 8;
    let mut hist = vec![0f32; H_BINS * S_BINS * V_BINS];
    for (x, y, p) in img.enumerate_pixels() {
        if mask.get_pixel(x, y).0[0] == 0 {
            continue;
        }
        let (h, s, v) = rgb_to_hsv(p.0[0], p.0[1], p.0[2]);
        let hi = ((h / 360.) * H_BINS as f32).min(H_BINS as f32 - 1.) as usize;
        let si = (s * S_BINS as f32).min(S_BINS as f32 - 1.) as usize;
        let vi = (v * V_BINS as f32).min(V_BINS as f32 - 1.) as usize;
        hist[(hi * S_BINS + si) * V_BINS + vi] += 1.;
    }
    hist
}

fn rgb_to_hsv(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let (r, g, b) = (r as f32 / 255., g as f32 / 255., b as f32 / 255.);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let h = if d == 0. {
        0.
    } else if max == r {
        60. * ((g - b) / d).rem_euclid(6.)
    } else if max == g {
        60. * ((b - r) / d + 2.)
    } else {
        60. * ((r - g) / d + 4.)
    };
    let s = if max == 0. { 0. } else { d / max };
    (h, s, max)
}

fn l2_normalize(v: &mut [f32]) {
    let norm = v.iter().map(|x| (x * x) as f64).sum::<f64>().sqrt() as f32;
    if norm > 1e-6 {
        for x in v.iter_mut() {
            *x /= norm;
        }
    }
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let mut dot = 0f64;
    let mut na = 0f64;
    let mut nb = 0f64;
    for (x, y) in a.iter().zip(b) {
        dot += (*x as f64) * (*y as f64);
        na += (*x as f64) * (*x as f64);
        nb += (*y as f64) * (*y as f64);
    }
    if na == 0. || nb == 0. {
        return 0.;
    }
    (dot / (na.sqrt() * nb.sqrt())) as f32
}
