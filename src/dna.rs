use anyhow::Result;
use rhai::{CallFnOptions, Engine, Scope, AST};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::input::*;
use crate::script_engine::{sleep, Frame, TaskState, StopReason, STOP};
use crate::vision::{Vision, pixel_equal};
use crate::worker;
use crate::{Args, k};

pub const WINDOW_TITLE: &str = "二重螺旋  ";

pub fn setup_engine(
    engine: &mut Engine,
    pad: &Arc<Mutex<Gamepad>>,
    state: &TaskState,
) {
    let p = pad.clone();
    let pa = state.pause.clone();
    engine.register_fn("run_combo", move |s: &str| run_combo(s, &p, &pa));
    engine.register_fn("task_started", move |img: Frame| -> bool { task_started(img).unwrap() });
    engine.register_fn("task_ended", move |img: Frame| -> bool { task_ended(img).unwrap() });
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
    timeout: Duration,
) -> Result<()> {
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
        STOP.store(false, Ordering::SeqCst);
        if loop_enabled {
            log("等待任务结束");
            while !detect_ended(&engine, &ast, &mut detect_scope, &vision, has_script_ended)? {
                if exit.load(Ordering::SeqCst) {
                    return Ok(());
                }
                sleep(0.5);
            }
        }

        *k!(&state.pause) = false;
        *k!(&state.cur_turn) = 1;
        let handle = worker::spawn_script(engine.clone(), ast.clone(), args.clone(), log.clone());
        let start = Instant::now();
        let mut started = false;

        let reason = loop {
            if loop_enabled {
                if !started && start.elapsed() >= Duration::from_secs(5) {
                    started = true;
                }
                if started && detect_ended(&engine, &ast, &mut detect_scope, &vision, has_script_ended)? {
                    sleep(0.5);
                    if detect_ended(&engine, &ast, &mut detect_scope, &vision, has_script_ended)? {
                        STOP.store(true, Ordering::SeqCst);
                        break StopReason::TaskEnded;
                    }
                }
            }
            if let Some(r) = worker::check(&handle, &exit, &reset, timeout, start) {
                break r;
            }
            sleep(0.5);
        };
        let _ = handle.join();
        STOP.store(false, Ordering::SeqCst);

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

fn reset_game(pad: &Arc<Mutex<Gamepad>>) {
    k!(pad).reset();
    thread::sleep(Duration::from_secs(1));
    k!(pad).click(START, 0.1, 0.3);
    k!(pad).click(X, 0.1, 0.3);
    k!(pad).click(A, 0.1, 0.3);
}

pub fn task_ended(img: Frame) -> Result<bool> {
    let img = &*k!(img);
    Ok(
        (
            pixel_equal(img, 879, 654, 0, 0, 0) ||
            pixel_equal(img, 880, 654, 0, 0, 0) ||
            pixel_equal(img, 881, 654, 0, 0, 0)
        ) && (
            pixel_equal(img, 1123, 654, 0, 0, 0) ||
            pixel_equal(img, 1124, 654, 0, 0, 0) ||
            pixel_equal(img, 1125, 654, 0, 0, 0)
        ) && !pixel_equal(img, 900, 681, 0, 0, 0)
    )
}

pub fn task_started(img: Frame) -> Result<bool> {
    let img = &*k!(img);
    Ok(
        pixel_equal(img, 99, 695, 255, 255, 255) &&
        pixel_equal(img, 241, 695, 255, 255, 255) &&
        !pixel_equal(img, 99, 689, 255, 255, 255)
    )
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
            "p" => { sleep(num); }
            _ => {}
        }
    }
}
