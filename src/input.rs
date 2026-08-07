use anyhow::Result;
use vigem_client::{Client, TargetId, Xbox360Wired, XButtons, XGamepad};

use crate::script_engine::sleep;

pub const UP: XButtons = XButtons!(UP);
pub const DOWN: XButtons = XButtons!(DOWN);
pub const LEFT: XButtons = XButtons!(LEFT);
pub const RIGHT: XButtons = XButtons!(RIGHT);
pub const START: XButtons = XButtons!(START);
pub const BACK: XButtons = XButtons!(BACK);
pub const LS: XButtons = XButtons!(LTHUMB);
pub const RS: XButtons = XButtons!(RTHUMB);
pub const LB: XButtons = XButtons!(LB);
pub const RB: XButtons = XButtons!(RB);
pub const GUIDE: XButtons = XButtons!(GUIDE);
pub const A: XButtons = XButtons!(A);
pub const B: XButtons = XButtons!(B);
pub const X: XButtons = XButtons!(X);
pub const Y: XButtons = XButtons!(Y);
pub const LT: XButtons = XButtons { raw: 0x0888 };
pub const RT: XButtons = XButtons { raw: 0x1888 };

pub struct Gamepad {
    target: Xbox360Wired<Client>,
    state: XGamepad,
}

impl Gamepad {
    pub fn new() -> Result<Self> {
        let client = Client::connect()?;
        let mut target = Xbox360Wired::new(client.try_clone()?, TargetId::XBOX360_WIRED);
        target.plugin()?;
        target.wait_ready()?;
        Ok(Self { target, state: XGamepad::default() })
    }

    pub fn press(&mut self, button: XButtons, hold: f64) {
        self.press_raw(button);
        self.update();
        sleep(hold);
    }

    pub fn release(&mut self, button: XButtons, post: f64) {
        self.release_raw(button);
        self.update();
        sleep(post);
    }

    pub fn press_raw(&mut self, button: XButtons) {
        match button {
            LT => self.state.left_trigger = 255,
            RT => self.state.right_trigger = 255,
            _ => self.state.buttons = XButtons(self.state.buttons.raw | button.raw),
        }
    }

    pub fn release_raw(&mut self, button: XButtons) {
        match button {
            LT => self.state.left_trigger = 0,
            RT => self.state.right_trigger = 0,
            _ => self.state.buttons = XButtons(self.state.buttons.raw & !button.raw),
        }
    }

    pub fn click(&mut self, button: XButtons, hold: f64, post: f64) {
        self.press(button, hold);
        self.release(button, post);
    }

    pub fn lstick(&mut self, x: i16, y: i16, duration: f64) {
        self.state.thumb_lx = x;
        self.state.thumb_ly = y;
        self.update();
        sleep(duration);
    }

    pub fn rstick(&mut self, x: i16, y: i16, duration: f64) {
        self.state.thumb_rx = x;
        self.state.thumb_ry = y;
        self.update();
        sleep(duration);
    }

    pub fn reset(&mut self) {
        self.state = XGamepad::default();
        self.update();
    }

    pub fn update(&mut self) {
        let _ = self.target.update(&self.state);
    }
}
