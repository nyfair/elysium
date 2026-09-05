use anyhow::{Context, Result};
use rhai::{AST, CallFnOptions, Engine, Scope};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::input::*;
use crate::script_engine::{sleep, Frame, TaskState, StopReason, STOP};
use crate::vision::{Vision, pixel_like};
use crate::worker;
use crate::{Args, GameType, k};

pub const WINDOW_TITLE: &str = "二重螺旋  ";

pub fn launch(args: &Args) -> Result<()> {
    let mut cfg = crate::load_launch_config("dna")?;
    if let Some(exe) = &args.exe {
        cfg.exec = exe.clone();
        crate::save_launch_config("dna", &cfg.exec, &cfg.login)?;
        println!("已更新启动配置：{}", cfg.exec);
    }
    if cfg.exec.is_empty() {
        anyhow::bail!("未配置游戏路径。用法：obs64 dna launch <游戏exe路径>");
    }
    println!("启动游戏：{}", cfg.exec);
    let mut child = Command::new(&cfg.exec)
        .spawn()
        .with_context(|| format!("启动进程失败：{}", cfg.exec))?;
    crate::wait_window(WINDOW_TITLE, &mut child, 30)?;
    if cfg.login.is_empty() {
        println!("未配置登录脚本，启动完成");
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
    log: Arc<dyn Fn(&str) + Send + Sync>,
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
            log("等待任务结束");
            let mut s = (*scope).clone();
            while !detect_ended(&engine, &ast, &mut s, &vision, has_script_ended)? {
                if exit.load(Ordering::SeqCst) {
                    return Ok(());
                }
                sleep(0.5);
            }
        }

        *k!(&state.pause) = false;
        let handle = worker::spawn_script(engine.clone(), ast.clone(), scope.clone(), log.clone());
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
            pixel_like(img, 879, 654, 0, 0, 0, 5) ||
            pixel_like(img, 880, 654, 0, 0, 0, 5) ||
            pixel_like(img, 881, 654, 0, 0, 0, 5)
        ) && (
            pixel_like(img, 1123, 654, 0, 0, 0, 5) ||
            pixel_like(img, 1124, 654, 0, 0, 0, 5) ||
            pixel_like(img, 1125, 654, 0, 0, 0, 5)
        ) && !pixel_like(img, 900, 681, 0, 0, 0, 5)
    )
}

pub fn task_started(img: Frame) -> Result<bool> {
    let img = &*k!(img);
    Ok(
        pixel_like(img, 115, 695, 255, 255, 255, 5) &&
        pixel_like(img, 225, 695, 255, 255, 255, 5) &&
        !pixel_like(img, 115, 689, 255, 255, 255, 5)
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
