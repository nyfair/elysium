use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, Result};
use rhai::{Engine, Scope, AST};

use crate::script_engine::{sleep, TaskState, STOP};
use crate::worker::{check, spawn_script};
use crate::{Args, GameType, log};

pub const WINDOW_TITLE: &str = "AzurPromilia";

pub fn launch(args: &Args) -> Result<()> {
    let mut cfg = crate::load_launch_config("ap")?;
    if let Some(exe) = &args.exe {
        cfg.exec = exe.clone();
        crate::save_launch_config("ap", &cfg.exec, &cfg.login)?;
        log!("已更新启动配置：{}，之后可使用 obs64 ap launch 启动游戏", cfg.exec);
    }
    if cfg.exec.is_empty() {
        anyhow::bail!("未配置游戏路径！使用方法：obs64 ap launch <安装目录\\AzurPromilia.exe>");
    }
    log!("启动游戏：{}", cfg.exec);
    let mut child = Command::new(&cfg.exec)
        .arg("-start=azurpromilia_launcher")
        .spawn()
        .with_context(|| format!("启动进程失败：{}", cfg.exec))?;
    crate::wait_window(WINDOW_TITLE, &mut child, 30)?;
    if cfg.login.is_empty() {
        log!("未配置登录脚本，启动完成");
        return Ok(());
    }
    crate::run_cli(args, GameType::Ap, &cfg.login)
}

pub fn run(
    engine: Arc<Engine>,
    ast: Arc<AST>,
    scope: Arc<Scope<'static>>,
    _state: &TaskState,
    exit: Arc<AtomicBool>,
    reset: Arc<AtomicBool>,
    timeout: std::time::Duration,
) -> Result<()> {
    STOP.store(false, Ordering::SeqCst);
    let handle = spawn_script(engine.clone(), ast.clone(), scope.clone());
    let start = Instant::now();
    loop {
        if check(&handle, &exit, &reset, timeout, start).is_some() {
            break;
        }
        sleep(0.5);
    }
    let _ = handle.join();
    STOP.store(false, Ordering::SeqCst);
    Ok(())
}
