use anyhow::Result;
use rhai::{AST, Engine};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use crate::script_engine::{sleep, TaskState, STOP};
use crate::worker;
use crate::Args;

pub const WINDOW_TITLE: &str = "异环  ";

pub fn setup_engine(_engine: &mut Engine, _state: &TaskState) {}

pub fn run(
    engine: Arc<Engine>,
    ast: Arc<AST>,
    args: Args,
    exit: Arc<AtomicBool>,
    reset: Arc<AtomicBool>,
    timeout: std::time::Duration,
    log: Arc<dyn Fn(&str) + Send + Sync>,
) -> Result<()> {
    STOP.store(false, Ordering::SeqCst);
    let handle = worker::spawn_script(engine, ast, args, log);
    let start = Instant::now();
    loop {
        if worker::check(&handle, &exit, &reset, timeout, start).is_some() {
            break;
        }
        sleep(0.1);
    }
    let _ = handle.join();
    STOP.store(false, Ordering::SeqCst);
    Ok(())
}
