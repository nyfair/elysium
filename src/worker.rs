use anyhow::{Context, Result};
use rhai::{AST, Engine, Scope};
use std::collections::VecDeque;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Instant;

use crate::{Args, GameType, k};
use crate::dna::{self, TaskState};
use crate::input::Gamepad;
use crate::script_engine;
use crate::vision::{self, Vision};

const MAX_LOGS: usize = 200;

pub struct ScriptMeta {
    pub author: String,
    pub desc: String,
    pub r#loop: bool,
    pub timeout: u64,
}

impl Default for ScriptMeta {
    fn default() -> Self {
        Self {
            author: String::new(),
            desc: String::new(),
            r#loop: true,
            timeout: 0,
        }
    }
}

pub fn parse_meta(script: &str) -> ScriptMeta {
    let mut lines = Vec::new();
    let mut in_block = false;
    for line in script.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("//") {
            let c = rest.trim();
            if c.starts_with('{') {
                in_block = true;
            }
            if in_block {
                lines.push(c);
            }
            if in_block && c.ends_with('}') {
                break;
            }
        } else if in_block {
            break;
        }
    }
    if lines.is_empty() {
        return ScriptMeta::default();
    }
    let v: serde_json::Value =
        serde_json::from_str(&lines.join("\n")).unwrap_or(serde_json::Value::Null);
    if v.is_null() {
        return ScriptMeta::default();
    }
    ScriptMeta {
        author: v.get("author").and_then(|x| x.as_str()).unwrap_or("").to_string(),
        desc: v.get("desc").and_then(|x| x.as_str()).unwrap_or("").to_string(),
        r#loop: v.get("loop").and_then(|x| x.as_bool()).unwrap_or(true),
        timeout: v.get("timeout").and_then(|x| x.as_u64()).unwrap_or(0),
    }
}

pub struct SharedState {
    pub running: bool,
    pub cycle: u64,
    pub logs: VecDeque<String>,
    pub started_at: Instant,
    pub error: Option<String>,
}

impl SharedState {
    pub fn new() -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(Self {
            running: false,
            cycle: 0,
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
    check_size: bool,
) -> Result<Resources> {
    let game_name = game.name();
    let title = match game {
        GameType::Dna => dna::WINDOW_TITLE,
        GameType::Nte => crate::nte::WINDOW_TITLE,
    };
    let window = windows_capture::window::Window::from_contains_name(title)
        .map_err(|e| anyhow::anyhow!("找不到游戏窗口：{e}"))?;
    vision::activate_window(&window);
    let vision = Arc::new(Vision::start(window)?);
    if check_size {
        let (w, h) = vision.dimensions();
        if w != vision::W || h != vision::H {
            anyhow::bail!("游戏窗口分辨率不是 {}x{}（当前 {w}x{h}），请调整后再运行", vision::W, vision::H);
        }
    }
    SharedState::push_log(shared, format!("正在捕获窗口：{title}"));

    let pad = Arc::new(Mutex::new(Gamepad::new()
        .context("无法连接虚拟手柄：请以管理员身份运行，或确认已安装 ViGEmBus 驱动")?));
    SharedState::push_log(shared, "虚拟手柄已就绪".into());

    let assets = Arc::new(vision::load_assets(game_name, 720)?);

    let mut engine = script_engine::new_engine(&vision, &pad, &assets);
    let sh = shared.clone();
    let log: Arc<dyn Fn(&str) + Send + Sync> =
        Arc::new(move |msg| SharedState::push_log(&sh, msg.to_string()));
    let l = log.clone();
    engine.on_print(move |msg: &str| l(msg));

    match game {
        GameType::Dna => {
            dna::setup_engine(&mut engine, &vision, &pad, state);
        }
        GameType::Nte => {
            crate::nte::setup_engine(&mut engine, state);
        }
    }
    let engine = Arc::new(engine);

    Ok(Resources { vision, pad, engine, log })
}

pub fn spawn(game: GameType, task: String, args: Args) -> Result<Worker> {
    let shared = SharedState::new();
    let state = Arc::new(TaskState {
        stop: Arc::new(Mutex::new(false)),
        pause: Arc::new(Mutex::new(false)),
        cur_turn: Arc::new(Mutex::new(1)),
    });
    let exit = Arc::new(AtomicBool::new(false));
    let reset = Arc::new(AtomicBool::new(false));
    let s = shared.clone();
    let st = state.clone();
    let ex = exit.clone();
    let re = reset.clone();
    let thread = thread::Builder::new()
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
    let state = Arc::new(TaskState {
        stop: Arc::new(Mutex::new(false)),
        pause: Arc::new(Mutex::new(false)),
        cur_turn: Arc::new(Mutex::new(1)),
    });
    let exit = Arc::new(AtomicBool::new(false));
    let reset = Arc::new(AtomicBool::new(false));
    let s = shared.clone();
    let st = state.clone();
    let ex = exit.clone();
    let re = reset.clone();
    let thread = thread::Builder::new()
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
    let resources = init_resources(game.clone(), shared, state, true)?;
    let game_name = game.name();
    let script = std::fs::read_to_string(&format!("{game_name}/scripts/{task}.rhai"))
        .map_err(|e| anyhow::anyhow!("找不到任务脚本：{e}"))?;
    let meta = parse_meta(&script);
    let ast = Arc::new(resources.engine.compile(&script)?);
    let has_script_ended = ast
        .iter_functions()
        .any(|f| f.name == "task_ended" && f.params.is_empty());

    k!(shared).running = true;
    let result = dna::run(
        resources.engine.clone(),
        resources.vision.clone(),
        resources.pad.clone(),
        ast,
        args,
        state.clone(),
        exit.clone(),
        reset.clone(),
        resources.log.clone(),
        has_script_ended,
        meta.r#loop,
        meta.timeout,
    );
    if let Err(e) = result {
        SharedState::push_log(shared, format!("任务出错：{e:#}"));
    }
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
    _exit: &Arc<AtomicBool>,
    _reset: &Arc<AtomicBool>,
) -> Result<()> {
    let resources = init_resources(game.clone(), shared, state, false)?;
    let ast = Arc::new(resources.engine.compile(&script)?);

    k!(shared).running = true;
    let handle = spawn_script(resources.engine.clone(), ast, args, resources.log.clone());
    let _ = handle.join();
    resources.vision.stop();
    k!(shared).running = false;
    SharedState::push_log(shared, "脚本执行结束".into());
    Ok(())
}

pub fn spawn_script(
    engine: Arc<Engine>,
    ast: Arc<AST>,
    args: Args,
    log: Arc<dyn Fn(&str) + Send + Sync>,
) -> JoinHandle<()> {
    thread::Builder::new()
        .name("task-sub".into())
        .spawn(move || {
            let mut scope = Scope::new();
            scope.push_constant("BOOST", args.boost as i64);
            scope.push_constant("STRATEGY", args.strategy.clone());
            scope.push_constant("COMBO", args.combo.clone());
            scope.push_constant("TURN", args.turn as i64);
            scope.push_constant("TIMEOUT", args.timeout);
            match engine.run_ast_with_scope(&mut scope, &ast) {
                Ok(()) => log("脚本执行完毕"),
                Err(e) => {
                    log(&format!("脚本出错：{e}"));
                    let _ = std::fs::write("error.log", format!("{e}\n"));
                }
            }
        })
        .expect("spawn sub thread")
}
