use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use rhai::{AST, CallFnOptions, Engine, Scope};

use crate::input::*;
use crate::script_engine::{sleep, Frame, TaskState, StopReason, STOP};
use crate::vision::{Vision, pixel_equal, pixel_like};
use crate::worker::{check, spawn_script};
use crate::{Args, GameType, k, log};

pub const WINDOW_TITLE: &str = "二重螺旋  ";

pub fn launch(args: &Args) -> Result<()> {
    let mut cfg = crate::load_launch_config("dna")?;
    if let Some(exe) = &args.exe {
        cfg.exec = exe.clone();
        crate::save_launch_config("dna", &cfg.exec, &cfg.login)?;
        log!("已更新启动配置：{}", cfg.exec);
    }
    if cfg.exec.is_empty() {
        anyhow::bail!("未配置游戏路径。用法：obs64 dna launch <游戏exe路径>");
    }
    log!("启动游戏：{}", cfg.exec);
    let mut child = Command::new(&cfg.exec)
        .spawn()
        .with_context(|| format!("启动进程失败：{}", cfg.exec))?;
    crate::wait_window(WINDOW_TITLE, &mut child, 30)?;
    if cfg.login.is_empty() {
        log!("未配置登录脚本，启动完成");
        return Ok(());
    }
    crate::run_cli(args, GameType::Dna, &cfg.login)
}

pub fn setup_engine(
    engine: &mut Engine,
    pad: &Arc<Mutex<Gamepad>>,
    state: &TaskState,
) {
    let p = pad.clone();
    let pa = state.pause.clone();
    engine.register_fn("run_combo", move |s: &str| run_combo(s, &p, &pa));
    let ctrl = Arc::new(ComboCtrl::new(pad.clone(), state.pause.clone()));
    if let Some(old) = k!(COMBO_CTRL).clone() {
        old.alive.store(false, Ordering::SeqCst);
    }
    *k!(COMBO_CTRL) = Some(ctrl.clone());
    engine.register_fn("auto_combo", move |s: &str| ctrl.start(s.to_string()));
    let p = pad.clone();
    engine.register_fn("reset_task", move || reset_task(&p));
    engine.register_fn("task_started", |img: Frame| -> bool { task_started(img).unwrap() });
    engine.register_fn("task_ended", |img: Frame| -> bool { task_ended(img).unwrap() });
}

pub fn run(
    engine: Arc<Engine>,
    ast: Arc<AST>,
    scope: Arc<Scope<'static>>,
    state: &TaskState,
    exit: Arc<AtomicBool>,
    reset: Arc<AtomicBool>,
    timeout: std::time::Duration,
    vision: Arc<Vision>,
    pad: Arc<Mutex<Gamepad>>,
    loop_enabled: bool,
) -> Result<()> {
    let has_script_ended = ast
        .iter_functions()
        .any(|f| f.name == "task_ended");

    loop {
        if exit.load(Ordering::SeqCst) {
            break;
        }
        STOP.store(false, Ordering::SeqCst);
        if loop_enabled {
            log!("等待任务结束");
            let mut s = (*scope).clone();
            while !detect_ended(&engine, &ast, &mut s, &vision, has_script_ended)? {
                if exit.load(Ordering::SeqCst) {
                    return Ok(());
                }
                sleep(0.5);
            }
        }

        *k!(&state.pause) = false;
        let handle = spawn_script(engine.clone(), ast.clone(), scope.clone());
        let start = Instant::now();
        let mut started = false;

        let reason = loop {
            if loop_enabled {
                if !started && start.elapsed() >= Duration::from_secs(5) {
                    started = true;
                }
                let mut s = (*scope).clone();
                if started && detect_ended(&engine, &ast, &mut s, &vision, has_script_ended)? {
                    sleep(0.5);
                    if detect_ended(&engine, &ast, &mut s, &vision, has_script_ended)? {
                        STOP.store(true, Ordering::SeqCst);
                        break StopReason::TaskEnded;
                    }
                }
            }
            if let Some(r) = check(&handle, &exit, &reset, timeout, start) {
                break r;
            }
            sleep(0.5);
        };
        let _ = handle.join();
        stop_combo();
        STOP.store(false, Ordering::SeqCst);

        match reason {
            StopReason::Reset => {
                log!("手动重置");
                reset_task(&pad);
            }
            StopReason::Timeout => {
                log!("任务超时，正在重置");
                reset_task(&pad);
                if !loop_enabled {
                    return Ok(());
                }
            }
            StopReason::Exit => return Ok(()),
            StopReason::Finished => {
                if !loop_enabled {
                    return Ok(());
                }
            }
            StopReason::TaskEnded => {}
        }
    }
    Ok(())
}

fn detect_ended(
    engine: &Engine,
    ast: &AST,
    scope: &mut Scope<'static>,
    vision: &Arc<Vision>,
    has_script_ended: bool,
) -> Result<bool> {
    let img = Arc::new(Mutex::new(vision.shot()?));
    if has_script_ended {
        engine
            .call_fn_with_options(
                CallFnOptions::new().eval_ast(false),
                scope,
                ast,
                "task_ended",
                (img,)
            )
            .map_err(|e| anyhow::anyhow!("{e}"))
    } else {
        task_ended(img)
    }
}

pub fn reset_task(pad: &Arc<Mutex<Gamepad>>) {
    k!(pad).reset();
    thread::sleep(Duration::from_secs(1));
    k!(pad).click(START, 0.1, 0.3);
    k!(pad).click(X, 0.1, 0.3);
    k!(pad).click(A, 0.1, 0.3);
}

pub fn task_ended(img: Frame) -> Result<bool> {
    let img = &*k!(img);
    Ok(
        ((
            pixel_equal(img, 879, 654, 0, 0, 0) ||
            pixel_equal(img, 880, 654, 0, 0, 0) ||
            pixel_equal(img, 881, 654, 0, 0, 0)
        ) && (
            pixel_equal(img, 1123, 654, 0, 0, 0) ||
            pixel_equal(img, 1124, 654, 0, 0, 0) ||
            pixel_equal(img, 1125, 654, 0, 0, 0)
        ) || (
            pixel_equal(img, 879, 663, 0, 0, 0) ||
            pixel_equal(img, 880, 663, 0, 0, 0) ||
            pixel_equal(img, 881, 663, 0, 0, 0)
        ) && (
            pixel_equal(img, 1123, 663, 0, 0, 0) ||
            pixel_equal(img, 1124, 663, 0, 0, 0) ||
            pixel_equal(img, 1125, 663, 0, 0, 0)
        ) || (
            pixel_equal(img, 879, 672, 0, 0, 0) ||
            pixel_equal(img, 880, 672, 0, 0, 0) ||
            pixel_equal(img, 881, 672, 0, 0, 0)
        ) && (
            pixel_equal(img, 1123, 672, 0, 0, 0) ||
            pixel_equal(img, 1124, 672, 0, 0, 0) ||
            pixel_equal(img, 1125, 672, 0, 0, 0)
        )) && !pixel_equal(img, 900, 681, 0, 0, 0)
    )
}

pub fn task_started(img: Frame) -> Result<bool> {
    let img = &*k!(img);
    Ok(
        pixel_like(img, 115, 690, 143, 209, 158, 5) &&
        pixel_like(img, 225, 690, 143, 209, 158, 5) &&
        !pixel_like(img, 115, 695, 143, 209, 158, 5)
    )
}

static COMBO_CTRL: Mutex<Option<Arc<ComboCtrl>>> = Mutex::new(None);

#[derive(Clone)]
struct ComboRequest {
    combo: String,
}

pub struct ComboCtrl {
    alive: Arc<AtomicBool>,
    epoch: Arc<AtomicU64>,
    slot: Mutex<Option<ComboRequest>>,
    wake: Mutex<Option<Sender<()>>>,
    pad: Arc<Mutex<Gamepad>>,
    pause: Arc<Mutex<bool>>,
}

impl ComboCtrl {
    fn new(pad: Arc<Mutex<Gamepad>>, pause: Arc<Mutex<bool>>) -> Self {
        Self {
            alive: Arc::new(AtomicBool::new(false)),
            epoch: Arc::new(AtomicU64::new(0)),
            slot: Mutex::new(None),
            wake: Mutex::new(None),
            pad,
            pause,
        }
    }

    fn start(self: &Arc<Self>, combo: String) {
        if !self.alive.load(Ordering::SeqCst) {
            self.alive.store(true, Ordering::SeqCst);
            let my_gen = self.epoch.fetch_add(1, Ordering::SeqCst) + 1;
            let (tx, rx) = std::sync::mpsc::channel();
            *k!(self.wake) = Some(tx);
            let c = self.clone();
            thread::Builder::new()
                .name("combo".into())
                .spawn(move || combo_worker(c, rx, my_gen))
                .expect("spawn combo thread");
        }
        let mut slot = k!(self.slot);
        if let Some(r) = &*slot
            && r.combo == combo {
                return;
            }
        *slot = Some(ComboRequest { combo });
        if let Some(tx) = k!(self.wake).clone() {
            let _ = tx.send(());
        }
    }

    fn run_request(&self, req: &ComboRequest, my_gen: u64) -> bool {
        loop {
            let mut i = 0;
            while i + 1 < req.combo.len() {
                if !self.alive.load(Ordering::SeqCst)
                    || self.epoch.load(Ordering::SeqCst) != my_gen
                    || STOP.load(Ordering::SeqCst)
                {
                    self.clear_if_current(req);
                    return false;
                }
                while *k!(self.pause) {
                    if !self.alive.load(Ordering::SeqCst)
                        || self.epoch.load(Ordering::SeqCst) != my_gen
                        || STOP.load(Ordering::SeqCst)
                    {
                        self.clear_if_current(req);
                        return false;
                    }
                    sleep(0.1);
                }
                if self.slot_changed(req) {
                    return true;
                }
                let ch = &req.combo[i..i + 1];
                i += 1;
                let num: f64 = req.combo[i..i + 1].parse().unwrap_or(0.);
                i += 1;
                combo_step(&self.pad, ch, num);
            }
        }
    }

    fn slot_changed(&self, req: &ComboRequest) -> bool {
        match &*k!(self.slot) {
            Some(r) => r.combo != req.combo,
            None => true,
        }
    }

    fn clear_if_current(&self, req: &ComboRequest) {
        let mut slot = k!(self.slot);
        if let Some(r) = &*slot
            && r.combo == req.combo {
                *slot = None;
            }
    }
}

pub fn stop_combo() {
    if let Some(c) = k!(COMBO_CTRL).clone() {
        c.alive.store(false, Ordering::SeqCst);
    }
}

fn combo_worker(ctrl: Arc<ComboCtrl>, rx: Receiver<()>, my_gen: u64) {
    loop {
        let req = loop {
            if !ctrl.alive.load(Ordering::SeqCst) || ctrl.epoch.load(Ordering::SeqCst) != my_gen {
                return;
            }
            if let Some(r) = k!(ctrl.slot).clone() {
                break r;
            }
            match rx.recv_timeout(Duration::from_millis(200)) {
                Ok(()) => continue,
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => return,
            }
        };
        if !ctrl.run_request(&req, my_gen) {
            return;
        }
    }
}

fn run_combo(combo_str: &str, pad: &Mutex<Gamepad>, pause: &Mutex<bool>) {
    let len = combo_str.len();
    let mut i = 0;
    while i + 1 < len {
        if STOP.load(Ordering::SeqCst) { break; }
        while *k!(pause) { sleep(0.1) }
        let ch = &combo_str[i..i+1];
        i += 1;
        let num: f64 = combo_str[i..i+1].parse().unwrap_or(0.);
        i += 1;
        combo_step(pad, ch, num);
    }
}

fn combo_step(pad: &Mutex<Gamepad>, ch: &str, num: f64) {
    match ch {
        "j" => for _ in 0..(num as i32) { k!(pad).click(LS, 0.1, 1.1) },
        "l" => for _ in 0..(num as i32) { k!(pad).click(X, 0.1, 0.1) },
        "L" => { k!(pad).click(X, 0.6, (num - 0.6).max(0.1)) }
        "r" => { k!(pad).click(RT, (num - 0.1).max(0.1), 0.1) }
        "w" => {
            k!(pad).lstick(0, 10000, num.max(0.1));
            k!(pad).lstick(0, 0, 0.);
        }
        "s" => {
            k!(pad).lstick(0, -10000, num.max(0.1));
            k!(pad).lstick(0, 0, 0.);
        }
        "a" => {
            k!(pad).lstick(-10000, 0, num.max(0.1));
            k!(pad).lstick(0, 0, 0.);
        }
        "d" => {
            k!(pad).lstick(10000, 0, num.max(0.1));
            k!(pad).lstick(0, 0, 0.);
        }
        "q" => {
            k!(pad).press(LB, 0.1);
            k!(pad).click(Y, 0.1, 0.1);
            k!(pad).release(LB, (num - 0.3).max(0.1));
        }
        "Q" => {
            k!(pad).press(LB, 0.1);
            k!(pad).click(Y, 0.6, 0.1);
            k!(pad).release(LB, (num - 0.8).max(0.1));
        }
        "e" => {
            k!(pad).press(LB, 0.1);
            k!(pad).click(X, 0.1, 0.1);
            k!(pad).release(LB, (num - 0.3).max(0.1));
        }
        "E" => {
            k!(pad).press(LB, 0.1);
            k!(pad).click(X, 0.6, 0.1);
            k!(pad).release(LB, (num - 0.8).max(0.1));
        }
        "z" => {
            k!(pad).press(LB, 0.1);
            k!(pad).click(B, 0.1, 0.1);
            k!(pad).release(LB, (num - 0.3).max(0.1));
        }
        "p" => { sleep(num); }
        _ => {}
    }
}
