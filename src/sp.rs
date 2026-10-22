use std::process::Command;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use rhai::Engine;

use crate::input::*;
use crate::ocr::Ocr;
use crate::script_engine::TaskState;
use crate::vision::Vision;
use crate::{Args, GameType, k, log};

pub const LAUNCHER: &str = "白银之城";
pub const WINDOW_TITLE: &str = "SilverPalace  ";

pub fn launch(args: &Args) -> Result<()> {
    let mut cfg = crate::load_launch_config("sp")?;
    if let Some(exe) = &args.exe {
        cfg.exec = exe.clone();
        crate::save_launch_config("sp", &cfg.exec, &cfg.login)?;
        log!("已更新启动配置：{}，之后可使用 obs64 sp launch 启动游戏", cfg.exec);
    }
    if cfg.exec.is_empty() {
        anyhow::bail!("未配置启动器路径！使用方法：obs64 sp launch <安装目录\\Launcher.exe>");
    }
    log!("启动 {}", cfg.exec);
    let mut child = Command::new(&cfg.exec)
        .spawn()
        .with_context(|| format!("启动进程失败：{}", cfg.exec))?;
    let window = crate::wait_window(LAUNCHER, &mut child, 10)?;
    crate::vision::activate_window(&window, false);
    std::thread::sleep(std::time::Duration::from_millis(500));

    let vision = Vision::start(window)?;
    let ocr = Ocr::global().context("无法初始化 OCR 引擎")?;
    let (x, y) = vision.get_dimension();
    log!("启动器画面: {}x{}", x, y);
    let roi = ((x as f32 * 0.76) as u32, (y as f32 * 0.76) as u32, (x as f32 * 0.19) as u32, (y as f32 * 0.09) as u32);
    loop {
        let mut img = vision.shot()?;
        let mut lines = ocr.recognize_roi(&img, roi, false)?;
        let mut text = lines.iter().map(|l| l.text.trim()).collect::<Vec<_>>().join("\n");
        log!("{}", text);
        let px = roi.0 as f32 + lines[0].x + lines[0].w / 2.;
        let py = roi.1 as f32 + lines[0].y + lines[0].h / 2.;
        if lines[0].text.contains("开始游戏") {
            crate::post_click(window, px, py);
            break
        } else if lines[0].text.contains("更新") || lines[0].text.contains("下载") {
            crate::post_click(window, px, py);
            loop {
                std::thread::sleep(std::time::Duration::from_secs(2));
                img = vision.shot()?;
                lines = ocr.recognize_roi(&img, roi, false)?;
                text = lines.iter().map(|l| l.text.trim()).collect::<Vec<_>>().join("\n");
                if text.contains("开始") { break }
                log!("{}", text);
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    vision.stop();

    let window = crate::wait_window(WINDOW_TITLE, &mut child, 30)?;
    if cfg.login.is_empty() {
        log!("未配置登录脚本，启动完成");
        return Ok(());
    }
    std::thread::sleep(std::time::Duration::from_millis(2000));
    crate::vision::activate_window(&window, true);
    std::thread::sleep(std::time::Duration::from_millis(10000));
    crate::run_cli(args, GameType::Sp, &cfg.login)
}

pub fn setup_engine(
    _engine: &mut Engine,
    pad: &Arc<Mutex<Gamepad>>,
    _state: &TaskState,
) {
    k!(pad).click(LB, 0.0334, 0.);
}
