use anyhow::Result;
use rhai::{Engine, Scope, AST};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::input::*;
use crate::script_engine::{sleep, TaskState, STOP};
use crate::vision::{Vision, pixel_equal};
use crate::worker;
use crate::{Args, k};

pub const WINDOW_TITLE: &str = "二重螺旋  ";

pub fn setup_engine(
    engine: &mut Engine,
    vision: &Arc<Vision>,
    pad: &Arc<Mutex<Gamepad>>,
    state: &TaskState,
) {
    let p = pad.clone();
    let pa = state.pause.clone();
    engine.register_fn("run_combo", move |s: &str| run_combo(s, &p, &pa));
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
        if loop_enabled {
            log("等待任务结束");
            while !detect_ended(&engine, &ast, &mut detect_scope, &vision, has_script_ended)? {
                if exit.load(Ordering::SeqCst) {
                    return Ok(());
                }
                sleep(0.5);
            }
        }

        STOP.store(false, Ordering::SeqCst);
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
                break match r {
                    worker::StopReason::Finished => StopReason::Finished,
                    worker::StopReason::Exit => StopReason::Exit,
                    worker::StopReason::Reset => StopReason::Reset,
                    worker::StopReason::Timeout => StopReason::Timeout,
                };
            }
            sleep(0.5);
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
    thread::sleep(Duration::from_secs(1));
    k!(pad).click(START, 0.1, 0.3);
    k!(pad).click(X, 0.1, 0.3);
    k!(pad).click(A, 0.1, 0.3);
}

pub fn task_ended(vision: &Arc<Vision>) -> Result<bool> {
    let img = vision.shot()?;
    Ok(
        pixel_equal(&img, 881, 655, 0, 0, 0) &&
        pixel_equal(&img, 1123, 655, 0, 0, 0) &&
        !pixel_equal(&img, 900, 681, 0, 0, 0)
    )
}

pub fn task_started(vision: &Arc<Vision>) -> Result<bool> {
    let img = vision.shot()?;
    Ok(
        pixel_equal(&img, 99, 695, 255, 255, 255) &&
        pixel_equal(&img, 241, 695, 255, 255, 255) &&
        !pixel_equal(&img, 99, 689, 255, 255, 255)
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
