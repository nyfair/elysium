use rhai::Engine;

use crate::dna::TaskState;
use crate::k;

pub const WINDOW_TITLE: &str = "异环  ";

pub fn setup_engine(engine: &mut Engine, state: &TaskState) {
    let s = state.stop.clone();
    engine.register_fn("set_stop", move |val: bool| *k!(s) = val);
    let s = state.stop.clone();
    engine.register_fn("get_stop", move || -> bool { *k!(s) });
}
