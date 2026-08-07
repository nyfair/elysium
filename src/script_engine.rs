use rhai::{Array, Engine};

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crate::vision::{Vision, AssetMap, get_pixel, pixel_equal, pixel_like, ncc_match};
use crate::input::*;
use crate::k;

type Frame = Arc<Mutex<image::ImageBuffer<image::Rgb<u8>, Vec<u8>>>>;

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

pub fn new_engine(
    vision: &Arc<Vision>,
    pad: &Arc<Mutex<Gamepad>>,
    assets: &Arc<AssetMap>,
    state: &TaskState,
) -> Engine {
    let mut engine = Engine::new();
    engine.register_fn("set_stop", move |val: bool| STOP.store(val, Ordering::SeqCst));
    engine.register_fn("get_stop", move || -> bool { STOP.load(Ordering::SeqCst) });
    let pa = state.pause.clone();
    engine.register_fn("set_pause", move |val: bool| *k!(pa) = val);
    let pa = state.pause.clone();
    engine.register_fn("get_pause", move || -> bool { *k!(pa) });
    let t = state.cur_turn.clone();
    engine.register_fn("set_turn", move |val: i64| *k!(t) = val);
    let t = state.cur_turn.clone();
    engine.register_fn("get_turn", move || -> i64 { *k!(t) });
    engine.register_fn("sleep", |d: f64| { sleep(d) });
    let v = vision.clone();
    engine.register_fn("shot", move || -> Frame {
        Arc::new(Mutex::new(v.shot().unwrap()))
    });
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
    engine.register_fn("press", move |btn: &str| k!(p).press(parse_button(btn), 0.1));
    let p = pad.clone();
    engine.register_fn("press", move |btn: &str, hold: f64| k!(p).press(parse_button(btn), hold));
    let p = pad.clone();
    engine.register_fn("press_raw", move |btn: &str| k!(p).press_raw(parse_button(btn)));
    let p = pad.clone();
    engine.register_fn("release", move |btn: &str| k!(p).release(parse_button(btn), 0.1));
    let p = pad.clone();
    engine.register_fn("release", move |btn: &str, post: f64| k!(p).release(parse_button(btn), post));
    let p = pad.clone();
    engine.register_fn("release_raw", move |btn: &str| k!(p).release_raw(parse_button(btn)));
    let p = pad.clone();
    engine.register_fn("pad_update", move || k!(p).update());
    let p = pad.clone();
    engine.register_fn("click", move |btn: &str| k!(p).click(parse_button(btn), 0.1, 0.1));
    let p = pad.clone();
    engine.register_fn("click", move |btn: &str, hold: f64| k!(p).click(parse_button(btn), hold, 0.1));
    let p = pad.clone();
    engine.register_fn("click", move |btn: &str, hold: f64, post: f64| k!(p).click(parse_button(btn), hold, post));
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

    engine.on_print(|s: &str| println!("{s}"));
    engine
}

fn parse_button(name: &str) -> vigem_client::XButtons {
    match name {
        "A" => A,
        "B" => B,
        "X" => X,
        "Y" => Y,
        "LB" => LB,
        "RB" => RB,
        "LS" => LS,
        "RS" => RS,
        "UP" => UP,
        "DOWN" => DOWN,
        "LEFT" => LEFT,
        "RIGHT" => RIGHT,
        "START" => START,
        "BACK" => BACK,
        "GUIDE" => GUIDE,
        "LT" => LT,
        "RT" => RT,
        _ => GUIDE,
    }
}
