use rhai::{Array, Dynamic, Engine, Map, Module, Scope};

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::vision::{Vision, AssetMap, get_pixel, ncc_match, pixel_equal, pixel_like, scale_roi};
use crate::input::{Button, Gamepad};
use crate::{Args, k};
#[cfg(feature = "ocr")]
use crate::ocr::Ocr;

pub type Frame = Arc<Mutex<image::ImageBuffer<image::Rgb<u8>, Vec<u8>>>>;

pub static STOP: AtomicBool = AtomicBool::new(false);

pub fn sleep(secs: f64) {
    let mut waited = 0.0f64;
    while waited < secs {
        if STOP.load(Ordering::SeqCst) {
            break;
        }
        let step = 0.2f64.min(secs - waited);
        thread::sleep(Duration::from_secs_f64(step));
        waited += step;
    }
}

pub fn init_scope(args: &Args) -> Scope<'static> {
    let mut scope = Scope::new();
    scope.push_constant("BOOST", args.boost as i64);
    scope.push_constant("STRATEGY", args.strategy.clone());
    scope.push_constant("COMBO", args.combo.clone());
    scope.push_constant("TURN", args.turn as i64);
    scope.push_constant("TIMEOUT", args.timeout);
    scope
}

pub struct TaskState {
    pub pause: Arc<Mutex<bool>>,
    pub cur_turn: Arc<Mutex<i64>>,
}

impl Default for TaskState {
    fn default() -> Self {
        Self {
            pause: Arc::new(Mutex::new(false)),
            cur_turn: Arc::new(Mutex::new(1)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StopReason {
    Finished,
    Exit,
    Reset,
    Timeout,
    TaskEnded,
}

pub struct ScriptMeta {
    pub author: String,
    pub desc: String,
    pub r#loop: bool,
    pub timeout: u64,
}

impl ScriptMeta {
    pub const DEFAULT_TIMEOUT: u64 = 86400;

    pub fn parse(script: &str) -> Self {
        let mut lines = Vec::new();
        let mut in_block = false;
        for line in script.lines() {
            let t = line.trim();
            if let Some(rest) = t.strip_prefix("//") {
                let c = rest.trim();
                if c.starts_with('{') {
                    in_block = true;
                }
                if in_block {
                    lines.push(c);
                }
                if in_block && c.ends_with('}') {
                    break;
                }
            } else if in_block {
                break;
            }
        }
        if lines.is_empty() {
            return Self::default();
        }
        let v: serde_json::Value =
            serde_json::from_str(&lines.join("\n")).unwrap_or(serde_json::Value::Null);
        if v.is_null() {
            return Self::default();
        }
        Self {
            author: v.get("author").and_then(|x| x.as_str()).unwrap_or("").to_string(),
            desc: v.get("desc").and_then(|x| x.as_str()).unwrap_or("").to_string(),
            r#loop: v.get("loop").and_then(|x| x.as_bool()).unwrap_or(true),
            timeout: v.get("timeout").and_then(|x| x.as_u64()).unwrap_or(Self::DEFAULT_TIMEOUT),
        }
    }
}

impl Default for ScriptMeta {
    fn default() -> Self {
        Self {
            author: String::new(),
            desc: String::new(),
            r#loop: true,
            timeout: Self::DEFAULT_TIMEOUT,
        }
    }
}

pub fn new_engine(
    vision: &Arc<Vision>,
    pad: &Arc<Mutex<Gamepad>>,
    assets: &Arc<AssetMap>,
    state: &TaskState,
    #[cfg(feature = "ocr")]
    ocr: &Arc<Ocr>,
) -> Engine {
    let mut engine = Engine::new();
    engine.register_type_with_name::<Button>("Button");
    let mut module = Module::new();
    for (name, btn) in [
        ("UP", Button::Up),
        ("DOWN", Button::Down),
        ("LEFT", Button::Left),
        ("RIGHT", Button::Right),
        ("START", Button::Start),
        ("BACK", Button::Back),
        ("LS", Button::LS),
        ("RS", Button::RS),
        ("LB", Button::LB),
        ("RB", Button::RB),
        ("GUIDE", Button::Guide),
        ("A", Button::A),
        ("B", Button::B),
        ("X", Button::X),
        ("Y", Button::Y),
        ("LT", Button::LT),
        ("RT", Button::RT),
    ] {
        module.set_var(name, Dynamic::from(btn));
    }
    engine.register_global_module(module.into());
    engine.on_progress(move |_ops| {
        if STOP.load(Ordering::SeqCst) {
            Some(Dynamic::from(StopReason::Exit))
        } else {
            None
        }
    });
    engine.register_raw_fn("rand", [std::any::TypeId::of::<i64>()], |_, args| {
        let max_val = args[0].as_int().unwrap_or(1);
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_micros() as i64;
        let limit = if max_val <= 0 { 1 } else { max_val };
        Ok(nanos % limit)
    });
    let pa = state.pause.clone();
    engine.register_fn("set_pause", move |val: bool| *k!(pa) = val);
    let pa = state.pause.clone();
    engine.register_fn("get_pause", move || -> bool { *k!(pa) });
    let t = state.cur_turn.clone();
    engine.register_fn("set_turn", move |val: i64| *k!(t) = val);
    let t = state.cur_turn.clone();
    engine.register_fn("get_turn", move || -> i64 { *k!(t) });
    engine.register_fn("wait", |d: f64| { sleep(d) });
    let v = vision.clone();
    engine.register_fn("shot", move || -> Frame {
        Arc::new(Mutex::new(v.shot().unwrap()))
    });
    engine.register_fn("load", |path: &str| -> Result<Frame, Box<rhai::EvalAltResult>> {
        match image::open(path) {
            Ok(img) => Ok(Arc::new(Mutex::new(img.to_rgb8()))),
            Err(e) => Err(format!("load_img 失败：{e}").into()),
        }
    });
    engine.register_fn("save", move |img: Frame, path: &str| {
        let i = k!(img);
        i.save(path).unwrap();
    });
    engine.register_fn("crop", move |img: Frame, x: i64, y: i64, w: i64, h: i64| -> Frame {
        let orig = k!(img);
        let (rx, ry, rw, rh) = scale_roi(orig.width(), orig.height(), (x as u32, y as u32, w as u32, h as u32));
        let crop = image::imageops::crop_imm(&*orig, rx, ry, rw, rh).to_image();
        Arc::new(Mutex::new(crop))
    });
    engine.register_fn("get_pixel", move |img: Frame, x: i64, y: i64| -> Array {
        let p = get_pixel(&k!(img), x as u32, y as u32);
        vec![(p[0] as i64).into(), (p[1] as i64).into(), (p[2] as i64).into()]
    });
    engine.register_fn("pixel_equal", move |img: Frame, x: i64, y: i64, r: i64, g: i64, b: i64| -> bool {
        pixel_equal(&k!(img), x as u32, y as u32, r as u8, g as u8, b as u8)
    });
    engine.register_fn("pixel_like", move |img: Frame, x: i64, y: i64, r: i64, g: i64, b: i64, v: i64| -> bool {
        pixel_like(&k!(img), x as u32, y as u32, r as u8, g as u8, b as u8, v as u8)
    });
    let a = assets.clone();
    engine.register_fn("ncc_match", move |img: Frame, tplt: &str| -> Array {
        let img = k!(img);
        let tpl = a.get(tplt).unwrap();
        let (x, y, score) = ncc_match(&img, tpl, None);
        vec![(x as i64).into(), (y as i64).into(), score.into()]
    });
    let a = assets.clone();
    engine.register_fn("ncc_match", move |img: Frame, tplt: &str, x: i64, y: i64, w: i64, h: i64| -> Array {
        let img = k!(img);
        let tpl = a.get(tplt).unwrap();
        let roi = if w > 0 && h > 0 {
            Some((x as u32, y as u32, w as u32, h as u32))
        } else {
            None
        };
        let (x, y, score) = ncc_match(&img, tpl, roi);
        vec![(x as i64).into(), (y as i64).into(), score.into()]
    });

    let p = pad.clone();
    engine.register_fn("press", move |btn: Button| k!(p).press(btn.xbox(), 0.1));
    let p = pad.clone();
    engine.register_fn("press", move |btn: Button, hold: f64| k!(p).press(btn.xbox(), hold));
    let p = pad.clone();
    engine.register_fn("press_raw", move |btn: Button| k!(p).press_raw(btn.xbox()));
    let p = pad.clone();
    engine.register_fn("release", move |btn: Button| k!(p).release(btn.xbox(), 0.1));
    let p = pad.clone();
    engine.register_fn("release", move |btn: Button, post: f64| k!(p).release(btn.xbox(), post));
    let p = pad.clone();
    engine.register_fn("release_raw", move |btn: Button| k!(p).release_raw(btn.xbox()));
    let p = pad.clone();
    engine.register_fn("pad_update", move || k!(p).update());
    let p = pad.clone();
    engine.register_fn("click", move |btn: Button| k!(p).click(btn.xbox(), 0.1, 0.1));
    let p = pad.clone();
    engine.register_fn("click", move |btn: Button, hold: f64| k!(p).click(btn.xbox(), hold, 0.1));
    let p = pad.clone();
    engine.register_fn("click", move |btn: Button, hold: f64, post: f64| k!(p).click(btn.xbox(), hold, post));
    let p = pad.clone();
    engine.register_fn("pad_reset", move || k!(p).reset());
    let p = pad.clone();
    engine.register_fn("lstick", move || k!(p).lstick(0, 0, 0.));
    let p = pad.clone();
    engine.register_fn("lstick", move |x: i64, y: i64| k!(p).lstick(x as i16, y as i16, 0.));
    let p = pad.clone();
    engine.register_fn("lstick", move |x: i64, y: i64, dur: f64| k!(p).lstick(x as i16, y as i16, dur));
    let p = pad.clone();
    engine.register_fn("rstick", move || k!(p).rstick(0, 0, 0.));
    let p = pad.clone();
    engine.register_fn("rstick", move |x: i64, y: i64| k!(p).rstick(x as i16, y as i16, 0.));
    let p = pad.clone();
    engine.register_fn("rstick", move |x: i64, y: i64, dur: f64| k!(p).rstick(x as i16, y as i16, dur));

    #[cfg(feature = "ocr")]
    {
        let o = ocr.clone();
        engine.register_fn("ocr_info", move |img: Frame, x: i64, y: i64, w: i64, h: i64| -> Array {
            ocr_info_roi(&o, &k!(img), x, y, w, h)
        });
        let o = ocr.clone();
        engine.register_fn("ocr", move |img: Frame, x: i64, y: i64, w: i64, h: i64| -> Dynamic {
            ocr_roi(&o, &k!(img), x, y, w, h).into()
        });
    }

    engine.on_print(|s: &str| println!("{s}"));
    engine
}

#[cfg(feature = "ocr")]
fn ocr_info_roi(
    ocr: &Ocr,
    img: &image::ImageBuffer<image::Rgb<u8>, Vec<u8>>,
    x: i64,
    y: i64,
    w: i64,
    h: i64,
) -> Array {
    match ocr.recognize_roi(img, (x as u32, y as u32, w as u32, h as u32)) {
        Ok(lines) => lines
            .into_iter()
            .map(|l| {
                let mut m = Map::new();
                m.insert("text".into(), l.text.into());
                m.insert("score".into(), (l.score as f64).into());
                m.insert("x".into(), (l.x as f64).into());
                m.insert("y".into(), (l.y as f64).into());
                m.insert("w".into(), (l.w as f64).into());
                m.insert("h".into(), (l.h as f64).into());
                Dynamic::from(m)
            })
            .collect(),
        Err(e) => {
            eprintln!("ocr 出错：{e}");
            Vec::new()
        }
    }
}

#[cfg(feature = "ocr")]
fn ocr_roi(
    ocr: &Ocr,
    img: &image::ImageBuffer<image::Rgb<u8>, Vec<u8>>,
    x: i64,
    y: i64,
    w: i64,
    h: i64,
) -> String {
    match ocr.recognize_roi(img, (x as u32, y as u32, w as u32, h as u32)) {
        Ok(lines) => lines.into_iter().map(|l| l.text).collect::<Vec<_>>().join("\n"),
        Err(e) => {
            eprintln!("ocr_text 出错：{e}");
            String::new()
        }
    }
}
