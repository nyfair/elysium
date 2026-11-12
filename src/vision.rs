use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use fast_image_resize::{images::Image, FilterType, IntoImageView, PixelType, ResizeAlg, Resizer,};
use image::{DynamicImage, GrayImage, ImageBuffer, Luma, Rgb, RgbImage, RgbaImage};
use rustfft::num_complex::Complex;
use rustfft::{FftDirection, FftPlanner};
use windows::Win32::Foundation::{HWND, LPARAM, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_EXTENDED_FRAME_BOUNDS};
use windows::Win32::Graphics::Gdi::ClientToScreen;
use windows::Win32::UI::WindowsAndMessaging::{
    GetClientRect, IsIconic, PostMessageW, SW_RESTORE, SW_SHOW, SetForegroundWindow, ShowWindow,
};
use windows_capture::capture::GraphicsCaptureApiHandler;
use windows_capture::frame::Frame;
use windows_capture::graphics_capture_api::InternalCaptureControl;
use windows_capture::settings::{
    ColorFormat, CursorCaptureSettings, DirtyRegionSettings, DrawBorderSettings,
    MinimumUpdateIntervalSettings, SecondaryWindowSettings, Settings,
};
use windows_capture::window::Window;

use crate::{k, log_error};

const TEMPLATE_BASE_HEIGHT: f64 = 2160.;

static TEMPLATES: Mutex<Option<HashMap<(PathBuf, u32, Option<(u32, u32, u32, u32)>, Option<(u32, u32)>),
    Arc<RgbImage>>>> = Mutex::new(None);
static FFT_PLANNER: Mutex<Option<FftPlanner<f64>>> = Mutex::new(None);
static SHOT_TRIGGER: AtomicBool = AtomicBool::new(false);
static FRAME_SEQ: AtomicU64 = AtomicU64::new(0);
static SHARED: Mutex<Option<Arc<Mutex<FrameBuf>>>> = Mutex::new(None);
static EPOCH: AtomicU64 = AtomicU64::new(0);

pub const BASE_WIDTH: f64 = 1280.;
pub const BASE_HEIGHT: f64 = 720.;

pub fn fast_resize(img: &DynamicImage, new_w: u32, new_h: u32, alg: ResizeAlg) -> DynamicImage {
    let converted: DynamicImage;
    let (src, pixel_type) = match img.pixel_type() {
        Some(PixelType::U8x3) => (img, PixelType::U8x3),
        Some(PixelType::U8x4) => (img, PixelType::U8x4),
        _ => {
            converted = DynamicImage::ImageRgb8(img.to_rgb8());
            (&converted, PixelType::U8x3)
        }
    };
    let mut dst = Image::new(new_w, new_h, pixel_type);
    let mut resizer = Resizer::new();
    let options = fast_image_resize::ResizeOptions::new().resize_alg(alg);
    resizer.resize(src, &mut dst, Some(&options)).expect("resize 失败");
    match pixel_type {
        PixelType::U8x3 => DynamicImage::ImageRgb8(
            ImageBuffer::from_raw(new_w, new_h, dst.into_vec()).expect("RGB 尺寸不匹配"),
        ),
        _ => DynamicImage::ImageRgba8(
            RgbaImage::from_raw(new_w, new_h, dst.into_vec()).expect("RGBA 尺寸不匹配"),
        ),
    }
}

pub fn scale_coords(img_w: u32, img_h: u32, x: u32, y: u32) -> (u32, u32) {
    let scale_x = img_w as f64 / BASE_WIDTH;
    let scale_y = img_h as f64 / BASE_HEIGHT;
    let real_x = ((x as f64) * scale_x).round() as u32;
    let real_y = ((y as f64) * scale_y).round() as u32;
    (real_x.min(img_w.saturating_sub(1)), real_y.min(img_h.saturating_sub(1)))
}

pub fn scale_roi(img_w: u32, img_h: u32, roi: (u32, u32, u32, u32)) -> (u32, u32, u32, u32) {
    let scale_x = img_w as f64 / BASE_WIDTH;
    let scale_y = img_h as f64 / BASE_HEIGHT;
    let rx = ((roi.0 as f64) * scale_x).round() as u32;
    let ry = ((roi.1 as f64) * scale_y).round() as u32;
    let rw = ((roi.2 as f64) * scale_x).round() as u32;
    let rh = ((roi.3 as f64) * scale_y).round() as u32;
    (rx, ry, rw, rh)
}

struct FrameBuf {
    rgba: Vec<u8>,
    width: u32,
    height: u32,
}

struct VisionHandler {
    epoch: u64,
    off_x: u32,
    off_y: u32,
    cw: u32,
    ch: u32,
    raw: Vec<u8>,
}

impl GraphicsCaptureApiHandler for VisionHandler {
    type Flags = (i32, i32, i32, i32);
    type Error = Box<dyn std::error::Error + Send + Sync>;

    fn new(ctx: windows_capture::capture::Context<(i32, i32, i32, i32)>) -> Result<Self, Self::Error> {
        let (off_x, off_y, cw, ch) = ctx.flags;
        Ok(Self {
            epoch: EPOCH.load(Ordering::SeqCst),
            off_x: off_x.max(0) as u32,
            off_y: off_y.max(0) as u32,
            cw: cw.max(0) as u32,
            ch: ch.max(0) as u32,
            raw: Vec::new(),
        })
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
        if SHOT_TRIGGER.swap(false, Ordering::AcqRel) {
            let fb = frame.buffer()?;
            let w = fb.width();
            let h = fb.height();
            let (cw, ch, off_x, off_y) = if self.cw > 0
                && self.ch > 0
                && self.off_x + self.cw <= w
                && self.off_y + self.ch <= h
            {
                (self.cw, self.ch, self.off_x, self.off_y)
            } else {
                (w, h, 0, 0)
            };
            let bytes = fb.as_nopadding_buffer(&mut self.raw);
            if let Some(buf) = k!(SHARED).as_ref() {
                let mut f = k!(buf);
                f.rgba.clear();
                for y in off_y..off_y + ch {
                    let s = ((y * w + off_x) * 4) as usize;
                    let e = s + (cw as usize) * 4;
                    f.rgba.extend_from_slice(&bytes[s..e]);
                }
                f.width = cw;
                f.height = ch;
            }
            FRAME_SEQ.fetch_add(1, Ordering::Release);
        }
        Ok(())
    }
}

pub struct Vision {
    buffer: Arc<Mutex<FrameBuf>>,
}

fn to_rgb_image(f: &FrameBuf) -> Result<ImageBuffer<Rgb<u8>, Vec<u8>>> {
    if f.width == 0 || f.height == 0 || f.rgba.is_empty() {
        anyhow::bail!("截图数据为空");
    }
    let mut rgb = Vec::with_capacity((f.width as usize) * (f.height as usize) * 3);
    for px in f.rgba.as_chunks::<4>().0 {
        rgb.push(px[0]);
        rgb.push(px[1]);
        rgb.push(px[2]);
    }
    ImageBuffer::from_raw(f.width, f.height, rgb).ok_or_else(|| anyhow!("截图数据为空"))
}

pub fn activate_window(window: &Window, show: bool) {
    let hwnd = HWND(window.as_raw_hwnd());
    unsafe {
        if show {
            if IsIconic(hwnd).as_bool() {
                let _ = ShowWindow(hwnd, SW_RESTORE);
            } else {
                let _ = ShowWindow(hwnd, SW_SHOW);
            }
            let _ = SetForegroundWindow(hwnd);
        } else {
            let _ = PostMessageW(Some(hwnd), 6, WPARAM(1), LPARAM(0));
        }
    }
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
        let buffer = Arc::new(Mutex::new(FrameBuf { rgba: Vec::new(), width: 0, height: 0 }));
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
            (off_x as i32, off_y as i32, cw as i32, ch as i32),
        );
        thread::Builder::new()
            .name("capture".into())
            .spawn(move || {
                if let Err(e) = VisionHandler::start(settings) {
                    log_error!("截图出错：{}", e);
                }
            })?;
        SHOT_TRIGGER.store(true, Ordering::Release);
        for _ in 0..200 {
            if !k!(b).rgba.is_empty() {
                return Ok(Self { buffer: b });
            }
            thread::sleep(Duration::from_millis(10));
        }
        anyhow::bail!("等待截图超时")
    }

    pub fn stop(&self) {
        EPOCH.fetch_add(1, Ordering::SeqCst);
    }

    pub fn shot(&self) -> Result<ImageBuffer<Rgb<u8>, Vec<u8>>> {
        let _ = self.wait_frame(Duration::from_millis(200));
        let f = k!(&self.buffer);
        to_rgb_image(&f)
    }

    pub fn shot_to_file(&self, path: &str) -> Result<()> {
        let _ = self.wait_frame(Duration::from_millis(200));
        let f = k!(&self.buffer);
        let img = to_rgb_image(&f)?;
        drop(f);
        img.save(path)?;
        Ok(())
    }

    pub fn get_dimension(&self) -> (u32, u32) {
        let buf = k!(self.buffer);
        (buf.width, buf.height)
    }

    fn wait_frame(&self, timeout: Duration) -> Result<()> {
        let current_seq = FRAME_SEQ.load(Ordering::Acquire);
        SHOT_TRIGGER.store(true, Ordering::Release);
        let start = std::time::Instant::now();
        while start.elapsed() < timeout {
            if FRAME_SEQ.load(Ordering::Acquire) != current_seq {
                return Ok(());
            }
            thread::sleep(Duration::from_millis(2));
        }
        anyhow::bail!("等待截图响应超时")
    }
}

pub fn load_template_ex(
    image_dir: &str,
    name: &str,
    height: u32,
    crop: Option<(u32, u32, u32, u32)>,
    template_size: Option<(u32, u32)>,
) -> Result<Arc<RgbImage>> {
    let file = Path::new(name);
    let file = if file.extension().is_some() {
        file.to_path_buf()
    } else {
        file.with_extension("png")
    };
    let path = Path::new(image_dir).join(file);
    let key = (path.clone(), height, crop, template_size);
    if let Some(tpl) = k!(TEMPLATES).as_ref().and_then(|m| m.get(&key)).cloned() {
        return Ok(tpl);
    }

    let raw = image::open(&path).with_context(|| format!("读取模板失败：{}", path.display()))?;
    let raw_w = raw.width() as f64;
    let raw_h = raw.height() as f64;
    let cropped = if let Some((cx, cy, cw, ch)) = crop {
        raw.crop_imm(cx, cy, cw, ch)
    } else if raw.color().has_alpha() {
        let rgba = raw.to_rgba8();
        let mut min_x = u32::MAX;
        let mut min_y = u32::MAX;
        let mut max_x = 0u32;
        let mut max_y = 0u32;
        for (x, y, p) in rgba.enumerate_pixels() {
            if p[3] > 0 {
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
        }
        if min_x <= max_x && min_y <= max_y {
            raw.crop_imm(min_x, min_y, max_x - min_x + 1, max_y - min_y + 1)
        } else {
            raw
        }
    } else {
        raw
    };

    let rgb_raw = if cropped.color().has_alpha() {
        let rgba = cropped.to_rgba8();
        let premultiplied: RgbImage = ImageBuffer::from_fn(rgba.width(), rgba.height(), |x, y| {
            let p = rgba.get_pixel(x, y);
            let a = p[3] as u32;
            Rgb([
                ((p[0] as u32 * a) / 255) as u8,
                ((p[1] as u32 * a) / 255) as u8,
                ((p[2] as u32 * a) / 255) as u8,
            ])
        });
        let (mut min, mut max) = (u8::MAX, 0u8);
        for p in premultiplied.pixels() {
            for c in p.0 {
                min = min.min(c);
                max = max.max(c);
            }
        }
        if min == max {
            DynamicImage::ImageRgb8(ImageBuffer::from_fn(rgba.width(), rgba.height(), |x, y| {
                let a = rgba.get_pixel(x, y)[3];
                Rgb([a, a, a])
            }))
        } else {
            DynamicImage::ImageRgb8(premultiplied)
        }
    } else {
        cropped
    };

    let (final_w, final_h) = if let Some((tw, th)) = template_size {
        let s = height as f64 / BASE_HEIGHT;
        let sx = tw as f64 * s / raw_w;
        let sy = th as f64 * s / raw_h;
        (
            ((rgb_raw.width() as f64 * sx).round() as u32).max(1),
            ((rgb_raw.height() as f64 * sy).round() as u32).max(1),
        )
    } else {
        let s = height as f64 / TEMPLATE_BASE_HEIGHT;
        (((rgb_raw.width() as f64 * s) as u32).max(1), ((rgb_raw.height() as f64 * s) as u32).max(1))
    };

    let rgb = fast_resize(&rgb_raw, final_w, final_h, ResizeAlg::Convolution(FilterType::Lanczos3)).to_rgb8();
    let tpl = Arc::new(rgb);
    let mut cache = k!(TEMPLATES);
    cache.get_or_insert_with(HashMap::new).insert(key, tpl.clone());
    Ok(tpl)
}

pub fn ncc_match_template(
    image_dir: &str,
    background: &RgbImage,
    name: &str,
    roi: Option<(u32, u32, u32, u32)>,
) -> (u32, u32, f64) {
    ncc_match_template_ex(image_dir, background, name, None, None, roi)
}

pub fn ncc_match_template_ex(
    image_dir: &str,
    background: &RgbImage,
    name: &str,
    crop: Option<(u32, u32, u32, u32)>,
    template_size: Option<(u32, u32)>,
    roi: Option<(u32, u32, u32, u32)>,
) -> (u32, u32, f64) {
    let tpl = match load_template_ex(image_dir, name, background.height(), crop, template_size) {
        Ok(t) => t,
        Err(e) => {
            log_error!("加载模板“{name}”失败：{e:#}");
            return (0, 0, 0.);
        }
    };
    ncc_match(background, &tpl, roi)
}

pub fn get_pixel(img: &ImageBuffer<Rgb<u8>, Vec<u8>>, x: u32, y: u32) -> [u8; 3] {
    let (real_x, real_y) = scale_coords(img.width(), img.height(), x, y);
    let px = img.get_pixel(real_x, real_y);
    [px[0], px[1], px[2]]
}

pub fn pixel_equal(img: &ImageBuffer<Rgb<u8>, Vec<u8>>, x: u32, y: u32, r: u8, g: u8, b: u8) -> bool {
    let p = get_pixel(img, x, y);
    p[0] == r && p[1] == g && p[2] == b
}

pub fn pixel_like(img: &ImageBuffer<Rgb<u8>, Vec<u8>>, x: u32, y: u32, r: u8, g: u8, b: u8, v: u8) -> bool {
    let p = get_pixel(img, x, y);
    (p[0] as i16 - r as i16).abs() <= v as i16
        && (p[1] as i16 - g as i16).abs() <= v as i16
        && (p[2] as i16 - b as i16).abs() <= v as i16
}

pub fn find_line(
    img: &ImageBuffer<Rgb<u8>, Vec<u8>>,
    roi_x: i64,
    roi_y: i64,
    roi_w: i64,
    roi_h: i64,
    r: i64,
    g: i64,
    b: i64,
    threshold: i64,
    left_to_right: bool,
    up_to_bottom: bool,
    is_horizontal: bool,
    min: i64,
    max: i64,
) -> (i64, i64, i64) {
    let w = img.width();
    let h = img.height();
    let (cr, cg, cb) = (r as u8, g as u8, b as u8);
    let tv = threshold as u8;
    let (inner_lo, inner_hi, inner_pos) = if is_horizontal {
        (roi_x, roi_x + roi_w, left_to_right)
    } else {
        (roi_y, roi_y + roi_h, up_to_bottom)
    };
    let (outer_lo, outer_hi, outer_pos) = if is_horizontal {
        (roi_y, roi_y + roi_h, up_to_bottom)
    } else {
        (roi_x, roi_x + roi_w, left_to_right)
    };
    let inner_done = move |i: i64| if inner_pos { i > inner_hi } else { i < inner_lo };
    let outer_done = move |o: i64| if outer_pos { o > outer_hi } else { o < outer_lo };
    let emit = move |count: i64, start: i64, outer: i64| -> Option<(i64, i64, i64)> {
        if count > 0 && min <= count && count <= max {
            let center = if inner_pos { start + count / 2 } else { start - count / 2 };
            Some(if is_horizontal {
                (center, outer, count)
            } else {
                (outer, center, count)
            })
        } else {
            None
        }
    };
    let mut outer = if outer_pos { outer_lo } else { outer_hi };
    while !outer_done(outer) {
        let mut count = 0;
        let mut start = 0;
        let mut inner = if inner_pos { inner_lo } else { inner_hi };
        while !inner_done(inner) {
            let (x, y) = if is_horizontal { (inner, outer) } else { (outer, inner) };
            let (px, py) = scale_coords(w, h, x as u32, y as u32);
            let p = img.get_pixel(px, py);
            if (p[0] as i16 - cr as i16).abs() <= tv as i16
                && (p[1] as i16 - cg as i16).abs() <= tv as i16
                && (p[2] as i16 - cb as i16).abs() <= tv as i16
            {
                if count == 0 {
                    start = inner;
                }
                count += 1;
            } else if let Some(v) = emit(count, start, outer) {
                return v;
            } else {
                count = 0;
            }
            inner += if inner_pos { 1 } else { -1 };
        }
        if let Some(v) = emit(count, start, outer) {
            return v;
        }
        outer += if outer_pos { 1 } else { -1 };
    }
    (0, 0, 0)
}

pub fn ncc_match(
    background: &RgbImage,
    template: &RgbImage,
    roi: Option<(u32, u32, u32, u32)>,
) -> (u32, u32, f64) {
    let bg_w = background.width();
    let bg_h = background.height();
    let real_roi = roi.map(|r| scale_roi(bg_w, bg_h, r));
    let (rx, ry, rw, rh) = match real_roi {
        Some((x, y, w, h)) => (x, y, w.min(bg_w.saturating_sub(x)), h.min(bg_h.saturating_sub(y))),
        None => (0, 0, bg_w, bg_h),
    };
    let tw = template.width();
    let th = template.height();
    if tw == 0 || th == 0 || rw < tw || rh < th {
        return (0, 0, 0.);
    }

    let Some(scores) = ncc_rgb_scores(background, template, (rx, ry, rw, rh)) else {
        return (0, 0, 0.);
    };

    let oh = (rh - th + 1) as usize;
    let ow = (rw - tw + 1) as usize;
    let mut best = (0usize, 0usize);
    let mut best_val = f64::NEG_INFINITY;
    for j in 0..oh {
        for i in 0..ow {
            let v = scores[j * ow + i];
            if v > best_val {
                best_val = v;
                best = (i, j);
            }
        }
    }
    if best_val <= 0. || !best_val.is_finite() {
        return (0, 0, 0.);
    }

    let real_cx = best.0 as u32 + tw / 2 + rx;
    let real_cy = best.1 as u32 + th / 2 + ry;
    let base_cx = ((real_cx as f64) / (bg_w as f64 / BASE_WIDTH)).round() as u32;
    let base_cy = ((real_cy as f64) / (bg_h as f64 / BASE_HEIGHT)).round() as u32;
    (base_cx, base_cy, best_val)
}

fn ncc_rgb_scores(
    background: &RgbImage,
    template: &RgbImage,
    roi: (u32, u32, u32, u32),
) -> Option<Vec<f64>> {
    let (rx, ry, rw, rh) = roi;
    let (tw, th) = template.dimensions();
    let samples = tw as f64 * th as f64;
    let find: Vec<_> = template.pixels().map(ncc_features).collect();
    let mean = find.iter().map(|p| p.0).sum::<f64>() / samples;
    let luma_energy = find.iter().map(|p| (p.0 - mean).powi(2)).sum::<f64>();
    let color_energy = find.iter().map(|p| p.1.norm_sqr()).sum::<f64>();
    let total_energy = luma_energy + color_energy;
    if total_energy <= 1e-8 {
        return None;
    }
    let has_color = color_energy / total_energy >= 0.05;
    let template_energy = if has_color { total_energy } else { luma_energy };

    let m = fft_size(rh + th - 1);
    let n = fft_size(rw + tw - 1);
    let mut luma = vec![Complex::new(0., 0.); m * n];
    let mut color = has_color.then(|| vec![Complex::new(0., 0.); m * n]);
    let rw_u = rw as usize;
    let tw_u = tw as usize;
    let th_u = th as usize;
    let stride = rw_u + 1;
    let mut sums = vec![0.; stride * (rh as usize + 1)];
    let mut luma_squares = vec![0.; sums.len()];
    let mut color_squares = vec![0.; sums.len()];
    for j in 0..rh as usize {
        let mut row_sum = 0.;
        let mut row_luma_square = 0.;
        let mut row_color_square = 0.;
        for i in 0..rw_u {
            let (l, c) = ncc_features(background.get_pixel(rx + i as u32, ry + j as u32));
            luma[j * n + i].re = l;
            if let Some(color) = &mut color {
                color[j * n + i] = c;
            }
            row_sum += l;
            row_luma_square += l * l;
            row_color_square += c.norm_sqr();
            let index = (j + 1) * stride + i + 1;
            sums[index] = sums[j * stride + i + 1] + row_sum;
            luma_squares[index] = luma_squares[j * stride + i + 1] + row_luma_square;
            color_squares[index] = color_squares[j * stride + i + 1] + row_color_square;
        }
    }
    for j in 0..th_u {
        for i in 0..tw_u {
            luma[j * n + i].im = find[(th_u - 1 - j) * tw_u + tw_u - 1 - i].0 - mean;
        }
    }
    if luma_energy > 1e-8 {
        correlate_real_pair(&mut luma, m, n);
    }
    if let Some(color) = &mut color {
        let mut kernel = vec![Complex::new(0., 0.); m * n];
        for j in 0..th_u {
            for i in 0..tw_u {
                kernel[j * n + i] = find[(th_u - 1 - j) * tw_u + tw_u - 1 - i].1.conj();
            }
        }
        fft2d(color, m, n, false);
        fft2d(&mut kernel, m, n, false);
        for (a, b) in color.iter_mut().zip(&kernel) {
            *a *= b;
        }
        drop(kernel);
        fft2d(color, m, n, true);
    }
    let inv = 1. / (m * n) as f64;

    let oh = (rh - th + 1) as usize;
    let ow = (rw - tw + 1) as usize;
    let mut scores = vec![0.; oh * ow];
    for j in 0..oh {
        for i in 0..ow {
            let sum = window_sum(&sums, stride, i, j, tw_u, th_u);
            let sum_square = window_sum(&luma_squares, stride, i, j, tw_u, th_u);
            let le = (sum_square - sum * sum / samples).max(0.);
            let ce = if has_color {
                window_sum(&color_squares, stride, i, j, tw_u, th_u).max(0.)
            } else {
                0.
            };
            let energy = le + ce;
            if energy <= 1e-8 {
                continue;
            }
            let index = (j + th_u - 1) * n + i + tw_u - 1;
            let ld = if luma_energy > 1e-8 { luma[index].re * inv } else { 0. };
            let cd = color.as_ref().map_or(0., |c| c[index].re * inv);
            let mut score = ((ld + cd) / (energy * template_energy).sqrt()).clamp(-1., 1.);
            if has_color {
                score *= if ce > 1e-8 { (cd / (ce * color_energy).sqrt()).clamp(0., 1.) } else { 0. };
            }
            scores[j * ow + i] = score;
        }
    }
    Some(scores)
}

fn ncc_features(p: &Rgb<u8>) -> (f64, Complex<f64>) {
    let (r, g, b) = (p[0] as f64, p[1] as f64, p[2] as f64);
    ((r + g + b) * 0.5773502691896258,
     Complex::new((r - g) * std::f64::consts::FRAC_1_SQRT_2, (r + g - 2. * b) * 0.4082482904638631))
}

fn correlate_real_pair(data: &mut [Complex<f64>], rows: usize, cols: usize) {
    fft2d(data, rows, cols, false);
    for r in 0..rows {
        let mirror_row = if r == 0 { 0 } else { rows - r };
        for c in 0..cols {
            let i = r * cols + c;
            let mirror = mirror_row * cols + if c == 0 { 0 } else { cols - c };
            if i > mirror {
                continue;
            }
            let a = data[i];
            let b = data[mirror].conj();
            let product = ((a + b) * 0.5) * ((a - b) * Complex::new(0., -0.5));
            data[i] = product;
            data[mirror] = product.conj();
        }
    }
    fft2d(data, rows, cols, true);
}

fn fft_size(min: u32) -> usize {
    let mut best = (min as usize).next_power_of_two();
    let mut p2 = 1;
    while p2 <= best {
        let mut p3 = p2;
        while p3 <= best {
            let mut p5 = p3;
            while p5 < min as usize { p5 *= 5; }
            best = best.min(p5);
            p3 *= 3;
        }
        p2 *= 2;
    }
    best
}

fn window_sum(sums: &[f64], stride: usize, x: usize, y: usize, w: usize, h: usize) -> f64 {
    sums[(y + h) * stride + x + w] - sums[y * stride + x + w] - sums[(y + h) * stride + x] + sums[y * stride + x]
}

fn fft2d(data: &mut [Complex<f64>], rows: usize, cols: usize, inverse: bool) {
    let direction = if inverse {
        FftDirection::Inverse
    } else {
        FftDirection::Forward
    };
    let (row_fft, col_fft) = {
        let mut planner = k!(FFT_PLANNER);
        let planner = planner.get_or_insert_with(FftPlanner::new);
        (planner.plan_fft(cols, direction), planner.plan_fft(rows, direction))
    };
    let mut scratch = vec![Complex::new(0., 0.);
        row_fft.get_inplace_scratch_len().max(col_fft.get_inplace_scratch_len())];
    for row in data.chunks_exact_mut(cols) {
        row_fft.process_with_scratch(row, &mut scratch);
    }
    let mut transposed = vec![Complex::new(0., 0.); rows * cols];
    transpose(data, &mut transposed, rows, cols);
    for row in transposed.chunks_exact_mut(rows) {
        col_fft.process_with_scratch(row, &mut scratch);
    }
    transpose(&transposed, data, cols, rows);
}

fn transpose(src: &[Complex<f64>], dst: &mut [Complex<f64>], rows: usize, cols: usize) {
    const BLOCK: usize = 8;
    for r0 in (0..rows).step_by(BLOCK) {
        for c0 in (0..cols).step_by(BLOCK) {
            for r in r0..(r0 + BLOCK).min(rows) {
                for c in c0..(c0 + BLOCK).min(cols) {
                    dst[c * rows + r] = src[r * cols + c];
                }
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
    hist_mask: GrayImage,
}

impl TemplateSet {
    pub fn new(
        size: u32,
        radius_ratio: f32,
        hist_radius_ratio: f32,
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
            hist_mask: circle_mask(size, hist_radius_ratio),
        }
    }

    pub fn add_alpha_ref(&mut self, name: &str, file: &str, img: &RgbaImage) {
        let resized = fast_resize(
            &DynamicImage::ImageRgba8(img.clone()),
            self.size,
            self.size,
            ResizeAlg::Convolution(FilterType::Bilinear),
        ).to_rgba8();
        let mask = GrayImage::from_fn(self.size, self.size, |x, y| {
            Luma([if resized.get_pixel(x, y).0[3] > 128 { 255 } else { 0 }])
        });
        self.refs.push(RefItem {
            name: name.to_string(),
            file: file.to_string(),
            vec: feature(&resized, &mask, &self.hist_mask),
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
        let (x, y, w, h) = scale_roi(frame.width(), frame.height(), roi);
        if x + w > frame.width() || y + h > frame.height() {
            return None;
        }
        let crop = image::imageops::crop_imm(frame, x, y, w, h).to_image();
        let q = fast_resize(
            &DynamicImage::ImageRgb8(crop),
            self.size,
            self.size,
            ResizeAlg::Convolution(FilterType::Bilinear),
        ).to_rgba8();
        let (_, var) = masked_gray_stats(&q, &self.query_mask);
        let qv = feature(&q, &self.query_mask, &self.hist_mask);

        // Keep the best score of character within all fashions
        let mut best_by_name: HashMap<&str, (usize, f32)> = HashMap::new();
        for (i, r) in self.refs.iter().enumerate() {
            let score = cosine(&qv, &r.vec);
            let best = best_by_name.entry(&r.name).or_insert((i, score));
            if score > best.1 {
                *best = (i, score);
            }
        }
        let mut scored: Vec<(usize, f32)> = best_by_name.into_values().collect();
        scored.sort_by(|a, b| {
            b.1.total_cmp(&a.1)
                .then_with(|| self.refs[a.0].name.cmp(&self.refs[b.0].name))
        });

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
            let runner_up = scored.get(1).map_or(f32::NEG_INFINITY, |x| x.1);
            if s < self.score_min || s - runner_up < self.score_gap {
                None
            } else {
                Some((self.refs[i].name.clone(), s))
            }
        };
        Some(MatchReport { var, top, verdict })
    }
}

fn feature(rgba: &RgbaImage, mask: &GrayImage, hist_mask: &GrayImage) -> Vec<f32> {
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
    let mut hist = hsv_hist(rgba, mask, hist_mask);
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

fn hsv_hist(img: &RgbaImage, mask: &GrayImage, hist_mask: &GrayImage) -> Vec<f32> {
    const H_BINS: usize = 16;
    const S_BINS: usize = 8;
    const V_BINS: usize = 8;
    let mut hist = vec![0f32; H_BINS * S_BINS * V_BINS];
    for (x, y, p) in img.enumerate_pixels() {
        if mask.get_pixel(x, y).0[0] == 0 || hist_mask.get_pixel(x, y).0[0] == 0 {
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
