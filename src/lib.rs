use std::{collections::BTreeMap, net::UdpSocket, sync::mpsc::Receiver, time::Duration};

use glfw::{Action, Glfw, Key, WindowEvent};
use glow::HasContext;
use gw::{category::MarkerCategory, load_markers, marker::Marker, trail::Trail};
use mlink::MumbleCache;

use crate::mlink::GetMLMode;

pub mod glc;
pub mod gw;
pub mod mlink;

pub struct JokolayApp {
    pub glfw: Glfw,
    pub gl: glow::Context,
    pub window: glfw::Window,
    pub marker_categories: BTreeMap<String, MarkerCategory>,
    pub markers: BTreeMap<u32, Vec<Marker>>,
    pub trails: BTreeMap<u32, Vec<Trail>>,
}

impl JokolayApp {
    pub fn new() -> Self {
        let (glfw, gl, window, _events) = glfw_window_init();
        let (marker_categories, markers, trails) = load_markers();

        unsafe {
            if gl.get_error() != glow::NO_ERROR {
                println!("glerror at {} {} {}", file!(), line!(), column!());
            }
        }
        JokolayApp {
            glfw,
            gl,
            window,
            marker_categories,
            markers,
            trails,
        }
    }
    pub fn run(&mut self) {
        let gl = &self.gl;
        loop {
            unsafe {
                if gl.get_error() != glow::NO_ERROR {
                    println!("glerror at {} {} {}", file!(), line!(), column!());
                }
            }
    
            glfw::Context::swap_buffers(&mut self.window);
    
            self.glfw.poll_events();
        }
        
    }
}

// pub struct EguiApp {

// }

// impl epi::App for EguiApp {
//     fn setup(
//         &mut self,
//         _ctx: &egui::CtxRef,
//         _frame: &mut epi::Frame<'_>,
//         _storage: Option<&dyn epi::Storage>,
//     ) {
//     }

//     fn warm_up_enabled(&self) -> bool {
//         false
//     }

//     fn save(&mut self, _storage: &mut dyn epi::Storage) {}

//     fn on_exit(&mut self) {}

//     fn auto_save_interval(&self) -> std::time::Duration {
//         std::time::Duration::from_secs(30)
//     }

//     fn max_size_points(&self) -> egui::Vec2 {
//         // Some browsers get slow with huge WebGL canvases, so we limit the size:
//         egui::Vec2::new(1024.0, 2048.0)
//     }

//     fn clear_color(&self) -> egui::Rgba {
//         // NOTE: a bright gray makes the shadows of the windows look weird.
//         // We use a bit of transparency so that if the user switches on the
//         // `transparent()` option they get immediate results.
//         egui::Color32::from_rgba_unmultiplied(12, 12, 12, 180).into()
//     }

//     fn update(&mut self, ctx: &egui::CtxRef, frame: &mut epi::Frame<'_>) {
//         egui::Ui::new(ctx, layer_id, id, max_rect, clip_rect)
//     }

//     fn name(&self) -> &str {
//         "egui app "
//     }
// }

pub fn process_events(
    window: &mut glfw::Window,
    events: &Receiver<(f64, glfw::WindowEvent)>,
    gl: &glow::Context,
) {
    for (_, event) in glfw::flush_messages(events) {
        match event {
            glfw::WindowEvent::FramebufferSize(width, height) => {
                // make sure the viewport matches the new window dimensions; note that width and
                // height will be significantly larger than specified on retina displays.
                unsafe {
                    gl.viewport(0, 0, width, height);
                }
                eprintln!("resizing viewport");
            }
            glfw::WindowEvent::Key(Key::Escape, _, Action::Press, _) => {
                window.set_should_close(true)
            }
            _ => {}
        }
    }
}

pub fn glfw_window_init() -> (
    Glfw,
    glow::Context,
    glfw::Window,
    std::sync::mpsc::Receiver<(f64, WindowEvent)>,
) {
    let scr_height: u32 = 600;
    let scr_width: u32 = 800;
    let mut glfw = glfw::init(glfw::FAIL_ON_ERRORS).unwrap();
    glfw.window_hint(glfw::WindowHint::ContextVersion(4, 6));
    glfw.window_hint(glfw::WindowHint::OpenGlProfile(
        glfw::OpenGlProfileHint::Core,
    ));
    glfw.window_hint(glfw::WindowHint::TransparentFramebuffer(true));
    glfw.window_hint(glfw::WindowHint::Floating(true));
    //glfw.window_hint(glfw::WindowHint::MousePassthrough(true));
    // glfw.window_hint(glfw::WindowHint::DoubleBuffer(false));

    let (mut window, events) = glfw
        .create_window(
            scr_width,
            scr_height,
            "LearnOpenGL",
            glfw::WindowMode::Windowed,
        )
        .expect("Failed to create GLFW window");

    window.set_key_polling(true);
    glfw::Context::make_current(&mut window);
    window.set_framebuffer_size_polling(true);
    let gl =
        unsafe { glow::Context::from_loader_function(|s| window.get_proc_address(s) as *const _) };
    unsafe {
        gl.enable(glow::DEPTH_TEST);
        gl.enable(glow::BLEND);
        gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
    }
    (glfw, gl, window, events)
}

pub fn create_mlink_cache(key: &str) -> MumbleCache {
    let retry_times = 50_u32;

    for _ in 0..retry_times {
        let socket = UdpSocket::bind("127.0.0.1:0").expect("failed to bind to socket");
        socket
            .connect("127.0.0.1:7187")
            .expect("failed to connect to socket");
        let mc = MumbleCache::new(key, Duration::from_millis(20), GetMLMode::UdpSync(socket));
        if mc.is_ok() {
            return mc.unwrap();
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    panic!("couldn't get mumblelink after 50 retries");
}
