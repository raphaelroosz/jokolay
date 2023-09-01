use cap_std::fs::Dir;
use egui_backend::{egui, BackendConfig, GfxBackend, UserApp, WindowBackend};
use egui_window_glfw_passthrough::{GlfwBackend, GlfwConfig};
mod init;
mod trace;
use init::get_jokolay_dir;
use jmf::manager::MarkerManager;
use joko_render::JokoRenderer;
use jokolink::{MumbleChanges, MumbleManager};
use miette::Result;
use std::path::PathBuf;
use trace::{install_tracing, show_notifications};
use tracing::{error, info};

pub struct Jokolay {
    pub show_tracing_window: bool,
    pub fps: u32,
    pub frame_count: u64,
    pub frame_reset_seconds_timestamp: u64,
    pub jdir: Dir,
    pub jpath: PathBuf,
    pub mumble_manager: Result<MumbleManager>,
    pub marker_manager: Result<MarkerManager>,
    pub joko_renderer: JokoRenderer,
    pub egui_context: egui::Context,
    pub window_backend: GlfwBackend,
}

impl Jokolay {
    fn new(
        window_backend: GlfwBackend,
        joko_renderer: JokoRenderer,
        jpath: PathBuf,
        jdir: Dir,
    ) -> Result<Self> {
        let mumble = MumbleManager::new("MumbleLink", None);
        let marker_manager = MarkerManager::new(&jdir);
        let egui_context = egui::Context::default();
        // use roboto for ui fonts
        {
            let mut fonts = egui::FontDefinitions::default();
            fonts.font_data.insert(
                "roboto".to_owned(),
                egui::FontData::from_static(include_bytes!("../roboto.ttf")),
            );
            fonts
                .families
                .get_mut(&egui::FontFamily::Proportional)
                .unwrap()
                .insert(0, "roboto".to_owned());
            egui_context.set_fonts(fonts);
        }

        Ok(Self {
            mumble_manager: mumble,
            marker_manager,

            joko_renderer,
            frame_count: 0,
            frame_reset_seconds_timestamp: 0,
            fps: 0,
            window_backend,
            jdir,
            jpath,
            egui_context,
            show_tracing_window: false,
        })
    }
}
impl UserApp for Jokolay {
    fn gui_run(&mut self) {
        // most of the fn contents are in Self::run fn instead.
        // As we need some custom input filtering (to match scale of gw2 UI or custom scaling)
    }

    type UserGfxBackend = JokoRenderer;

    type UserWindowBackend = GlfwBackend;

    fn get_all(
        &mut self,
    ) -> (
        &mut Self::UserWindowBackend,
        &mut Self::UserGfxBackend,
        &egui::Context,
    ) {
        (
            &mut self.window_backend,
            &mut self.joko_renderer,
            &self.egui_context,
        )
    }

    fn run(
        &mut self,
        logical_size: [f32; 2],
    ) -> Option<(egui::PlatformOutput, std::time::Duration)> {
        let Self {
            fps,
            frame_count,
            frame_reset_seconds_timestamp,
            mumble_manager,
            marker_manager,
            joko_renderer,
            egui_context,
            window_backend,
            jdir: _,
            jpath: _,
            show_tracing_window,
        } = self;
        let egui_context = egui_context.clone();
        // don't bother doing anything if there's no window
        if let Some(full_output) = if window_backend.get_window().is_some() {
            let input = window_backend.take_raw_input();
            joko_renderer.prepare_frame(window_backend);
            egui_context.begin_frame(input);
            let latest_time = window_backend.glfw.get_time();
            *frame_count += 1;
            if latest_time - *frame_reset_seconds_timestamp as f64 > 1.0 {
                *fps = *frame_count as u32;
                *frame_count = 0;
                *frame_reset_seconds_timestamp = latest_time as u64;
            }
            let link = if let Ok(mm) = mumble_manager.as_mut() {
                match mm.tick(&egui_context) {
                    Ok(ml) => ml,
                    Err(e) => {
                        error!("mumble manager tick error: {e:#?}");
                        None
                    }
                }
            } else {
                None
            };
            egui_context.request_repaint();
            let cursor_position = egui_context.pointer_latest_pos();
            egui::Window::new("Tracing Window")
                .open(show_tracing_window)
                .show(&egui_context, |ui| {
                    trace::show_tracing_events(ui);
                });
            egui::Window::new("egui window")
                .default_width(300.0)
                .show(&egui_context, |ui| {
                    ui.label(&format!("cursor position: {cursor_position:?}"));
                    ui.label(&format!("fps: {}", *fps));
                    let mut is_passthrough = window_backend.window.is_mouse_passthrough();
                    ui.checkbox(&mut is_passthrough, "is window passthrough?");
                    if let Err(e) = mumble_manager {
                        ui.label(format!("mumble manager error: {e:#?}"));
                    }
                });
            if let Ok(marker_manager) = marker_manager {
                marker_manager.tick(&egui_context, latest_time, joko_renderer, &link);
            }
            // check if we need to change window position or size.
            if let Some(link) = link.as_ref() {
                joko_renderer.update_from_mumble_link(link);
                if link.changes.contains(MumbleChanges::WindowPosition)
                    || link.changes.contains(MumbleChanges::WindowSize)
                {
                    info!(
                        "resizing/repositioning to match gw2 window dimensions: {:?} {:?}",
                        link.window_pos, link.window_size
                    );
                    // to account for the invisible border shadows thingy. IDK if these pixel values are the same across all dpi/monitors
                    window_backend
                        .window
                        .set_pos(link.window_pos.x + 5, link.window_pos.y + 56);
                    window_backend
                        .window
                        .set_size(link.window_size.x - 10, link.window_size.y - 61);
                }
            }
            show_notifications(&egui_context);
            // if it doesn't require either keyboard or pointer, set passthrough to true
            window_backend.window.set_mouse_passthrough(
                !(egui_context.wants_keyboard_input() || egui_context.wants_pointer_input()),
            );
            Some(egui_context.end_frame())
        } else {
            None
        } {
            let egui::FullOutput {
                platform_output,
                repaint_after,
                textures_delta,
                shapes,
            } = full_output;
            let (wb, gb, egui_context) = self.get_all();
            let egui_context = egui_context.clone();

            gb.render_egui(
                egui_context.tessellate(shapes),
                textures_delta,
                logical_size,
            );
            gb.present(wb);
            return Some((platform_output, repaint_after));
        }
        None
    }
}

pub fn start_jokolay() {
    let (jokolay_dir_path, jdir) = get_jokolay_dir().unwrap();
    let _log_file_flush_guard = install_tracing(&jdir).unwrap();
    info!("using {jokolay_dir_path:?} as the jokolay data directory");
    rayon::ThreadPoolBuilder::default()
        .panic_handler(|p| {
            error!("rayon thread paniced. {p:#?}");
        })
        .build_global()
        .expect("failed to set panic handler for rayon");
    let mut glfw_backend = GlfwBackend::new(
        GlfwConfig {
            glfw_callback: Box::new(|glfw_context| {
                glfw_context.window_hint(
                    egui_window_glfw_passthrough::glfw::WindowHint::SRgbCapable(true),
                );
                glfw_context.window_hint(egui_window_glfw_passthrough::glfw::WindowHint::Floating(
                    true,
                ));
            }),
            ..Default::default()
        },
        BackendConfig {
            transparent: Some(true),
            is_opengl: false,
            ..Default::default()
        },
    );

    let joko_renderer = JokoRenderer::new(&mut glfw_backend, Default::default());
    // remove decorations
    glfw_backend.window.set_decorated(false);
    let jokolay = Jokolay::new(glfw_backend, joko_renderer, jokolay_dir_path, jdir);
    <Jokolay as UserApp>::UserWindowBackend::run_event_loop(
        jokolay.expect("failed to create jokolay app"),
    );
}