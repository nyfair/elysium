mod vision;
mod input;
mod script_engine;
mod worker;
mod tui;
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
use windows::Win32::UI::Shell::{IsUserAnAdmin, ShellExecuteW};
use windows::Win32::UI::WindowsAndMessaging;
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
    #[arg(short = 'p', long, default_value = "")]
    pub plan: String,
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
    let params = args.join(" ");
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
            WindowsAndMessaging::SW_SHOWNORMAL,
        )
    };
    if result.0 as isize <= 32 {
        bail!("以管理员身份启动失败（错误码 {}），请右键选择\"以管理员身份运行\"", result.0 as isize);
    }
    Ok(())
}

fn main() -> Result<()> {
    unsafe {
        let _ = WindowsAndMessaging::SetProcessDPIAware();
    }
    let args = Args::parse();

    let needs_priv = match &args.game {
        Some(_) => !matches!(args.task.as_deref(), Some("shot") | Some("act")),
        None => true,
    };
    if needs_priv && !is_admin() {
        println!("检测到沒有管理员权限，正在请求...");
        relaunch_as_admin()?;
        return Ok(());
    }

    let Some(game) = args.game.clone() else {
        return tui::run(&args);
    };
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
        vision::activate_window(&window);
        println!("窗口已激活");
        return Ok(());
    }

    run_cli(&args, game, task)
}

fn run_cli(args: &Args, game: GameType, task: &str) -> Result<()> {
    let window = Window::from_contains_name(game.title())
        .map_err(|e| anyhow::anyhow!("找不到游戏窗口：{}", e))?;
    vision::activate_window(&window);
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

    let script = std::fs::read_to_string(script_path(game.name(), task))
        .map_err(|e| anyhow::anyhow!("找不到任务脚本：{e}"))?;
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
    let exit = Arc::new(AtomicBool::new(false));
    let reset = Arc::new(AtomicBool::new(false));
    let meta = script_engine::ScriptMeta::parse(&script);
    let ast = Arc::new(engine.compile(&script)?);
    let timeout = if args.timeout > 0 {
        Duration::from_secs(args.timeout)
    } else {
        Duration::from_secs(meta.timeout)
    };
    let scope = Arc::new(script_engine::init_scope(args));

    match game {
        #[cfg(feature = "dna")]
        GameType::Dna => dna::run(
            Arc::new(engine),
            ast,
            scope,
            &state,
            exit,
            reset,
            timeout,
            log,
            vision,
            pad,
            meta.r#loop,
        )?,
        #[cfg(feature = "nte")]
        GameType::Nte => nte::run(
            Arc::new(engine),
            ast,
            scope,
            &state,
            exit,
            reset,
            timeout,
            log,
        )?,
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
