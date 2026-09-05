#![allow(clippy::too_many_arguments)]
#![allow(clippy::type_complexity)]

mod vision;
mod input;
mod script_engine;
mod worker;
mod tui;
mod audio;
#[cfg(feature = "dna")]
mod dna;
#[cfg(feature = "nte")]
mod nte;
#[cfg(feature = "ocr")]
mod ocr;

use anyhow::{bail, Context, Result};
use clap::{Parser, ValueEnum};
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::Shell::{IsUserAnAdmin, ShellExecuteW};
use windows::Win32::UI::WindowsAndMessaging::{
    SW_SHOWNORMAL, WM_LBUTTONDOWN, WM_LBUTTONUP,
    PostMessageW, SetProcessDPIAware
};
use windows_capture::window::Window;

use crate::script_engine::TaskState;

#[macro_export]
macro_rules! k {
    ($mutex:expr) => {
        $mutex.lock().unwrap()
    };
}

#[derive(ValueEnum, Clone, Debug, PartialEq, Eq)]
pub enum GameType {
    #[cfg(feature = "dna")]
    Dna,
    #[cfg(feature = "nte")]
    Nte,
}

impl GameType {
    pub fn name(&self) -> &'static str {
        match self {
            #[cfg(feature = "dna")]
            GameType::Dna => "dna",
            #[cfg(feature = "nte")]
            GameType::Nte => "nte",
        }
    }

    pub fn display(&self) -> &'static str {
        match self {
            #[cfg(feature = "dna")]
            GameType::Dna => "两个陀螺",
            #[cfg(feature = "nte")]
            GameType::Nte => "海特洛",
        }
    }

    pub fn title(&self) -> &'static str {
        match self {
            #[cfg(feature = "dna")]
            GameType::Dna => dna::WINDOW_TITLE,
            #[cfg(feature = "nte")]
            GameType::Nte => nte::WINDOW_TITLE,
        }
    }
}

#[derive(Parser, Clone, Debug)]
#[command(name = "elysium")]
pub struct Args {
    pub game: Option<GameType>,
    pub task: Option<String>,
    pub exe: Option<String>,
    #[arg(short = 'p', long, num_args = 1..)]
    pub plan: Vec<String>,
    #[arg(short = 'b', long, default_value = "0")]
    pub boost: u32,
    #[arg(short = 's', long, default_value = "")]
    pub strategy: String,
    #[arg(short = 'c', long, default_value = "")]
    pub combo: String,
    #[arg(short = 't', long, default_value = "99")]
    pub turn: u32,
    #[arg(short = 'o', long, default_value = "0")]
    pub timeout: u64,
}

fn to_wide(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
}

fn is_admin() -> bool {
    unsafe { IsUserAnAdmin().as_bool() }
}

fn relaunch_as_admin() -> Result<()> {
    let exe = std::env::current_exe()?;
    let dir = std::env::current_dir()?;
    let args: Vec<String> = std::env::args().skip(1).collect();
    let params = args
        .iter()
        .map(|a| {
            if a.contains(' ') || a.contains('\t') {
                format!("\"{}\"", a.replace('"', "\\\""))
            } else {
                a.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    println!("提权命令行: {} {}", exe.to_string_lossy(), params);
    let runas = to_wide("runas");
    let exe_w = to_wide(&exe.to_string_lossy());
    let dir_w = to_wide(&dir.to_string_lossy());
    let params_w = to_wide(&params);
    let result = unsafe {
        ShellExecuteW(
            None,
            PCWSTR::from_raw(runas.as_ptr()),
            PCWSTR::from_raw(exe_w.as_ptr()),
            PCWSTR::from_raw(params_w.as_ptr()),
            PCWSTR::from_raw(dir_w.as_ptr()),
            SW_SHOWNORMAL,
        )
    };
    if result.0 as isize <= 32 {
        bail!("以管理员身份启动失败（错误码 {}），请右键选择\"以管理员身份运行\"", result.0 as isize);
    }
    Ok(())
}

fn main() -> Result<()> {
    unsafe {
        let _ = SetProcessDPIAware();
    }
    let args = Args::parse();
    if !is_admin() {
        println!("检测到沒有管理员权限，正在请求...");
        relaunch_as_admin()?;
        return Ok(());
    }

    let Some(game) = args.game.clone() else {
        return tui::run(&args);
    };
    if !args.plan.is_empty() {
        return run_plan(&args, game);
    }
    let Some(task) = &args.task else {
        bail!("缺少任务参数（示例：elysium dna 通用任务）");
    };

    if task == "shot" {
        let window = Window::from_contains_name(game.title())
            .map_err(|e| anyhow::anyhow!("找不到游戏窗口：{}", e))?;
        let vision = vision::Vision::start(window)?;
        vision.shot_to_file("shot.png")?;
        println!("截图已保存到 shot.png");
        return Ok(());
    }

    if task == "act" {
        let window = Window::from_contains_name(game.title())
            .map_err(|e| anyhow::anyhow!("找不到游戏窗口：{}", e))?;
        vision::activate_window(&window, false);
        println!("窗口已激活");
        return Ok(());
    }

    if task == "launch" {
        return match game {
            #[cfg(feature = "dna")]
            GameType::Dna => dna::launch(&args),
            #[cfg(feature = "nte")]
            GameType::Nte => nte::launch(&args),
        };
    }

    run_cli(&args, game, task)
}

struct CliResources {
    game: GameType,
    vision: Arc<vision::Vision>,
    pad: Arc<Mutex<input::Gamepad>>,
    state: Arc<TaskState>,
    engine: Arc<rhai::Engine>,
    log: Arc<dyn Fn(&str) + Send + Sync>,
}

fn init_cli(game: GameType) -> Result<CliResources> {
    let window = Window::from_contains_name(game.title())
        .map_err(|e| anyhow::anyhow!("找不到游戏窗口：{}", e))?;
    vision::activate_window(&window, false);
    let vision = Arc::new(vision::Vision::start(window)?);
    let pad = Arc::new(Mutex::new(input::Gamepad::new()
        .context("无法连接虚拟手柄：请以管理员身份运行，或确认已安装 ViGEmBus 驱动")?));
    let assets = Arc::new(vision::load_assets(game.name(), vision.get_dimension().1)?);
    #[cfg(feature = "ocr")]
    let ocr = ocr::Ocr::global().context("无法初始化 OCR 引擎")?;

    let state = Arc::new(TaskState::default());
    let mut engine = script_engine::new_engine(
        &vision, &pad, &assets, &state,
        #[cfg(feature = "ocr")]
        &ocr
    );
    let log: Arc<dyn Fn(&str) + Send + Sync> = Arc::new(|msg| println!("{msg}"));
    let l = log.clone();
    engine.on_print(move |msg: &str| l(msg));

    match game {
        #[cfg(feature = "dna")]
        GameType::Dna => {
            dna::setup_engine(&mut engine, &pad, &state);
        }
        #[cfg(feature = "nte")]
        GameType::Nte => {
            nte::setup_engine(&mut engine, &pad, &state, &vision, &ocr);
        }
    }
    Ok(CliResources {
        game,
        vision,
        pad,
        state,
        engine: Arc::new(engine),
        log,
    })
}

fn run_task(res: &CliResources, args: &Args, task: &str) -> Result<()> {
    let script = std::fs::read_to_string(script_path(res.game.name(), task))
        .map_err(|e| anyhow::anyhow!("找不到任务脚本：{e}"))?;
    let meta = script_engine::ScriptMeta::parse(&script);
    let ast = Arc::new(res.engine.compile(&script)?);
    let timeout = if args.timeout > 0 {
        Duration::from_secs(args.timeout)
    } else {
        Duration::from_secs(meta.timeout)
    };
    let scope = Arc::new(script_engine::init_scope(args));
    *k!(&res.state.pause) = false;
    *k!(&res.state.cur_turn) = 1;
    let exit = Arc::new(AtomicBool::new(false));
    let reset = Arc::new(AtomicBool::new(false));
    println!("开始任务：{task}");
    match res.game {
        #[cfg(feature = "dna")]
        GameType::Dna => dna::run(
            res.engine.clone(),
            ast,
            scope,
            &res.state,
            exit,
            reset,
            timeout,
            res.log.clone(),
            res.vision.clone(),
            res.pad.clone(),
            meta.r#loop,
        )?,
        #[cfg(feature = "nte")]
        GameType::Nte => nte::run(
            res.engine.clone(),
            ast,
            scope,
            &res.state,
            exit,
            reset,
            timeout,
            res.log.clone(),
        )?,
    }
        crate::audio::disable_all();
    println!("任务完成：{task}");
    Ok(())
}

pub(crate) fn run_cli(args: &Args, game: GameType, task: &str) -> Result<()> {
    let res = init_cli(game)?;
    run_task(&res, args, task)
}

fn run_plan(args: &Args, game: GameType) -> Result<()> {
    let res = init_cli(game)?;
    for item in &args.plan {
        let (task, params) = match item.split_once(':') {
            Some((t, p)) => (t, p),
            None => (item.as_str(), ""),
        };
        if task.is_empty() {
            bail!("计划项格式错误：{item}");
        }
        let mut task_args = args.clone();
        for kv in params.split(',') {
            if kv.is_empty() {
                continue;
            }
            let Some((k, v)) = kv.split_once('=') else {
                bail!("计划项参数格式错误：{kv}（示例：任务A:b=3,t=20）");
            };
            match k {
                "b" => task_args.boost = v.parse().context("boost 必须是数字")?,
                "s" => task_args.strategy = v.to_string(),
                "c" => task_args.combo = v.to_string(),
                "t" => task_args.turn = v.parse().context("turn 必须是数字")?,
                "o" => task_args.timeout = v.parse().context("timeout 必须是数字")?,
                _ => bail!("未知参数：{k}（可用 b/s/c/t/o）"),
            }
        }
        run_task(&res, &task_args, task)?;
    }
    Ok(())
}

fn script_path(game: &str, task: &str) -> std::path::PathBuf {
    let custom = std::path::Path::new(&format!("user-{game}-scripts")).join(format!("{task}.rhai"));
    if custom.is_file() {
        custom
    } else {
        std::path::Path::new(game).join("scripts").join(format!("{task}.rhai"))
    }
}

pub struct LaunchConfig {
    pub exec: String,
    pub login: String,
}

pub fn load_launch_config(game: &str) -> Result<LaunchConfig> {
    let path = format!("user-{game}-scripts/config.json");
    let text =
        std::fs::read_to_string(&path).with_context(|| format!("读取 {path} 失败（请先配置游戏路径）"))?;
    let v: serde_json::Value = serde_json::from_str(&text).context("解析 config.json 失败")?;
    Ok(LaunchConfig {
        exec: v["exec"].as_str().unwrap_or("").to_string(),
        login: v["login"].as_str().unwrap_or("").to_string(),
    })
}

pub fn save_launch_config(game: &str, exec: &str, login: &str) -> Result<()> {
    let path = format!("user-{game}-scripts/config.json");
    let v = serde_json::json!({ "exec": exec, "login": login });
    std::fs::write(&path, serde_json::to_string_pretty(&v)?).with_context(|| format!("写入 {path} 失败"))?;
    Ok(())
}

pub fn wait_window(
    title: &str,
    child: &mut std::process::Child,
    timeout_secs: u64,
) -> Result<windows_capture::window::Window> {
    let deadline = std::time::Instant::now() + Duration::from_secs(timeout_secs);
    loop {
        if let Ok(w) = Window::from_contains_name(title) {
            return Ok(w);
        }
        if child.try_wait()?.is_some() {
            std::thread::sleep(Duration::from_millis(1000));
            continue
        }
        if std::time::Instant::now() >= deadline {
            anyhow::bail!("等待窗口超时（{timeout_secs}s）：{title}");
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

pub fn post_click(window: Window, x: f32, y: f32) {
    unsafe {
        let hwnd = Some(HWND(window.as_raw_hwnd()));
        let lp = LPARAM(((y as isize & 0xFFFF) << 16) | (x as isize & 0xFFFF));
        let _ = PostMessageW(hwnd, WM_LBUTTONDOWN, WPARAM(1), lp);
        std::thread::sleep(std::time::Duration::from_millis(50));
        let _ = PostMessageW(hwnd, WM_LBUTTONUP, WPARAM(0), lp);
    }
}
