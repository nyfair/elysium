use anyhow::{bail, Context, Result};
use image::{ImageBuffer, Rgb};
use rhai::{Array, Dynamic, Engine, Scope, AST};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::input::*;
use crate::ocr::Ocr;
use crate::script_engine::{sleep, Frame, TaskState, STOP};
use crate::vision::{pixel_like, MatchReport, TemplateSet, Vision};
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
