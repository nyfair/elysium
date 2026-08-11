use anyhow::{bail, Context, Result};
use image::{ImageBuffer, Rgb};
use rhai::{Array, Dynamic, Engine, AST};
use std::collections::{HashMap, HashSet};
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use crate::k;
use crate::script_engine::{sleep, Frame, TaskState, STOP};
use crate::vision::{MatchReport, TemplateSet};
use crate::worker;
use crate::Args;

pub const WINDOW_TITLE: &str = "异环  ";

const CHARACTER_JSON: &str = "nte/DataTable/Character/DT_Character.json";
const AVATAR_DIR: &str = "nte/UI_Icon/AvatarImage/CustomAvatar/256";

pub const AVATAR_ROIS: [(u32, u32, u32, u32); 4] = [
    (1162, 133, 64, 64),
    (1162, 221, 64, 64),
    (1162, 309, 64, 64),
    (1162, 397, 64, 64),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Element {
    Unknown,
    Incantation,
    Chaos,
    Lakshana,
    Nature,
    Psyche,
    Cosmos,
}

impl FromStr for Element {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "咒" => Ok(Self::Incantation),
            "暗" => Ok(Self::Chaos),
            "相" => Ok(Self::Lakshana),
            "灵" => Ok(Self::Nature),
            "魂" => Ok(Self::Psyche),
            "光" => Ok(Self::Cosmos),
            _ => Err(()),
        }
    }
}

impl Element {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Unknown => "",
            Self::Incantation => "咒",
            Self::Chaos => "暗",
            Self::Lakshana => "相",
            Self::Nature => "灵",
            Self::Psyche => "魂",
            Self::Cosmos => "光",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Character {
    pub name: String,
    pub asset_name: String,
    pub tag: Vec<String>,
    pub element: Element,
    pub debug_info: Option<String>,
}

impl Default for Character {
    fn default() -> Self {
        Self {
            name: String::new(),
            asset_name: String::new(),
            tag: Vec::new(),
            element: Element::Unknown,
            debug_info: None,
        }
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
        let element = Element::from_str(element_str)
            .map_err(|_| anyhow::anyhow!("未知元素类型：{element_str}（角色 {name}）"))?;
        let tag = info["PlayerViewTagArray"]
            .as_array()
            .map(|a| a.iter().filter_map(|t| t.as_str().map(String::from)).collect())
            .unwrap_or_default();
        out.push(Character { name, asset_name: asset, tag, element, debug_info: None });
    }
    Ok(out)
}

const FEATURE_SIZE: u32 = 64;
const MASK_RADIUS_RATIO: f32 = 0.42;
const SCORE_MIN: f32 = 0.55;
const SCORE_GAP: f32 = 0.01;
const VARIANCE_MIN: f32 = 0.025;

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
        files.sort();
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

pub fn setup_engine(engine: &mut Engine, matcher: &Arc<AvatarMatcher>, _state: &TaskState) {
    engine.register_fn("name", |c: &mut Character| c.name.clone());
    engine.register_fn("tag", |c: &mut Character| -> Array {
        c.tag.iter().map(|t| t.into()).collect()
    });
    engine.register_fn("element", |c: &mut Character| c.element.as_str().to_string());
    engine.register_fn("debug_info", |c: &mut Character| {
        c.debug_info.clone().unwrap_or_default()
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
}

pub fn run(
    engine: Arc<Engine>,
    ast: Arc<AST>,
    args: Args,
    exit: Arc<AtomicBool>,
    reset: Arc<AtomicBool>,
    timeout: std::time::Duration,
    log: Arc<dyn Fn(&str) + Send + Sync>,
) -> Result<()> {
    STOP.store(false, Ordering::SeqCst);
    let handle = worker::spawn_script(engine, ast, args, log);
    let start = Instant::now();
    loop {
        if worker::check(&handle, &exit, &reset, timeout, start).is_some() {
            break;
        }
        sleep(0.1);
    }
    let _ = handle.join();
    STOP.store(false, Ordering::SeqCst);
    Ok(())
}
