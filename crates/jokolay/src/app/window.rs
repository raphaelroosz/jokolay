use std::sync::{Arc, RwLock};

use egui_window_glfw_passthrough::GlfwBackend;
use joko_component_models::{
    default_component_result, from_broadcast, Component, ComponentChannels, ComponentResult,
};
use joko_link_models::{MumbleChanges, MumbleLink};

pub(crate) const MINIMAL_WINDOW_WIDTH: u32 = 640;
pub(crate) const MINIMAL_WINDOW_HEIGHT: u32 = 480;
pub(crate) const MINIMAL_WINDOW_POSITION_X: i32 = 0;
pub(crate) const MINIMAL_WINDOW_POSITION_Y: i32 = 0;

struct WindowManagerChannels {
    subscription_mumblelink: tokio::sync::broadcast::Receiver<ComponentResult>,
}
pub(crate) struct WindowManager {
    glfw_backend: Arc<RwLock<GlfwBackend>>,
    window_changed: bool,
    maximal_window_width: u32,
    maximal_window_height: u32,
    last_known_link: Option<MumbleLink>,
    channels: Option<WindowManagerChannels>,
}

impl WindowManager {
    pub fn new(glfw_backend: Arc<RwLock<GlfwBackend>>) -> Self {
        //retrieve current screen resolution
        let video_mode = glfw_backend
            .write()
            .unwrap()
            .glfw
            .with_primary_monitor(|_, m| {
                if let Some(m) = m {
                    m.get_video_mode()
                } else {
                    None
                }
            });
        let maximal_window_width = video_mode.unwrap().width;
        let maximal_window_height = video_mode.unwrap().height;

        glfw_backend.write().unwrap().window.set_floating(true);
        glfw_backend.write().unwrap().window.set_decorated(false);

        Self {
            glfw_backend,
            window_changed: true,
            maximal_window_width,
            maximal_window_height,
            last_known_link: None,
            channels: None,
        }
    }
}

/// Necessary lies for GlfwBackend, which despite not moved (Arc + Mutex) shall prevent compilation
unsafe impl Send for WindowManager {}
unsafe impl Sync for WindowManager {}

impl Component for WindowManager {
    fn accept_notifications(&self) -> bool {
        true
    }
    fn bind(&mut self, mut channels: ComponentChannels) {
        let channels = WindowManagerChannels {
            subscription_mumblelink: channels.requirements.remove(&0).unwrap(),
        };

        self.channels = Some(channels);
    }
    fn flush_all_messages(&mut self) {}
    fn init(&mut self) {}
    fn requirements(&self) -> Vec<&str> {
        vec!["ui:mumble_link"] // is it ?
    }
    fn tick(&mut self, _latest_time: f64) -> ComponentResult {
        assert!(
            self.channels.is_some(),
            "channels must be initialized before interacting with component."
        );
        let channels = self.channels.as_mut().unwrap();
        if let Ok(data) = channels.subscription_mumblelink.try_recv() {
            let link: Option<MumbleLink> = from_broadcast(&data);
            match link {
                Some(link) => {
                    if link.changes.contains(MumbleChanges::WindowPosition)
                        || link.changes.contains(MumbleChanges::WindowSize)
                    {
                        self.window_changed = true;
                    }
                    self.last_known_link = Some(link);
                }
                _ => {
                    //error!("WindowManager manager tick error, MumbleLink link data, nothing found");
                }
            }
        } else {
            println!("WindowManager: No data from mumble");
        }
        if let Some(last_known_link) = &mut self.last_known_link {
            if self.window_changed {
                let client_pos = &last_known_link.client_pos.0;
                let client_size = &last_known_link.client_size.0;
                let mut glfw_backend = self.glfw_backend.write().unwrap();
                glfw_backend.window.set_pos(
                    client_pos.x.max(MINIMAL_WINDOW_POSITION_X),
                    client_pos.y.max(MINIMAL_WINDOW_POSITION_Y),
                );
                // if gw2 is in windowed fullscreen mode, then the size is full resolution of the screen/monitor.
                // But if we set that size, when you focus jokolay, the screen goes blank on win11 (some kind of fullscreen optimization maybe?)
                // so we remove a pixel from right/bottom edges. mostly indistinguishable, but makes sure that transparency works even in windowed fullscrene mode of gw2
                let client_size_x = MINIMAL_WINDOW_WIDTH
                    .max(client_size.x)
                    .min(self.maximal_window_width);
                let client_size_y = MINIMAL_WINDOW_HEIGHT
                    .max(client_size.y)
                    .min(self.maximal_window_height);
                glfw_backend
                    .window
                    .set_size((client_size_x - 1) as i32, (client_size_y - 1) as i32);
            }
            self.window_changed = false;
        }
        default_component_result()
    }
}
