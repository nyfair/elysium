use anyhow::Result;
use rhai::{Engine, Scope, AST};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::input::*;
use crate::vision::{self, Vision};
use crate::worker;
use crate::{Args, k, sleep};

pub const WINDOW_TITLE: &str = "二重螺旋  ";

pub struct TaskState {
    pub stop: Arc<Mutex<bool>>,
    pub pause: Arc<Mutex<bool>>,
    pub cur_turn: Arc<Mutex<i64>>,
}

pub fn setup_engine(
    engine: &mut Engine,
    vision: &Arc<Vision>,
    pad: &Arc<Mutex<Gamepad>>,
    state: &TaskState,
) {
    let s = state.stop.clone();
    engine.register_fn("set_stop", move |val: bool| *k!(s) = val);
    let s = state.stop.clone();
    engine.register_fn("get_stop", move || -> bool { *k!(s) });
    let pa = state.pause.clone();
    engine.register_fn("set_pause", move |val: bool| *k!(pa) = val);
    let pa = state.pause.clone();
    engine.register_fn("get_pause", move || -> bool { *k!(pa) });
    let t = state.cur_turn.clone();
    engine.register_fn("set_turn", move |val: i64| *k!(t) = val);
    let t = state.cur_turn.clone();
    engine.register_fn("get_turn", move || -> i64 { *k!(t) });
    let p = pad.clone();
    let st = state.stop.clone();
    let pa = state.pause.clone();
    engine.register_fn("run_combo", move |s: &str| run_combo(s, &p, &st, &pa));
    let v = vision.clone();
    engine.register_fn("task_started", move || -> bool { task_started(&v).unwrap() });
    let v = vision.clone();
    engine.register_fn("task_ended", move || -> bool { task_ended(&v).unwrap() });
}

enum StopReason {
    Finished,
    Exit,
    Reset,
    Timeout,
    TaskEnded,
}

pub fn run(
    engine: Arc<Engine>,
    vision: Arc<Vision>,
    pad: Arc<Mutex<Gamepad>>,
    ast: Arc<AST>,
    args: Args,
    state: Arc<TaskState>,
    exit: Arc<AtomicBool>,
    reset: Arc<AtomicBool>,
    log: Arc<dyn Fn(&str) + Send + Sync>,
    has_script_ended: bool,
    loop_enabled: bool,
    meta_timeout: u64,
) -> Result<()> {
    let timeout = if args.timeout > 0 {
        Duration::from_secs(args.timeout)
    } else if meta_timeout > 0 {
        Duration::from_secs(meta_timeout)
    } else {
        Duration::from_secs(90)
    };
    let mut detect_scope = Scope::new();
    detect_scope.push_constant("BOOST", args.boost as i64);
    detect_scope.push_constant("STRATEGY", args.strategy.clone());
    detect_scope.push_constant("COMBO", args.combo.clone());
    detect_scope.push_constant("TURN", args.turn as i64);
    detect_scope.push_constant("TIMEOUT", args.timeout);

    loop {
        if exit.load(Ordering::SeqCst) {
            break;
        }
        if loop_enabled {
            log("等待任务结束");
            while !detect_ended(&engine, &ast, &mut detect_scope, &vision, has_script_ended)? {
                if exit.load(Ordering::SeqCst) {
                    return Ok(());
                }
                sleep!(0.5);
            }
        }

        *k!(&state.stop) = false;
        let handle = worker::spawn_script(engine.clone(), ast.clone(), args.clone(), log.clone());
        let start = Instant::now();
        let mut started = false;

        let reason = loop {
            if handle.is_finished() {
                break StopReason::Finished;
            }
            if exit.load(Ordering::SeqCst) {
                *k!(&state.stop) = true;
                break StopReason::Exit;
            }
            if reset.swap(false, Ordering::SeqCst) {
                *k!(&state.stop) = true;
                break StopReason::Reset;
            }
            if start.elapsed() > timeout {
                *k!(&state.stop) = true;
                break StopReason::Timeout;
            }
            if loop_enabled {
                if !started && start.elapsed() >= Duration::from_secs(5) {
                    started = true;
                }
                if started && detect_ended(&engine, &ast, &mut detect_scope, &vision, has_script_ended)? {
                    sleep!(0.5);
                    if detect_ended(&engine, &ast, &mut detect_scope, &vision, has_script_ended)? {
                        *k!(&state.stop) = true;
                        break StopReason::TaskEnded;
                    }
                }
            }
            sleep!(0.5);
        };
        let _ = handle.join();

        match reason {
            StopReason::Reset => {
                log("手动重置");
                reset_game(&pad);
            }
            StopReason::Timeout => {
                log("任务超时，正在重置");
                reset_game(&pad);
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
    scope: &mut Scope<'_>,
    vision: &Arc<Vision>,
    has_script_ended: bool,
) -> Result<bool> {
    if has_script_ended {
        engine
            .call_fn::<bool>(scope, ast, "task_ended", ())
            .map_err(|e| anyhow::anyhow!("{e}"))
    } else {
        task_ended(vision)
    }
}

fn reset_game(pad: &Arc<Mutex<Gamepad>>) {
    k!(pad).reset();
    sleep!(1);
    k!(pad).click(START, 0.1, 0.3);
    k!(pad).click(X, 0.1, 0.3);
    k!(pad).click(A, 0.1, 0.3);
}

pub fn task_ended(vision: &Arc<Vision>) -> Result<bool> {
    let img = vision.shot()?;
    Ok(
        vision::pixel_equal(&img, 881, 655, 0, 0, 0) &&
        vision::pixel_equal(&img, 1123, 655, 0, 0, 0) &&
        !vision::pixel_equal(&img, 900, 681, 0, 0, 0)
    )
}

pub fn task_started(vision: &Arc<Vision>) -> Result<bool> {
    let img = vision.shot()?;
    Ok(
        vision::pixel_equal(&img, 99, 695, 255, 255, 255) &&
        vision::pixel_equal(&img, 241, 695, 255, 255, 255) &&
        !vision::pixel_equal(&img, 99, 689, 255, 255, 255)
    )
}

fn run_combo(combo_str: &str, pad: &Mutex<Gamepad>, stop: &Mutex<bool>, pause: &Mutex<bool>) {
    let len = combo_str.len();
    let mut i = 0;
    while i + 1 < len {
        if *k!(stop) { break; }
        while *k!(pause) { sleep!(0.1) }
        let ch = &combo_str[i..i+1];
        i += 1;
        let num: f64 = combo_str[i..i+1].parse().unwrap_or(0.);
        i += 1;

        match ch {
            "j" => for _ in 0..(num as i32) { k!(pad).click(LS, 0.1, 1.1) },
            "l" => for _ in 0..(num as i32) { k!(pad).click(X, 0.1, 0.1) },
            "L" => { k!(pad).click(X, 0.6, (num - 0.6).max(0.1)) }
            "r" => { k!(pad).click(RT, (num - 0.1).max(0.1), 0.1) }
            "w" => {
                k!(pad).lstick(0, 10000, num);
                k!(pad).lstick(0, 0, 0.);
            }
            "s" => {
                k!(pad).lstick(0, -10000, num);
                k!(pad).lstick(0, 0, 0.);
            }
            "a" => {
                k!(pad).lstick(-10000, 0, num);
                k!(pad).lstick(0, 0, 0.);
            }
            "d" => {
                k!(pad).lstick(10000, 0, num);
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
            "p" => { sleep!(num); }
            _ => {}
        }
    }
}
