use anyhow::{bail, Context, Result};
use fast_image_resize::{FilterType, ResizeAlg};
use image::{DynamicImage, ImageBuffer, Rgb};
use rhai::{Array, Dynamic, Engine, Scope, AST};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::input::*;
use crate::ocr::Ocr;
use crate::script_engine::{sleep, Frame, TaskState, STOP};
use crate::vision::{pixel_like, BASE_HEIGHT, BASE_WIDTH, MatchReport, TemplateSet, Vision};
use crate::worker;
use crate::k;

pub const WINDOW_TITLE: &str = "异环  ";

const CHARACTER_JSON: &str = "nte/DataTable/Character/DT_Character.json";
const AVATAR_DIR: &str = "nte/UI_Icon/AvatarImage/CustomAvatar/256";
const TP_JSON: &str = "nte/tp.json";

pub const AVATAR_ROIS: [(u32, u32, u32, u32); 4] = [
    (1162, 133, 64, 64),
    (1162, 221, 64, 64),
    (1162, 309, 64, 64),
    (1162, 397, 64, 64),
];

const FEATURE_SIZE: u32 = 64;
const MASK_RADIUS_RATIO: f32 = 0.42;
const SCORE_MIN: f32 = 0.55;
const SCORE_GAP: f32 = 0.01;
const VARIANCE_MIN: f32 = 0.025;
const SOUND_DIR: &str = "nte/sounds";
const DODGE_THRESHOLD: f64 = 0.16;
const DODGE_DELAY: f64 = 0.;
const DODGE_COOLDOWN: f64 = 0.55;
const COUNTER_THRESHOLD: f64 = 0.35;
const COUNTER_DELAY: f64 = 0.1;
const COUNTER_COOLDOWN: f64 = 1.0;

fn dodge_action() -> crate::audio::Action {
    Arc::new(|pad| {
        k!(pad).click(RB, 0.0334, 0.0334);
        k!(pad).click(RB, 0.2, 0.);
    })
}

fn counter_action() -> crate::audio::Action {
    Arc::new(|pad| {
        k!(pad).click(X, 0.0334, 0.0334);
        k!(pad).click(X, 0.2, 0.);
    })
}

#[derive(Debug, Clone)]
#[derive(Default)]
pub struct Character {
    pub name: String,
    pub asset_name: String,
    pub tag: Vec<String>,
    pub element: u8,
    pub debug_info: Option<String>,
}


fn parse_element(s: &str) -> Option<u8> {
    match s {
        "光" => Some(1),
        "灵" => Some(2),
        "咒" => Some(3),
        "暗" => Some(4),
        "魂" => Some(5),
        "相" => Some(6),
        _ => None,
    }
}

fn element_name(e: u8) -> &'static str {
    match e {
        1 => "光",
        2 => "灵",
        3 => "咒",
        4 => "暗",
        5 => "魂",
        6 => "相",
        _ => "",
    }
}

pub fn load_characters() -> Result<Vec<Character>> {
    let text = std::fs::read_to_string(CHARACTER_JSON)
        .with_context(|| format!("读取 {CHARACTER_JSON} 失败"))?;
    let json: serde_json::Value = serde_json::from_str(&text)
        .with_context(|| format!("解析 {CHARACTER_JSON} 失败"))?;
    let rows = json[0]["Rows"].as_object().context("DT_Character.json 缺少 Rows")?;
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for (_id, info) in rows {
        let asset = info["ItemIconBig"]["AssetPathName"]
            .as_str()
            .context("缺少 ItemIconBig.AssetPathName")?
            .rsplit('/')
            .next()
            .unwrap_or("")
            .to_string();
        if !seen.insert(asset.clone()) {
            continue;
        }
        let name = info["ItemName"]["SourceString"]
            .as_str()
            .context("缺少 ItemName.SourceString")?
            .to_string();
        let element_str = info["ElementData"]["CharacterElementType"]
            .as_str()
            .context("缺少 ElementData.CharacterElementType")?;
        let element = parse_element(element_str)
            .ok_or_else(|| anyhow::anyhow!("未知元素类型：{element_str}（角色 {name}）"))?;
        let tag = info["PlayerViewTagArray"]
            .as_array()
            .map(|a| a.iter().filter_map(|t| t.as_str().map(String::from)).collect())
            .unwrap_or_default();
        out.push(Character { name, asset_name: asset, tag, element, debug_info: None });
    }
    Ok(out)
}

pub struct AvatarMatcher {
    templates: TemplateSet,
    chars_by_name: HashMap<String, Character>,
}

impl AvatarMatcher {
    pub fn load(chars: &[Character]) -> Result<Self> {
        let mut files = Vec::new();
        for entry in std::fs::read_dir(AVATAR_DIR)
            .with_context(|| format!("读取 {AVATAR_DIR} 失败"))?
        {
            let path = entry?.path();
            if path.extension().map(|x| x == "png").unwrap_or(false) {
                files.push(path);
            }
        }
        if files.is_empty() {
            bail!("未在 {AVATAR_DIR} 找到任何头像素材");
        }
        let mut chars_by_name = HashMap::new();
        for c in chars {
            chars_by_name.insert(c.name.clone(), c.clone());
        }
        let mut templates =
            TemplateSet::new(FEATURE_SIZE, MASK_RADIUS_RATIO, VARIANCE_MIN, SCORE_MIN, SCORE_GAP);
        for path in &files {
            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            let Some(c) = chars.iter().find(|c| stem.starts_with(&c.asset_name)) else {
                continue;
            };
            let img = image::open(path)
                .with_context(|| format!("读取 {} 失败", path.display()))?
                .to_rgba8();
            let file = path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            templates.add_alpha_ref(&c.name, &file, &img);
        }
        if templates.is_empty() {
            bail!("未在 {AVATAR_DIR} 找到匹配角色的头像素材");
        }
        Ok(Self { templates, chars_by_name })
    }

    pub fn match_roi(
        &self,
        frame: &ImageBuffer<Rgb<u8>, Vec<u8>>,
        roi: (u32, u32, u32, u32),
    ) -> Option<MatchReport> {
        self.templates.match_roi(frame, roi)
    }

    pub fn character(&self, name: &str) -> Option<&Character> {
        self.chars_by_name.get(name)
    }
}

fn report_debug(rep: &MatchReport) -> String {
    let mut s = format!("var: {:.4}  score:", rep.var);
    for (i, t) in rep.top.iter().enumerate() {
        s.push_str(&format!(" {}.{}({} {:.3})", i + 1, t.name, t.file, t.score));
    }
    s
}

pub struct TpPoint {
    pub area: String,
    pub r#type: i64,
    pub index: i64,
}

pub struct TpData {
    pub areas: Vec<String>,
    pub points: HashMap<String, TpPoint>,
}

pub fn load_tp() -> Result<TpData> {
    let text = std::fs::read_to_string(TP_JSON)
        .with_context(|| format!("读取 {TP_JSON} 失败"))?;
    let json: serde_json::Value = serde_json::from_str(&text)
        .with_context(|| format!("解析 {TP_JSON} 失败"))?;
    let areas = json["areas"]
        .as_array()
        .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
        .unwrap_or_default();
    let mut points = HashMap::new();
    if let Some(tp) = json["tp"].as_object() {
        for (name, v) in tp {
            points.insert(
                name.clone(),
                TpPoint {
                    area: v["area"].as_str().unwrap_or("").to_string(),
                    r#type: v["type"].as_i64().unwrap_or(0),
                    index: v["index"].as_i64().unwrap_or(0),
                },
            );
        }
    }
    Ok(TpData { areas, points })
}

fn locate(
    vision: &Vision,
    ocr: &Ocr,
    pad: &Mutex<Gamepad>,
    tp: &TpData,
    target_area: &str,
    type_idx: i64,
    index: i64,
) -> bool {
    let n = tp.areas.len();
    if n == 0 {
        eprintln!("teleport: areas 为空");
        return false;
    }
    let Some(tgt_idx) = tp.areas.iter().position(|a| a == target_area) else {
        eprintln!("teleport: 未知区域：{target_area}");
        return false;
    };
    k!(pad).press(RT, 0.);
    k!(pad).click(RB, 0.1, 0.3);
    let img = match vision.shot() {
        Ok(i) => i,
        Err(e) => {
            eprintln!("teleport: 截图失败：{e}");
            return false;
        }
    };
    let lines = match ocr.recognize_roi(&img, (974, 132, 200, 30)) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("teleport: 区域识别失败：{e}");
            return false;
        }
    };
    let text = lines.iter().map(|l| l.text.trim()).collect::<Vec<_>>().concat();
    let Some(cur_idx) = tp.areas.iter().position(|a| text.contains(a.as_str())) else {
        eprintln!("teleport: 无法匹配当前区域：{text:?}");
        return false;
    };
    let mut d = (tgt_idx as i64 - cur_idx as i64).rem_euclid(n as i64);
    if d > n as i64 / 2 {
        d -= n as i64;
    }
    if d > 0 {
        for _ in 0..d {
            k!(pad).click(RIGHT, 0.1, 0.3);
        }
    } else if d < 0 {
        for _ in 0..-d {
            k!(pad).click(LEFT, 0.1, 0.3);
        }
    }
    for _ in 0..type_idx {
        k!(pad).click(DOWN, 0.1, 0.2);
    }
    k!(pad).click(A, 0.1, 0.2);
    for _ in 0..=index {
        k!(pad).click(DOWN, 0.1, 0.2);
    }
    k!(pad).click(A, 0.3, 1.7);
    k!(pad).click(B, 0.1, 0.3);
    k!(pad).release(RT, 0.);
    true
}

fn teleport(
    vision: &Vision,
    ocr: &Ocr,
    pad: &Mutex<Gamepad>,
    tp: &TpData,
    target_area: &str,
    type_idx: i64,
    index: i64,
) -> bool {
    k!(pad).click(BACK, 0.1, 1.7);
    if !locate(vision, ocr, pad, tp, target_area, type_idx, index) { return false }
    k!(pad).click(A, 0.3, 0.2);
    k!(pad).click(A, 0.3, 0.2);
    k!(pad).click(A, 0.3, 0.2);
    k!(pad).click(A, 0.5, 4.);
    wait_loading(vision)
}

fn wait_loading(vision: &Vision) -> bool {
    for _ in 0..200 {
        let img = vision.shot().unwrap();
        if pixel_like(&img, 32, 134, 255, 255, 255, 15) &&
            pixel_like(&img, 902, 37, 255, 255, 255, 15) &&
            pixel_like(&img, 1182, 37, 255, 255, 255, 15) &&
            !pixel_like(&img, 26, 134, 255, 255, 255, 15) {
            return true
        }
        sleep(0.1);
    }
    false
}

fn treasure_morph(buf: &mut [u8], tmp: &mut [u8], w: usize, h: usize, r: usize, dilate: bool) {
    for y in 0..h {
        let src = &buf[y * w..(y + 1) * w];
        let dst = &mut tmp[y * w..(y + 1) * w];
        for x in 0..w {
            dst[x] = if dilate {
                let x0 = x.saturating_sub(r);
                let x1 = (x + r + 1).min(w);
                let mut v = 0;
                for i in x0..x1 {
                    if src[i] != 0 {
                        v = 1;
                        break;
                    }
                }
                v
            } else {
                let x0 = x.saturating_sub(r);
                let x1 = (x + r + 1).min(w);
                let mut v = 1;
                for i in x0..x1 {
                    if src[i] == 0 {
                        v = 0;
                        break;
                    }
                }
                v
            };
        }
    }
    for x in 0..w {
        for y in 0..h {
            buf[y * w + x] = if dilate {
                let y0 = y.saturating_sub(r);
                let y1 = (y + r + 1).min(h);
                let mut v = 0;
                for j in y0..y1 {
                    if tmp[j * w + x] != 0 {
                        v = 1;
                        break;
                    }
                }
                v
            } else {
                let y0 = y.saturating_sub(r);
                let y1 = (y + r + 1).min(h);
                let mut v = 1;
                for j in y0..y1 {
                    if tmp[j * w + x] == 0 {
                        v = 0;
                        break;
                    }
                }
                v
            };
        }
    }
}

fn find_treasure(img: &ImageBuffer<Rgb<u8>, Vec<u8>>) -> Array {
    let owned;
    let work: &ImageBuffer<Rgb<u8>, Vec<u8>> =
        if img.width() == BASE_WIDTH as u32 && img.height() == BASE_HEIGHT as u32 {
            img
        } else {
            let dyn_img = DynamicImage::ImageRgb8(img.clone());
            owned = crate::vision::fast_resize(
                &dyn_img,
                BASE_WIDTH as u32,
                BASE_HEIGHT as u32,
                ResizeAlg::Convolution(FilterType::Lanczos3),
            )
            .into_rgb8();
            &owned
        };
    let w = BASE_WIDTH as usize;
    let h = BASE_HEIGHT as usize;
    let data = work.as_raw();
    let mut raw = vec![0u8; w * h];
    for (i, px) in data.chunks_exact(3).enumerate() {
        let r = px[0] as i16;
        let g = px[1] as i16;
        let b = px[2] as i16;
        if r >= 150 && r > b + 20 && b > g + 20 {
            let (rf, gf, bf) = (r as f64, g as f64, b as f64);
            let d = rf - gf;
            let s = d / rf * 255.;
            if s >= 80. && s <= 210. {
                let hue = 360. + 60. * (gf - bf) / d;
                if hue >= 310. && hue <= 352. {
                    raw[i] = 1;
                }
            }
        }
    }
    let mut mask = raw.clone();
    let mut tmp = vec![0u8; w * h];
    treasure_morph(&mut mask, &mut tmp, w, h, 6, true);
    treasure_morph(&mut mask, &mut tmp, w, h, 6, false);
    treasure_morph(&mut mask, &mut tmp, w, h, 3, true);
    let mut labels = vec![0u32; w * h];
    let mut areas = vec![0u32];
    let mut stack = Vec::new();
    for start in 0..w * h {
        if mask[start] == 0 || labels[start] != 0 {
            continue;
        }
        let id = areas.len() as u32;
        areas.push(0);
        labels[start] = id;
        stack.clear();
        stack.push(start);
        while let Some(i) = stack.pop() {
            areas[id as usize] += 1;
            let x = i % w;
            let y = i / w;
            for yy in y.saturating_sub(1)..=(y + 1).min(h - 1) {
                for xx in x.saturating_sub(1)..=(x + 1).min(w - 1) {
                    let j = yy * w + xx;
                    if mask[j] != 0 && labels[j] == 0 {
                        labels[j] = id;
                        stack.push(j);
                    }
                }
            }
        }
    }
    let nc = areas.len();
    let mut cnt = vec![0u32; nc];
    let mut sum_r = vec![0u64; nc];
    let mut sum_g = vec![0u64; nc];
    let mut sum_b = vec![0u64; nc];
    let mut top = vec![[i32::MAX; 2]; nc];
    let mut bot = vec![[i32::MIN; 2]; nc];
    let mut lft = vec![[i32::MAX; 2]; nc];
    let mut rgt = vec![[i32::MIN; 2]; nc];
    for i in 0..w * h {
        if raw[i] == 0 {
            continue;
        }
        let l = labels[i] as usize;
        if l == 0 {
            continue;
        }
        cnt[l] += 1;
        let x = (i % w) as i32;
        let y = (i / w) as i32;
        let o = i * 3;
        sum_r[l] += data[o] as u64;
        sum_g[l] += data[o + 1] as u64;
        sum_b[l] += data[o + 2] as u64;
        if y < top[l][1] {
            top[l] = [x, y];
        }
        if y > bot[l][1] {
            bot[l] = [x, y];
        }
        if x < lft[l][0] {
            lft[l] = [x, y];
        }
        if x > rgt[l][0] {
            rgt[l] = [x, y];
        }
    }
    let mut best: Option<(i64, i64, f64)> = None;
    for l in 1..nc {
        if areas[l] < 120 || areas[l] > 1200 || cnt[l] < 8 {
            continue;
        }
        let (tx, ty) = (top[l][0], top[l][1]);
        let (bx, by) = (bot[l][0], bot[l][1]);
        let (lx, ly) = (lft[l][0], lft[l][1]);
        let (rx, ry) = (rgt[l][0], rgt[l][1]);
        let rw = rx - lx + 1;
        let rh = by - ty + 1;
        if rw < 14 || rw > 32 || rh < 14 || rh > 32 {
            continue;
        }
        let align_x = (tx - bx).abs();
        let align_y = (ly - ry).abs();
        let wh = (rw - rh).abs();
        if align_x > 4 || align_y > 4 || wh > 3 {
            continue;
        }
        let mut hits = 0;
        let probes = [
            [(tx, ty - 1), (tx, ty - 2)],
            [(bx, by + 1), (bx, by + 2)],
            [(lx - 1, ly), (lx - 2, ly)],
            [(rx + 1, ry), (rx + 2, ry)],
        ];
        for dir in probes {
            for (px, py) in dir {
                if px >= 0 && py >= 0 && px < w as i32 && py < h as i32 {
                    let o = ((py as usize) * w + px as usize) * 3;
                    if data[o] < 50 && data[o + 1] < 50 && data[o + 2] < 50 {
                        hits += 1;
                        break;
                    }
                }
            }
        }
        let n = cnt[l] as f64;
        let mr = sum_r[l] as f64 / n;
        let mg = sum_g[l] as f64 / n;
        let mb = sum_b[l] as f64 / n;
        let dist =
            ((mr - 255.).powi(2) + (mg - 120.).powi(2) + (mb - 185.).powi(2)).sqrt();
        let score = hits as f64 * 30. - dist * 1.5 - wh as f64 * 5. - align_x as f64 * 3.;
        let cx = ((lx + rx) / 2) as i64;
        let cy = ((ty + by) / 2) as i64;
        match best {
            Some((_, _, bs)) if bs >= score => {}
            _ => best = Some((cx, cy, score)),
        }
    }
    match best {
        Some((cx, cy, score)) => vec![cx.into(), cy.into(), score.into()],
        None => vec![0i64.into(), 0i64.into(), 0f64.into()],
    }
}

pub fn setup_engine(
    engine: &mut Engine,
    pad: &Arc<Mutex<Gamepad>>,
    _state: &TaskState,
    vision: &Arc<Vision>,
    ocr: &Arc<Ocr>,
) {
    let chars = load_characters().unwrap();
    let matcher = Arc::new(AvatarMatcher::load(&chars).unwrap());
    let tp = Arc::new(load_tp().unwrap());
    engine.register_fn("name", |c: &mut Character| c.name.clone());
    engine.register_fn("tag", |c: &mut Character| -> Array {
        c.tag.iter().map(|t| t.into()).collect()
    });
    engine.register_fn("element", |c: &mut Character| element_name(c.element).to_string());
    engine.register_fn("debug_info", |c: &mut Character| {
        c.debug_info.clone().unwrap_or_default()
    });
    crate::audio::ensure_started(
        pad.clone(),
        vec![
            crate::audio::TemplateConfig {
                name: "dodge",
                path: format!("{SOUND_DIR}/dodge.wav").into(),
                threshold: DODGE_THRESHOLD,
                delay: DODGE_DELAY,
                cooldown: DODGE_COOLDOWN,
                action: dodge_action(),
            },
            crate::audio::TemplateConfig {
                name: "counter",
                path: format!("{SOUND_DIR}/counter.wav").into(),
                threshold: COUNTER_THRESHOLD,
                delay: COUNTER_DELAY,
                cooldown: COUNTER_COOLDOWN,
                action: counter_action(),
            },
        ],
    );
    engine.register_fn("set_dodge", |on: bool| crate::audio::set_switch("dodge", on));
    engine.register_fn("get_dodge", || -> bool { crate::audio::get_switch("dodge") });
    engine.register_fn("set_counter", |on: bool| {
        crate::audio::set_switch("counter", on)
    });
    engine.register_fn("get_counter", || -> bool {
        crate::audio::get_switch("counter")
    });

    let m = matcher.clone();
    engine.register_fn("get_team", move |img: Frame| -> Array {
        let img = k!(img);
        AVATAR_ROIS
            .iter()
            .map(|&roi| {
                let rep = match m.match_roi(&img, roi) {
                    Some(r) => r,
                    None => return Dynamic::from(Character::default()),
                };
                let mut c = match &rep.verdict {
                    Some((name, _)) => m.character(name).cloned().unwrap_or_default(),
                    None => Character::default(),
                };
                c.debug_info = Some(report_debug(&rep));
                Dynamic::from(c)
            })
            .collect()
    });

    let v = vision.clone();
    let p = pad.clone();
    let o = ocr.clone();
    let t = tp.clone();
    engine.register_fn("teleport", move |target: &str| -> bool {
        let Some(pt) = t.points.get(target) else {
            eprintln!("teleport: 未知地点：{target}");
            return false;
        };
        teleport(&v, &o, &p, &t, &pt.area, pt.r#type, pt.index)
    });
    let v = vision.clone();
    let p = pad.clone();
    let o = ocr.clone();
    let t = tp.clone();
    engine.register_fn("teleport", move |area: &str, type_idx: i64, index: i64| -> bool {
        teleport(&v, &o, &p, &t, area, type_idx, index)
    });
    let v = vision.clone();
    let p = pad.clone();
    let o = ocr.clone();
    let t = tp.clone();
    engine.register_fn("locate", move |target: &str| -> bool {
        let Some(pt) = t.points.get(target) else {
            eprintln!("locate: 未知地点：{target}");
            return false;
        };
        locate(&v, &o, &p, &t, &pt.area, pt.r#type, pt.index)
    });
    let v = vision.clone();
    let p = pad.clone();
    let o = ocr.clone();
    let t = tp.clone();
    engine.register_fn("locate", move |area: &str, type_idx: i64, index: i64| -> bool {
        locate(&v, &o, &p, &t, area, type_idx, index)
    });
    let v = vision.clone();
    engine.register_fn("wait_loading", move || -> bool {
        wait_loading(&v)
    });
    engine.register_fn("find_treasure", |img: Frame| -> Array { find_treasure(&k!(img)) });

    k!(pad).click(LB, 0.0334, 0.);
}

pub fn run(
    engine: Arc<Engine>,
    ast: Arc<AST>,
    scope: Arc<Scope<'static>>,
    _state: &TaskState,
    exit: Arc<AtomicBool>,
    reset: Arc<AtomicBool>,
    timeout: std::time::Duration,
    log: Arc<dyn Fn(&str) + Send + Sync>,
) -> Result<()> {
    STOP.store(false, Ordering::SeqCst);
    let handle = worker::spawn_script(engine.clone(), ast.clone(), scope.clone(), log.clone());
    let start = Instant::now();
    loop {
        if worker::check(&handle, &exit, &reset, timeout, start).is_some() {
            break;
        }
        sleep(0.5);
    }
    let _ = handle.join();
    STOP.store(false, Ordering::SeqCst);
    Ok(())
}
