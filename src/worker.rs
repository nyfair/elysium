use anyhow::{Context, Result};
use rhai::{AST, Engine, Scope};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{Builder, JoinHandle};
use std::time::{Duration, Instant};

use crate::{Args, GameType, k};
use crate::input::Gamepad;
use crate::script_engine::{sleep, init_scope, new_engine, STOP, StopReason, ScriptMeta, TaskState};
use crate::vision::{activate_window, load_assets, AssetMap, Vision};

const MAX_LOGS: usize = 200;

#[derive(Clone)]
struct GameResources {
    pad: Arc<Mutex<Gamepad>>,
    assets: Arc<AssetMap>,
}

static RES_CACHE: Mutex<Option<(GameType, GameResources)>> = Mutex::new(None);

pub fn check(
    handle: &JoinHandle<()>,
    exit: &AtomicBool,
    reset: &AtomicBool,
    timeout: Duration,
    start: Instant,
) -> Option<StopReason> {
    if handle.is_finished() {
        return Some(StopReason::Finished);
    }
    if exit.load(Ordering::SeqCst) {
        STOP.store(true, Ordering::SeqCst);
        return Some(StopReason::Exit);
    }
    if reset.swap(false, Ordering::SeqCst) {
        STOP.store(true, Ordering::SeqCst);
        return Some(StopReason::Reset);
    }
    if start.elapsed() > timeout {
        STOP.store(true, Ordering::SeqCst);
        return Some(StopReason::Timeout);
    }
    None
}

pub struct SharedState {
    pub running: bool,
    pub logs: VecDeque<String>,
    pub started_at: Instant,
    pub error: Option<String>,
}

impl SharedState {
    pub fn new() -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(Self {
            running: false,
            logs: VecDeque::new(),
            started_at: Instant::now(),
            error: None,
        }))
    }

    pub fn push_log(shared: &Arc<Mutex<Self>>, msg: String) {
        let mut s = k!(shared);
        let elapsed = s.started_at.elapsed();
        s.logs.push_back(format!(
            "[{:02}:{:02}:{:02}] {}",
            elapsed.as_secs() / 60 / 60,
            elapsed.as_secs() / 60 % 60,
            elapsed.as_secs() % 60,
            msg
        ));
        while s.logs.len() > MAX_LOGS {
            s.logs.pop_front();
        }
    }

    pub fn set_error(shared: &Arc<Mutex<Self>>, msg: String) {
        let mut s = k!(shared);
        s.error = Some(msg);
        s.running = false;
    }
}

pub struct Worker {
    pub shared: Arc<Mutex<SharedState>>,
    pub state: Arc<TaskState>,
    pub exit: Arc<AtomicBool>,
    pub reset: Arc<AtomicBool>,
    pub thread: JoinHandle<()>,
}

struct Resources {
    vision: Arc<Vision>,
    pad: Arc<Mutex<Gamepad>>,
    engine: Arc<Engine>,
    log: Arc<dyn Fn(&str) + Send + Sync>,
}

fn init_resources(
    game: GameType,
    shared: &Arc<Mutex<SharedState>>,
    state: &Arc<TaskState>,
) -> Result<Resources> {
    let game_name = game.name();
    let title = match game {
        #[cfg(feature = "dna")]
        GameType::Dna => crate::dna::WINDOW_TITLE,
        #[cfg(feature = "nte")]
        GameType::Nte => crate::nte::WINDOW_TITLE,
    };
    let window = windows_capture::window::Window::from_contains_name(title)
        .map_err(|e| anyhow::anyhow!("找不到游戏窗口：{e}"))?;
    activate_window(&window);
    let vision = Arc::new(Vision::start(window)?);
    SharedState::push_log(shared, format!("正在捕获窗口：{title}"));

    let cached = k!(RES_CACHE).clone();
    let (pad, assets) = match cached {
        Some((g, res)) if g == game => {
            k!(res.pad).reset();
            (res.pad, res.assets)
        }
        _ => {
            let pad = Arc::new(Mutex::new(Gamepad::new()
                .context("无法连接虚拟手柄：请以管理员身份运行，或确认已安装 ViGEmBus 驱动")?));
            let assets = Arc::new(load_assets(game_name, vision.get_dimension().1)?);
            *k!(RES_CACHE) = Some((
                game.clone(),
                GameResources { pad: pad.clone(), assets: assets.clone() },
            ));
            SharedState::push_log(shared, "虚拟手柄已就绪".into());
            (pad, assets)
        }
    };

    #[cfg(feature = "ocr")]
    let ocr = crate::ocr::Ocr::global().context("无法初始化 OCR 引擎")?;
    let mut engine = new_engine(
        &vision, &pad, &assets, state,
        #[cfg(feature = "ocr")]
        &ocr
    );
    let sh = shared.clone();
    let log: Arc<dyn Fn(&str) + Send + Sync> =
        Arc::new(move |msg| SharedState::push_log(&sh, msg.to_string()));
    let l = log.clone();
    engine.on_print(move |msg: &str| l(msg));

    match game {
        #[cfg(feature = "dna")]
        GameType::Dna => {
            crate::dna::setup_engine(&mut engine, &pad, state);
        }
        #[cfg(feature = "nte")]
        GameType::Nte => {
            crate::nte::setup_engine(&mut engine, &pad, state, &vision, &ocr);
        }
    }
    let engine = Arc::new(engine);

    Ok(Resources { vision, pad, engine, log })
}

pub fn spawn(game: GameType, task: String, args: Args) -> Result<Worker> {
    let shared = SharedState::new();
    let exit = Arc::new(AtomicBool::new(false));
    let reset = Arc::new(AtomicBool::new(false));
    let state = Arc::new(TaskState::default());
    let s = shared.clone();
    let ex = exit.clone();
    let re = reset.clone();
    let st = state.clone();
    let thread = Builder::new()
        .name("task".into())
        .spawn(move || {
            if let Err(e) = run_inner(game, task, args, &s, &st, &ex, &re) {
                SharedState::set_error(&s, format!("{e:#}"));
            }
        })?;
    k!(shared).running = true;
    Ok(Worker { shared, state, exit, reset, thread })
}

pub fn spawn_custom(game: GameType, script: String, args: Args) -> Result<Worker> {
    let shared = SharedState::new();
    let exit = Arc::new(AtomicBool::new(false));
    let reset = Arc::new(AtomicBool::new(false));
    let state = Arc::new(TaskState::default());
    let s = shared.clone();
    let ex = exit.clone();
    let re = reset.clone();
    let st = state.clone();
    let thread = Builder::new()
        .name("task".into())
        .spawn(move || {
            if let Err(e) = run_custom_inner(game, script, args, &s, &st, &ex, &re) {
                SharedState::set_error(&s, format!("{e:#}"));
            }
        })?;
    k!(shared).running = true;
    Ok(Worker { shared, state, exit, reset, thread })
}

fn run_inner(
    game: GameType,
    task: String,
    args: Args,
    shared: &Arc<Mutex<SharedState>>,
    state: &Arc<TaskState>,
    exit: &Arc<AtomicBool>,
    reset: &Arc<AtomicBool>,
) -> Result<()> {
    let resources = init_resources(game.clone(), shared, state)?;
    let game_name = game.name();
    let script = std::fs::read_to_string(crate::script_path(game_name, &task))
        .map_err(|e| anyhow::anyhow!("找不到任务脚本：{e}"))?;
    let meta = ScriptMeta::parse(&script);
    let ast = Arc::new(resources.engine.compile(&script)?);
    let timeout = if args.timeout > 0 {
        Duration::from_secs(args.timeout)
    } else {
        Duration::from_secs(meta.timeout)
    };
    let scope = Arc::new(init_scope(&args));

    k!(shared).running = true;
    let result = match game {
        #[cfg(feature = "dna")]
        GameType::Dna => crate::dna::run(
            resources.engine.clone(),
            ast,
            scope,
            state,
            exit.clone(),
            reset.clone(),
            timeout,
            resources.log.clone(),
            resources.vision.clone(),
            resources.pad.clone(),
            meta.r#loop,
        ),
        #[cfg(feature = "nte")]
        GameType::Nte => crate::nte::run(
            resources.engine.clone(),
            ast,
            scope,
            state,
            exit.clone(),
            reset.clone(),
            timeout,
            resources.log.clone(),
        ),
    };
    if let Err(e) = result {
        SharedState::push_log(shared, format!("任务出错：{e:#}"));
    }
    crate::audio::disable_all();
    resources.vision.stop();
    k!(shared).running = false;
    SharedState::push_log(shared, "任务已停止".into());
    Ok(())
}

fn run_custom_inner(
    game: GameType,
    script: String,
    args: Args,
    shared: &Arc<Mutex<SharedState>>,
    state: &Arc<TaskState>,
    exit: &Arc<AtomicBool>,
    reset: &Arc<AtomicBool>,
) -> Result<()> {
    let resources = init_resources(game.clone(), shared, state)?;
    let ast = Arc::new(resources.engine.compile(&script)?);

    k!(shared).running = true;
    STOP.store(false, Ordering::SeqCst);
    let timeout = if args.timeout > 0 {
        Duration::from_secs(args.timeout)
    } else {
        Duration::from_secs(ScriptMeta::DEFAULT_TIMEOUT)
    };
    let scope = Arc::new(init_scope(&args));
    let handle = spawn_script(resources.engine.clone(), ast, scope, resources.log.clone());
    let start = Instant::now();
    loop {
        if check(&handle, exit, reset, timeout, start).is_some() {
            break;
        }
        sleep(0.1);
    }
    let _ = handle.join();
    STOP.store(false, Ordering::SeqCst);
    crate::audio::disable_all();
    resources.vision.stop();
    k!(shared).running = false;
    SharedState::push_log(shared, "脚本执行结束".into());
    Ok(())
}

pub fn spawn_script(
    engine: Arc<Engine>,
    ast: Arc<AST>,
    scope: Arc<Scope<'static>>,
    log: Arc<dyn Fn(&str) + Send + Sync>,
) -> JoinHandle<()> {
    STOP.store(false, Ordering::SeqCst);
    Builder::new()
        .spawn(move || {
            let mut s = (*scope).clone();
            match engine.run_ast_with_scope(&mut s, &ast) {
                Ok(()) => {},
                Err(e) => {
                    let stopped = matches!(
                        e.as_ref(),
                        rhai::EvalAltResult::ErrorTerminated(t, _)
                            if t.clone().try_cast::<StopReason>() == Some(StopReason::Exit)
                    );
                    if !stopped {
                        log(&format!("脚本出错：{e}"));
                        let _ = std::fs::write("error.log", format!("{e}\n"));
                    }
                }
            }
        })
        .expect("spawn sub thread")
}
