use rhai::{Array, Dynamic, Engine, Map, Module};

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::vision::{Vision, AssetMap, get_pixel, pixel_equal, pixel_like, ncc_match};
use crate::input::{Button, Gamepad};
use crate::k;
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

pub struct TaskState {
    pub pause: Arc<Mutex<bool>>,
    pub cur_turn: Arc<Mutex<i64>>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StopReason {
    Finished,
    Exit,
    Reset,
    Timeout,
    TaskEnded,
}

pub fn new_engine(
    vision: &Arc<Vision>,
    pad: &Arc<Mutex<Gamepad>>,
    assets: &Arc<AssetMap>,
    state: &TaskState,
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
    engine.register_raw_fn("rand", &[std::any::TypeId::of::<i64>()], |_, args| {
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
    engine.register_fn(
        "load_img",
        |path: &str| -> Result<Frame, Box<rhai::EvalAltResult>> {
            match image::open(path) {
                Ok(img) => Ok(Arc::new(Mutex::new(img.to_rgb8()))),
                Err(e) => Err(format!("load_img 失败：{e}").into()),
            }
        },
    );
    let v = vision.clone();
    engine.register_fn("shot", move |path: &str| -> Frame {
        let img = v.shot().unwrap();
        img.save(path).unwrap();
        Arc::new(Mutex::new(img))
    });
    engine.register_fn("get_pixel", move |img: Frame, x: i64, y: i64| -> Array {
        let p = get_pixel(&*k!(img), x as u32, y as u32);
        vec![(p[0] as i64).into(), (p[1] as i64).into(), (p[2] as i64).into()]
    });
    engine.register_fn("pixel_equal", move |img: Frame, x: i64, y: i64, r: i64, g: i64, b: i64| -> bool {
        pixel_equal(&*k!(img), x as u32, y as u32, r as u8, g as u8, b as u8)
    });
    engine.register_fn("pixel_like", move |img: Frame, x: i64, y: i64, r: i64, g: i64, b: i64, v: i64| -> bool {
        pixel_like(&*k!(img), x as u32, y as u32, r as u8, g as u8, b as u8, v as u8)
    });
    let a = assets.clone();
    engine.register_fn("ncc_match", move |img: Frame, tplt: &str| -> Array {
        let img = k!(img);
        let tpl = a.get(tplt).unwrap();
        let (x, y, score) = ncc_match(&img, tpl, None);
        vec![(x as i64).into(), (y as i64).into(), score.into()]
    });
    let a = assets.clone();
    engine.register_fn("ncc_match", move |img: Frame, tplt: &str, roi_x: i64, roi_y: i64, roi_w: i64, roi_h: i64| -> Array {
        let img = k!(img);
        let tpl = a.get(tplt).unwrap();
        let roi = if roi_w > 0 && roi_h > 0 {
            Some((roi_x as u32, roi_y as u32, roi_w as u32, roi_h as u32))
        } else {
            None
        };
        let (x, y, score) = ncc_match(&img, tpl, roi);
        vec![(x as i64).into(), (y as i64).into(), score.into()]
    });

    let p = pad.clone();
    engine.register_fn("press", move |btn: Button| k!(p).press(btn.to_xbuttons(), 0.1));
    let p = pad.clone();
    engine.register_fn("press", move |btn: Button, hold: f64| k!(p).press(btn.to_xbuttons(), hold));
    let p = pad.clone();
    engine.register_fn("press_raw", move |btn: Button| k!(p).press_raw(btn.to_xbuttons()));
    let p = pad.clone();
    engine.register_fn("release", move |btn: Button| k!(p).release(btn.to_xbuttons(), 0.1));
    let p = pad.clone();
    engine.register_fn("release", move |btn: Button, post: f64| k!(p).release(btn.to_xbuttons(), post));
    let p = pad.clone();
    engine.register_fn("release_raw", move |btn: Button| k!(p).release_raw(btn.to_xbuttons()));
    let p = pad.clone();
    engine.register_fn("pad_update", move || k!(p).update());
    let p = pad.clone();
    engine.register_fn("click", move |btn: Button| k!(p).click(btn.to_xbuttons(), 0.1, 0.1));
    let p = pad.clone();
    engine.register_fn("click", move |btn: Button, hold: f64| k!(p).click(btn.to_xbuttons(), hold, 0.1));
    let p = pad.clone();
    engine.register_fn("click", move |btn: Button, hold: f64, post: f64| k!(p).click(btn.to_xbuttons(), hold, post));
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

    let o = ocr.clone();
    engine.register_fn("ocr_info", move |img: Frame, x: i64, y: i64, w: i64, h: i64| -> Array {
        ocr_roi(&o, &k!(img), x, y, w, h, 2.)
    });
    let o = ocr.clone();
    engine.register_fn(
        "ocr_info",
        move |img: Frame, x: i64, y: i64, w: i64, h: i64, scale: f64| -> Array {
            ocr_roi(&o, &k!(img), x, y, w, h, scale as f32)
        },
    );
    let o = ocr.clone();
    engine.register_fn("ocr", move |img: Frame, x: i64, y: i64, w: i64, h: i64| -> Dynamic {
        ocr_text_roi(&o, &k!(img), x, y, w, h, 2.).into()
    });
    let o = ocr.clone();
    engine.register_fn(
        "ocr",
        move |img: Frame, x: i64, y: i64, w: i64, h: i64, scale: f64| -> Dynamic {
            ocr_text_roi(&o, &k!(img), x, y, w, h, scale as f32).into()
        },
    );

    engine.on_print(|s: &str| println!("{s}"));
    engine
}

fn ocr_roi(
    ocr: &Ocr,
    img: &image::ImageBuffer<image::Rgb<u8>, Vec<u8>>,
    x: i64,
    y: i64,
    w: i64,
    h: i64,
    scale: f32,
) -> Array {
    match ocr.recognize_roi(img, (x as u32, y as u32, w as u32, h as u32), scale) {
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

fn ocr_text_roi(
    ocr: &Ocr,
    img: &image::ImageBuffer<image::Rgb<u8>, Vec<u8>>,
    x: i64,
    y: i64,
    w: i64,
    h: i64,
    scale: f32,
) -> String {
    match ocr.recognize_roi(img, (x as u32, y as u32, w as u32, h as u32), scale) {
        Ok(lines) => lines.into_iter().map(|l| l.text).collect::<Vec<_>>().join("\n"),
        Err(e) => {
            eprintln!("ocr_text 出错：{e}");
            String::new()
        }
    }
}
