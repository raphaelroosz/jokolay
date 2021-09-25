use crate::core::{JokoConfig, JokoCore};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

use egui::CtxRef;
use glm::vec2;
use log::LevelFilter;

use rfd::{MessageButtons, MessageDialog, MessageLevel};
use tactical::localtypes::manager::MarkerManager;
use vfs::VfsPath;

pub mod core;
pub mod gui;
pub mod tactical;

/*

#[cfg(target_os = "windows")]
pub fn get_win_pos_dim(link_ptr: *const CMumbleLink) -> anyhow::Result<WindowDimensions> {
    unsafe {
        if !CMumbleLink::is_valid(link_ptr) {
            anyhow::bail!("the MumbleLink is not init yet. so, getting window position is not valid operation");
        }
        let context = (*link_ptr).context.as_ptr() as *const CMumbleContext;
        let mut pid: isize = (*context).process_id as isize;

        let result: BOOL = EnumWindows(Some(get_handle_by_pid), &mut pid as *mut isize as LPARAM);
        if result != 0 {
            anyhow::bail!("couldn't find gw2 window. error code: {}", GetLastError());
        }

        let mut rect: RECT = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        let status = GetWindowRect(pid as isize as HWND, &mut rect as LPRECT);
        if status == 0 {
            anyhow::bail!("could not get gw2 window size");
        }
        Ok(WindowDimensions {
            x: rect.left,
            y: rect.top,
            width: (rect.right - rect.left),
            height: (rect.bottom - rect.top),
        })
    }
} */

pub struct JokolayApp {
    pub core: JokoCore,
    pub ctx: CtxRef,
    pub mm: MarkerManager,
    pub config: JokoConfig,
    state: EState,
}

#[derive(Debug, Clone, Default)]
pub struct EState {
    pub show_mumble_window: bool,
    pub show_marker_manager: bool,
}
impl JokolayApp {
    pub fn new(mut config: JokoConfig, assets_path: PathBuf) -> Self {
        let (mut core, ctx) = JokoCore::new(&mut config, assets_path);

        let mm = MarkerManager::new(&mut core.fm);
        JokolayApp {
            state: Default::default(),
            mm,
            ctx,
            core,
            config,
        }
    }

    pub fn run(mut self) -> anyhow::Result<()> {
        let mut auto_save_timer = Instant::now();
        //fps counter
        let mut fps = 0;
        let mut timer = Instant::now();
        let mut average_egui = Duration::default();
        let mut average_draw_call = Duration::default();
        self.core
            .im
            .glfw
            .set_swap_interval(glfw::SwapInterval::Sync(0));

        while !self.core.ow.should_close() {
            // starting loop timer
            let et = Instant::now();
            if timer.elapsed() > Duration::from_secs(1) {
                dbg!(fps, average_egui, average_draw_call);
                fps = 0;
                timer = Instant::now();
            }
            fps += 1;

            let t = self.tick();
            // ending loop timer
            average_egui = (average_egui + et.elapsed()) / 2;
            // start draw call timer
            let dt = Instant::now();
            self.core.rr.draw_egui(
                t,
                vec2(
                    self.core.ow.config.framebuffer_width as f32,
                    self.core.ow.config.framebuffer_height as f32,
                ),
                &self.core.fm,
                self.ctx.clone(),
            );
            average_draw_call = (average_draw_call + dt.elapsed()) / 2;

            if auto_save_timer.elapsed() > Duration::from_secs(5) {
                auto_save_timer = Instant::now();
                Self::save_config(&self.config, &self.core.fm.config_file_path);
                Self::save_egui_memory(self.ctx.clone(), &self.core.fm.egui_cache_path);
            }
            self.core.ow.swap_buffers();
        }
        Ok(())
    }
}

impl JokolayApp {
    pub fn save_egui_memory(ctx: CtxRef, path: &VfsPath) {
        let egui_cache = path.create_file().map_err(|e| {
            log::error!(
                "could not write config to config file due to error {:?}",
                &e
            );
            e
        });
        if egui_cache.is_err() {
            return;
        }
        let writer = std::io::BufWriter::new(egui_cache.unwrap());
        let memory = ctx.memory().clone();
        serde_json::to_writer_pretty(writer, &memory)
            .map_err(|e| {
                log::error!(
                    "could not write config to config file due to error {:?}",
                    &e
                );
                e
            })
            .unwrap_or_default();
    }

    pub fn save_config(config: &JokoConfig, path: &VfsPath) {
        let config_file = path.create_file().map_err(|e| {
            log::error!(
                "could not write config to config file due to error {:?}",
                &e
            );
            e
        });
        if config_file.is_err() {
            return;
        }
        let writer = std::io::BufWriter::new(config_file.unwrap());
        serde_json::to_writer_pretty(writer, config)
            .map_err(|e| {
                log::error!(
                    "could not write config to config file due to error {:?}",
                    &e
                );
                e
            })
            .unwrap_or_default();
    }
}
/// initializes global logging backend that is used by log macros
/// Takes in a filter for stdout/stderr, a filter for logfile and finally the path to logfile
pub fn log_init(
    term_filter: LevelFilter,
    file_filter: LevelFilter,
    file_path: PathBuf,
) -> anyhow::Result<()> {
    use simplelog::*;
    use std::fs::File;
    let config = ConfigBuilder::new()
        .set_location_level(LevelFilter::Error)
        .build();

    CombinedLogger::init(vec![
        TermLogger::new(term_filter, config, TerminalMode::Mixed, ColorChoice::Auto),
        WriteLogger::new(file_filter, Config::default(), File::create(file_path)?),
    ])?;
    Ok(())
}

#[macro_export]
macro_rules! gl_error {
    ($gl:expr) => {
        let e = $gl.get_error();
        if e != glow::NO_ERROR {
            log::error!("glerror {} at {} {} {}", e, file!(), line!(), column!());
        }
    };
}

pub fn show_msg_box(title: &str, msg: &str, buttons: MessageButtons, lvl: MessageLevel) -> bool {
    MessageDialog::new()
        .set_level(lvl)
        .set_title(title)
        .set_description(msg)
        .set_buttons(buttons)
        .show()
}
