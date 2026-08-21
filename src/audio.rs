use rustfft::{FftPlanner, num_complex::Complex};
use anyhow::Context as _;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{Builder, sleep};
use std::time::{Duration, Instant};

use windows::Win32::Media::Audio::{
    AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_LOOPBACK, IAudioCaptureClient, IAudioClient,
    IMMDeviceEnumerator, MMDeviceEnumerator, WAVEFORMATEXTENSIBLE, eConsole, eRender,
};
use windows::Win32::System::Com::{CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree};

use crate::input::Gamepad;
use crate::k;

pub type Action = Arc<dyn Fn(&Arc<Mutex<Gamepad>>) + Send + Sync>;

pub struct TemplateConfig {
    pub name: &'static str,
    pub path: PathBuf,
    pub threshold: f64,
    pub delay: f64,
    pub cooldown: f64,
    pub action: Action,
    pub enabled: Arc<AtomicBool>,
}

struct PreparedTemplate {
    matcher: Matcher,
    threshold: f64,
    delay: f64,
    cooldown: f64,
    action: Action,
    enabled: Arc<AtomicBool>,
}

struct Matcher {
    freq: Vec<Complex<f32>>,
    tmpl_len: usize,
    inverse: Arc<dyn rustfft::Fft<f32>>,
}

impl Matcher {
    fn new(template_norm: &[f32], n: usize, planner: &mut FftPlanner<f32>) -> Self {
        let mut freq: Vec<Complex<f32>> = template_norm
            .iter()
            .map(|&x| Complex::new(x, 0.))
            .collect();
        freq.resize(n, Complex::new(0., 0.));
        planner.plan_fft(n, rustfft::FftDirection::Forward).process(&mut freq);
        let inverse = planner.plan_fft(n, rustfft::FftDirection::Inverse);
        Self { freq, tmpl_len: template_norm.len(), inverse }
    }

    fn score(&self, stream_spec: &[Complex<f32>], denom: usize) -> f32 {
        if denom < self.tmpl_len {
            return 0.;
        }
        let mut a = stream_spec.to_vec();
        for (x, y) in a.iter_mut().zip(self.freq.iter()) {
            *x *= y.conj();
        }
        self.inverse.process(&mut a);
        let inv = 1. / a.len() as f32;
        let valid = denom - self.tmpl_len + 1;
        let mut best = 0f32;
        for k in 0..valid {
            let v = a[k + self.tmpl_len - 1].re * inv;
            if v > best {
                best = v;
            }
        }
        best / denom as f32
    }
}

const FRAME_STEP_SECS: f64 = 0.05;
const POLL_MILLIS: u64 = 5;
const WORK_RATE: u32 = 32000;
const WINDOW_MARGIN_SECS: f64 = 0.6;

static SWITCHES: Mutex<Vec<Arc<AtomicBool>>> = Mutex::new(Vec::new());
static STATE: Mutex<Option<()>> = Mutex::new(None);

pub fn ensure_started(pad: Arc<Mutex<Gamepad>>, templates: Vec<TemplateConfig>) {
    let mut st = k!(STATE);
    let first = st.is_none();
    if first {
        *st = Some(());
    }
    drop(st);
    if first {
        k!(SWITCHES).extend(templates.iter().map(|t| t.enabled.clone()));
    }
    if !first {
        return;
    }
    let _ = Builder::new().name("dodge".into()).spawn(move || {
        if let Err(e) = listen(pad, templates) {
            eprintln!("音频监听启动失败：{e}");
        }
    });
}

pub fn disable_all() {
    for s in k!(SWITCHES).iter() {
        s.store(false, Ordering::SeqCst);
    }
}

fn listen(pad: Arc<Mutex<Gamepad>>, configs: Vec<TemplateConfig>) -> anyhow::Result<()> {
    let raws: Vec<(LoadedTemplate, TemplateConfig)> = configs
        .into_iter()
        .map(|c| {
            let name = c.name;
            let (data, rate) = load_wav(&c.path)
                .map_err(|e| anyhow::anyhow!("模板 {name} 加载失败：{e}"))?;
            Ok((LoadedTemplate { data, src_rate: rate }, c))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    let (client, capture, fmt) = open_loopback()?;
    let fs = fmt.sample_rate;
    let channels = fmt.channels as usize;
    let is_float = fmt.is_float;

    unsafe {
        client.Start()?;
    }

    let mut planner = FftPlanner::<f32>::new();
    let norms: Vec<Vec<f32>> = raws
        .iter()
        .map(|(t, _)| normalize(&highpass(&resample(&t.data, t.src_rate, WORK_RATE), WORK_RATE)))
        .collect();
    let max_tmpl = norms.iter().map(|v| v.len()).max().unwrap_or(1);
    let window_work = max_tmpl + (WINDOW_MARGIN_SECS * WORK_RATE as f64) as usize;
    let window_native = ((window_work as f64) * fs as f64 / WORK_RATE as f64).ceil() as usize;
    let step = (FRAME_STEP_SECS * fs as f64) as usize;
    let n_common = window_work + max_tmpl;
    let forward = planner.plan_fft(n_common, rustfft::FftDirection::Forward);

    let mut cfg_iter = raws.into_iter();
    let prepared: Vec<PreparedTemplate> = norms
        .into_iter()
        .map(|norm| {
            let (_, c) = cfg_iter.next().unwrap();
            PreparedTemplate {
                matcher: Matcher::new(&norm, n_common, &mut planner),
                threshold: c.threshold,
                delay: c.delay,
                cooldown: c.cooldown,
                action: c.action,
                enabled: c.enabled,
            }
        })
        .collect();
    let mut last_fire: Vec<Option<Instant>> = vec![None; prepared.len()];

    let mut buf: Vec<f32> = Vec::new();
    let mut pending: Vec<f32> = Vec::new();
    let mut since_calc = 0usize;
    loop {
        let packet = unsafe { capture.GetNextPacketSize()? };
        if packet == 0 {
            sleep(Duration::from_millis(POLL_MILLIS));
            continue;
        }
        let mut data_ptr: *mut u8 = std::ptr::null_mut();
        let mut frames = 0u32;
        let mut flags = 0u32;
        unsafe {
            capture.GetBuffer(&mut data_ptr, &mut frames, &mut flags, None, None)?;
        }
        if !data_ptr.is_null() && frames > 0 {
            let bytes = unsafe { std::slice::from_raw_parts(data_ptr, frames as usize * channels * fmt.bytes_per_sample) };
            decode_into(bytes, frames as usize, channels, is_float, fmt.bytes_per_sample, &mut pending);
            buf.append(&mut pending);
            since_calc += frames as usize;
        }
        unsafe {
            capture.ReleaseBuffer(frames)?;
        }
        if since_calc < step || buf.len() < step * 2 {
            continue;
        }
        since_calc = 0;
        let start = buf.len().saturating_sub(window_native.max(step));
        let wr = resample(&buf[start..], fs, WORK_RATE);
        if wr.len() < max_tmpl {
            continue;
        }
        let normed = normalize(&highpass(&wr, WORK_RATE));
        let denom = normed.len();
        let mut spec: Vec<Complex<f32>> = normed.iter().map(|&x| Complex::new(x, 0.)).collect();
        spec.resize(n_common, Complex::new(0., 0.));
        forward.process(&mut spec);
        let scores: Vec<f32> = prepared
            .iter()
            .map(|t| t.matcher.score(&spec, denom))
            .collect();
        for (i, t) in prepared.iter().enumerate() {
            let ready = last_fire[i]
                .map(|t0| t0.elapsed().as_secs_f64() >= t.cooldown)
                .unwrap_or(true);
            if scores[i] as f64 >= t.threshold && ready && t.enabled.load(Ordering::SeqCst) {
                last_fire[i] = Some(Instant::now());
                let act = t.action.clone();
                let pad2 = pad.clone();
                let delay = t.delay;
                Builder::new()
                    .name("dodge-action".into())
                    .spawn(move || {
                        if delay > 0. {
                            sleep(Duration::from_secs_f64(delay));
                        }
                        act(&pad2);
                    })
                    .ok();
            }
        }
        if buf.len() > window_native + step * 4 {
            buf.drain(..buf.len() - window_native);
        }
    }
}

struct LoadedTemplate {
    data: Vec<f32>,
    src_rate: u32,
}

struct MixFormat {
    sample_rate: u32,
    channels: u16,
    bytes_per_sample: usize,
    is_float: bool,
}

fn open_loopback() -> anyhow::Result<(IAudioClient, IAudioCaptureClient, MixFormat)> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let device = enumerator.GetDefaultAudioEndpoint(eRender, eConsole)?;
        let client: IAudioClient = device.Activate::<IAudioClient>(CLSCTX_ALL, None)?;
        let pwfx = client.GetMixFormat()?;
        let wfx = &*pwfx;
        let (is_float, bytes_per_sample) = match wfx.wFormatTag as u32 {
            3 => (true, (wfx.wBitsPerSample / 8) as usize),
            1 => (false, (wfx.wBitsPerSample / 8) as usize),
            0xFFFE => {
                let ext = &*(pwfx as *const WAVEFORMATEXTENSIBLE);
                let sub_format = std::ptr::addr_of!(ext.SubFormat).read_unaligned();
                let float_guid = windows::core::GUID::from_u128(0x00000003_0000_0010_8000_00aa00389b71);
                let pcm_guid = windows::core::GUID::from_u128(0x00000001_0000_0010_8000_00aa00389b71);
                if sub_format == float_guid {
                    (true, (ext.Format.wBitsPerSample / 8) as usize)
                } else if sub_format == pcm_guid {
                    (false, (ext.Format.wBitsPerSample / 8) as usize)
                } else {
                    CoTaskMemFree(Some(pwfx.cast()));
                    anyhow::bail!("不支持的混音格式");
                }
            }
            _ => {
                CoTaskMemFree(Some(pwfx.cast()));
                anyhow::bail!("不支持的混音格式");
            }
        };
        let fmt = MixFormat {
            sample_rate: wfx.nSamplesPerSec,
            channels: wfx.nChannels,
            bytes_per_sample,
            is_float,
        };
        client.Initialize(
            AUDCLNT_SHAREMODE_SHARED,
            AUDCLNT_STREAMFLAGS_LOOPBACK,
            10_000_000,
            0,
            pwfx,
            None,
        )?;
        let capture: IAudioCaptureClient = client.GetService()?;
        Ok((client, capture, fmt))
    }
}

fn decode_into(bytes: &[u8], frames: usize, channels: usize, is_float: bool, bps: usize, out: &mut Vec<f32>) {
    out.clear();
    for f in 0..frames {
        let mut sum = 0f32;
        for c in 0..channels {
            let off = (f * channels + c) * bps;
            let v = match (is_float, bps) {
                (true, 4) => f32::from_le_bytes([bytes[off], bytes[off + 1], bytes[off + 2], bytes[off + 3]]),
                (false, 2) => i16::from_le_bytes([bytes[off], bytes[off + 1]]) as f32 / 32768.,
                _ => 0.,
            };
            sum += v;
        }
        out.push(sum / channels as f32);
    }
}

fn load_wav(path: &std::path::Path) -> anyhow::Result<(Vec<f32>, u32)> {
    let raw = std::fs::read(path)?;
    if &raw[0..4] != b"RIFF" || &raw[8..12] != b"WAVE" {
        anyhow::bail!("不是 WAV 文件");
    }
    let mut pos = 12usize;
    let mut fmt: Option<(u16, u16, u32, u16)> = None;
    let mut data: Option<&[u8]> = None;
    while pos + 8 <= raw.len() {
        let id = &raw[pos..pos + 4];
        let size = u32::from_le_bytes(raw[pos + 4..pos + 8].try_into()?) as usize;
        let body = raw.get(pos + 8..pos + 8 + size).unwrap_or(&[]);
        match id {
            b"fmt " => {
                let tag = u16::from_le_bytes(body[0..2].try_into()?);
                let channels = u16::from_le_bytes(body[2..4].try_into()?);
                let rate = u32::from_le_bytes(body[4..8].try_into()?);
                let bits = u16::from_le_bytes(body[14..16].try_into()?);
                let effective_tag = if tag == 0xFFFE && body.len() >= 40 {
                    let d1 = u32::from_le_bytes(body[24..28].try_into()?);
                    d1 as u16
                } else {
                    tag
                };
                fmt = Some((effective_tag, channels, rate, bits));
            }
            b"data" => data = Some(body),
            _ => {}
        }
        pos += 8 + size + (size & 1);
    }
    let (tag, channels, rate, bits) = fmt.context("缺少 fmt 块")?;
    let data = data.context("缺少 data 块")?;
    let bps = (bits / 8) as usize;
    let frames = data.len() / (channels as usize * bps);
    let mut mono = Vec::with_capacity(frames);
    for f in 0..frames {
        let mut sum = 0f32;
        for c in 0..channels as usize {
            let off = (f * channels as usize + c) * bps;
            let v = match (tag, bits) {
                (3, 32) => f32::from_le_bytes(data[off..off + 4].try_into()?),
                (1, 16) => i16::from_le_bytes(data[off..off + 2].try_into()?) as f32 / 32768.,
                _ => anyhow::bail!("不支持的采样格式 tag={tag} bits={bits}"),
            };
            sum += v;
        }
        mono.push(sum / channels as f32);
    }
    Ok((mono, rate))
}

fn resample(data: &[f32], src: u32, dst: u32) -> Vec<f32> {
    if src == dst || data.is_empty() {
        return data.to_vec();
    }
    let out_len = ((data.len() as f64) * dst as f64 / src as f64).round() as usize;
    let ratio = src as f64 / dst as f64;
    (0..out_len)
        .map(|i| {
            let pos = i as f64 * ratio;
            let i0 = pos.floor() as usize;
            let i1 = (i0 + 1).min(data.len() - 1);
            let frac = (pos - i0 as f64) as f32;
            data[i0] * (1. - frac) + data[i1] * frac
        })
        .collect()
}

struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
}

impl Biquad {
    fn highpass(fs: f32, f0: f32, q: f32) -> Self {
        let w0 = 2. * std::f32::consts::PI * f0 / fs;
        let alpha = w0.sin() / (2. * q);
        let cos = w0.cos();
        let a0 = 1. + alpha;
        Self {
            b0: (1. + cos) / 2. / a0,
            b1: -(1. + cos) / a0,
            b2: (1. + cos) / 2. / a0,
            a1: -2. * cos / a0,
            a2: (1. - alpha) / a0,
            z1: 0.,
            z2: 0.,
        }
    }

    fn apply(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }
}

fn highpass(data: &[f32], fs: u32) -> Vec<f32> {
    let mut s1 = Biquad::highpass(fs as f32, 1000., 0.54119610);
    let mut s2 = Biquad::highpass(fs as f32, 1000., 1.30656296);
    data.iter().map(|&x| s2.apply(s1.apply(x))).collect()
}

fn normalize(data: &[f32]) -> Vec<f32> {
    let n = data.len() as f32;
    if n == 0. {
        return Vec::new();
    }
    let mean = data.iter().sum::<f32>() / n;
    let var = data.iter().map(|x| (x - mean).powi(2)).sum::<f32>() / n;
    let std = var.sqrt() + 1e-8;
    data.iter().map(|x| (x - mean) / std).collect()
}
