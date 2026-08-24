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

#[derive(Clone)]
pub struct TemplateConfig {
    pub name: &'static str,
    pub path: PathBuf,
    pub threshold: f64,
    pub delay: f64,
    pub cooldown: f64,
    pub action: Action,
}

struct PreparedTemplate {
    name: &'static str,
    matcher: Matcher,
    threshold: f64,
    delay: f64,
    cooldown: f64,
    action: Action,
}

struct Matcher {
    freq: Vec<Complex<f32>>,
    tmpl_len: usize,
}

impl Matcher {
    fn new(template_norm: &[f32], n: usize, planner: &mut FftPlanner<f32>) -> Self {
        let mut freq: Vec<Complex<f32>> = template_norm
            .iter()
            .map(|&x| Complex::new(x, 0.))
            .collect();
        freq.resize(n, Complex::new(0., 0.));
        planner.plan_fft(n, rustfft::FftDirection::Forward).process(&mut freq);
        Self { freq, tmpl_len: template_norm.len() }
    }
}

fn peak_of(prod: &[Complex<f32>], im: bool, n: usize, tmpl_len: usize, denom: usize) -> f32 {
    if denom == 0 {
        return 0.;
    }
    let mut best = 0f32;
    if denom >= tmpl_len {
        for k in 0..=(denom - tmpl_len) {
            let c = prod[k + tmpl_len - 1];
            let v = if im { c.im } else { c.re };
            if v > best {
                best = v;
            }
        }
    } else {
        for m in 0..=(tmpl_len - denom) {
            let c = prod[m];
            let v = if im { c.im } else { c.re };
            if v > best {
                best = v;
            }
        }
    }
    best / n as f32 / denom as f32
}

const FRAME_STEP_SECS: f64 = 0.05;
const POLL_MILLIS: u64 = 5;
const WORK_RATE: u32 = 32000;
const DEBUG_SCORES: bool = true;

static SWITCHES: Mutex<Vec<(String, bool)>> = Mutex::new(Vec::new());
static MONITOR: Mutex<Option<Arc<AtomicBool>>> = Mutex::new(None);
static SNAPSHOT: Mutex<Option<(Arc<Mutex<Gamepad>>, Vec<TemplateConfig>)>> = Mutex::new(None);
static REBUILD: AtomicBool = AtomicBool::new(false);

pub fn set_switch(name: &str, on: bool) {
    {
        let mut sw = k!(SWITCHES);
        if let Some(v) = sw.iter_mut().find(|(n, _)| n == name) {
            v.1 = on;
        } else {
            sw.push((name.to_owned(), on));
        }
    }
    if on {
        if k!(MONITOR).is_some() {
            REBUILD.store(true, Ordering::SeqCst);
        }
        maybe_spawn();
    }
}

pub fn get_switch(name: &str) -> bool {
    k!(SWITCHES)
        .iter()
        .find(|(n, _)| n == name)
        .map(|(_, v)| *v)
        .unwrap_or(false)
}

pub fn disable_all() {
    {
        let mut sw = k!(SWITCHES);
        for (_, v) in sw.iter_mut() {
            *v = false;
        }
    }
    maybe_stop();
}

fn maybe_spawn() {
    let alive = k!(MONITOR)
        .as_ref()
        .map(|s| !s.load(Ordering::SeqCst))
        .unwrap_or(false);
    if alive {
        return;
    }
    let Some((pad, configs)) = k!(SNAPSHOT).clone() else {
        return;
    };
    let stop = Arc::new(AtomicBool::new(false));
    *k!(MONITOR) = Some(stop.clone());
    let _ = Builder::new()
        .name("dodge".into())
        .spawn(move || {
            if let Err(e) = listen(pad, configs, stop) {
                eprintln!("音频监听启动失败：{e}");
            }
        });
}

fn maybe_stop() {
    let any_on = k!(SWITCHES).iter().any(|(_, v)| *v);
    if !any_on && let Some(s) = k!(MONITOR).as_ref() {
        s.store(true, Ordering::SeqCst);
        *k!(MONITOR) = None;
    }
}

pub fn ensure_started(pad: Arc<Mutex<Gamepad>>, templates: Vec<TemplateConfig>) {
    {
        let mut sw = k!(SWITCHES);
        for t in &templates {
            if !sw.iter().any(|(n, _)| n == t.name) {
                sw.push((t.name.to_owned(), false));
            }
        }
    }
    *k!(SNAPSHOT) = Some((pad, templates));
}

fn listen(
    pad: Arc<Mutex<Gamepad>>,
    configs: Vec<TemplateConfig>,
    stop: Arc<AtomicBool>,
) -> anyhow::Result<()> {
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

    let mut selected: Vec<TemplateConfig> = raws
        .iter()
        .filter(|(_, c)| get_switch(c.name))
        .map(|(_, c)| c.clone())
        .collect();
    if selected.is_empty() {
        selected = raws.iter().map(|(_, c)| c.clone()).collect();
    }
    let any_enabled = !selected.is_empty()
        && raws
            .iter()
            .any(|(_, c)| get_switch(c.name));

    let mut planner = FftPlanner::<f32>::new();
    let norms: Vec<Vec<f32>> = raws
        .iter()
        .map(|(t, _)| normalize(&highpass(&resample(&t.data, t.src_rate, WORK_RATE), WORK_RATE)))
        .collect();
    let active_max_tmpl = raws
        .iter()
        .zip(norms.iter())
        .filter(|((_, c), _)| get_switch(c.name))
        .map(|(_, v)| v.len())
        .max()
        .unwrap_or(1);
    let stream_work = (FRAME_STEP_SECS * 2. * WORK_RATE as f64) as usize;
    let stream_native = ((stream_work as f64) * fs as f64 / WORK_RATE as f64).ceil() as usize;
    let step = (FRAME_STEP_SECS * fs as f64) as usize;
    let n_common = stream_work + active_max_tmpl;
    let forward = planner.plan_fft(n_common, rustfft::FftDirection::Forward);
    let inverse = planner.plan_fft(n_common, rustfft::FftDirection::Inverse);

    let prepared: Vec<PreparedTemplate> = raws
        .iter()
        .zip(norms.iter())
        .filter(|((_, c), _)| any_enabled && get_switch(c.name))
        .map(|((_, c), norm)| PreparedTemplate {
            name: c.name,
            matcher: Matcher::new(norm, n_common, &mut planner),
            threshold: c.threshold,
            delay: c.delay,
            cooldown: c.cooldown,
            action: c.action.clone(),
        })
        .collect();
    let mut last_fire: Vec<Option<Instant>> = vec![None; prepared.len()];
    let mut scores: Vec<f32> = vec![0.; prepared.len()];
    let mut active: Vec<usize> = Vec::with_capacity(prepared.len());
    let mut spec: Vec<Complex<f32>> = vec![Complex::new(0., 0.); n_common];
    let mut prod_a: Vec<Complex<f32>> = vec![Complex::new(0., 0.); n_common];
    let mut wr: Vec<f32> = Vec::with_capacity(stream_work + 64);
    let mut filtered: Vec<f32> = Vec::with_capacity(stream_work + 64);
    let mut normed: Vec<f32> = Vec::with_capacity(stream_work + 64);

    let mut buf: Vec<f32> = Vec::new();
    let mut pending: Vec<f32> = Vec::new();
    let mut since_calc = 0usize;
    let mut busy = Duration::ZERO;
    let mut hops = 0u64;
    let mut last_report = Instant::now();
    loop {
        if stop.load(Ordering::SeqCst) {
            unsafe {
                client.Stop()?;
            }
            return Ok(());
        }
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
        if since_calc < step || buf.len() < stream_native {
            continue;
        }
        since_calc = 0;
        let t0 = Instant::now();
        active.clear();
        for (i, t) in prepared.iter().enumerate() {
            if DEBUG_SCORES || get_switch(t.name) {
                active.push(i);
            } else {
                scores[i] = 0.;
            }
        }
        if active.is_empty() {
            continue;
        }
        let start = buf.len().saturating_sub(stream_native.max(step));
        resample_into(&buf[start..], fs, WORK_RATE, &mut wr);
        highpass_into(&wr, WORK_RATE, &mut filtered);
        normalize_into(&filtered, &mut normed);
        let denom = normed.len();
        for (i, v) in spec.iter_mut().enumerate() {
            *v = Complex::new(normed.get(i).copied().unwrap_or(0.), 0.);
        }
        forward.process(&mut spec);
        for pair in active.chunks(2) {
            let m0 = &prepared[pair[0]].matcher;
            if pair.len() == 2 {
                let m1 = &prepared[pair[1]].matcher;
                for k in 0..n_common {
                    let s = spec[k];
                    let a = s * m0.freq[k].conj();
                    let b = s * m1.freq[k].conj();
                    prod_a[k] = Complex::new(a.re - b.im, a.im + b.re);
                }
                inverse.process(&mut prod_a);
                scores[pair[0]] = peak_of(&prod_a, false, n_common, m0.tmpl_len, denom);
                scores[pair[1]] = peak_of(&prod_a, true, n_common, m1.tmpl_len, denom);
            } else {
                for k in 0..n_common {
                    prod_a[k] = spec[k] * m0.freq[k].conj();
                }
                inverse.process(&mut prod_a);
                scores[pair[0]] = peak_of(&prod_a, false, n_common, m0.tmpl_len, denom);
            }
        }
        if DEBUG_SCORES {
            let s = scores.iter().map(|v| format!("{v:.4}")).collect::<Vec<_>>().join(" ");
            eprintln!("scores: {s}");
        }
        busy += t0.elapsed();
        hops += 1;
        let elapsed = last_report.elapsed();
        if elapsed.as_secs_f64() >= 5. {
            eprintln!(
                "audio: {} hops/5s, busy {:.2} ms/s ({:.2}% core)",
                hops,
                busy.as_secs_f64() * 1000. / elapsed.as_secs_f64(),
                busy.as_secs_f64() / elapsed.as_secs_f64() * 100.
            );
            hops = 0;
            busy = Duration::ZERO;
            last_report = Instant::now();
        }
        for (i, t) in prepared.iter().enumerate() {
            let ready = last_fire[i]
                .map(|t0| t0.elapsed().as_secs_f64() >= t.cooldown)
                .unwrap_or(true);
            if scores[i] as f64 >= t.threshold && ready && get_switch(t.name) {
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
        if buf.len() > stream_native + step * 4 {
            buf.drain(..buf.len() - stream_native);
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

fn resample_into(data: &[f32], src: u32, dst: u32, out: &mut Vec<f32>) {
    out.clear();
    if src == dst || data.is_empty() {
        out.extend_from_slice(data);
        return;
    }
    let out_len = ((data.len() as f64) * dst as f64 / src as f64).round() as usize;
    let ratio = src as f64 / dst as f64;
    for i in 0..out_len {
        let pos = i as f64 * ratio;
        let i0 = pos.floor() as usize;
        let i1 = (i0 + 1).min(data.len() - 1);
        let frac = (pos - i0 as f64) as f32;
        out.push(data[i0] * (1. - frac) + data[i1] * frac);
    }
}

fn highpass_into(data: &[f32], fs: u32, out: &mut Vec<f32>) {
    let mut s1 = Biquad::highpass(fs as f32, 1000., 0.54119610);
    let mut s2 = Biquad::highpass(fs as f32, 1000., 1.30656296);
    out.clear();
    out.extend(data.iter().map(|&x| s2.apply(s1.apply(x))));
}

fn normalize_into(data: &[f32], out: &mut Vec<f32>) {
    out.clear();
    let n = data.len() as f32;
    if n == 0. {
        return;
    }
    let mean = data.iter().sum::<f32>() / n;
    let var = data.iter().map(|x| (x - mean).powi(2)).sum::<f32>() / n;
    let std = var.sqrt() + 1e-8;
    out.extend(data.iter().map(|x| (x - mean) / std));
}
