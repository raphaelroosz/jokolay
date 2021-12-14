use std::{rc::Rc, sync::mpsc::Receiver};

use glfw::{Glfw, WindowEvent};

use crate::core::{input::glfw_input::GlfwInput, window::OverlayWindow};

pub mod glfw_input;
pub mod rdev_input;

#[derive(Debug)]
pub struct InputManager {
    pub glfw_input: GlfwInput,
}

impl InputManager {
    pub fn new(events: Receiver<(f64, WindowEvent)>, glfw: Glfw) -> Self {
        Self {
            glfw_input: GlfwInput::new(events, glfw),
        }
    }

    pub fn tick(&mut self, gl: Rc<glow::Context>, ow: &mut OverlayWindow) -> FrameEvents {
        self.glfw_input.get_events(gl, ow)
    }
}

#[derive(Debug, Clone)]
pub struct FrameEvents {
    pub all_events: Vec<WindowEvent>,
    pub clipboard_text: Option<String>,
    pub cursor_position: egui::Pos2,
    pub time: f64,
    pub average_frame_rate: u16,
}
