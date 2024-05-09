use std::{
    sync::{Arc, RwLock},
    thread,
};

use cap_std::fs_utf8::Dir;
use egui_window_glfw_passthrough::{GlfwBackend, GlfwConfig};
use joko_link_ui_manager::MumbleUIManager;
mod init;
mod menu;
mod ui_parameters;
mod window;

use crate::manager::{theme::ThemeManager, trace::JokolayTracingLayer};
use init::{get_jokolay_dir, get_jokolay_path};
use joko_component_manager::{ComponentExecutor, ComponentManager};
use joko_package_manager::{PackageDataManager, PackageUIManager};

use joko_link_manager::MumbleManager;
use joko_package_manager::jokolay_to_editable_path;
use joko_render_manager::renderer::JokoRenderer;
use miette::{Context, IntoDiagnostic, Result};
use tracing::{error, info_span};

use self::{menu::MenuPanelManager, window::WindowManager};

struct JokolayGui {
    menu_panel: Arc<RwLock<MenuPanelManager>>,
    egui_context: egui::Context,
    glfw_backend: Arc<RwLock<GlfwBackend>>,
}
#[allow(unused)]
pub struct Jokolay {
    gui: JokolayGui,
    app: ComponentManager,
}

impl Jokolay {
    pub fn new(root_dir: Arc<Dir>, root_path: std::path::PathBuf) -> Result<Self> {
        /*
            We have two mumble_managers, one for UI, one for backend, each keeping its own copy
            this avoid transmition between threads to read same data from system
            It happens anyway when the UI start the edit mode of the mumble link.
        */

        let mut component_manager = ComponentManager::new();

        let _ = component_manager.register(
            "ui:mumble_link",
            Arc::new(RwLock::new(
                MumbleManager::new("MumbleLink", true)
                    .wrap_err("failed to create mumble manager")?,
            )),
        );
        let _ = component_manager.register(
            "back:mumble_link",
            Arc::new(RwLock::new(
                MumbleManager::new("MumbleLink", false)
                    .wrap_err("failed to create mumble manager")?,
            )),
        );
        let egui_context = egui::Context::default();
        let mumble_ui = Arc::new(RwLock::new(MumbleUIManager::new(egui_context.clone())));

        component_manager
            .register("ui:mumble_ui", mumble_ui.clone())
            .unwrap();

        /*
        components can be migrated to plugins
        root_path/
            ui.toml
            components/
                mumble_link/
                    ...
                theme_manager/
                    ...
                package_ui/
                    ...
                package_data/
                    ...
                plugins/
                    plugin1
                    plugin2
                    ...
        */

        let _ = component_manager.register(
            "back:jokolay_package_manager",
            Arc::new(RwLock::new(PackageDataManager::new(
                &root_path, //TODO: when given to a plugin, root MUST be unique to the plugin and cannot be global to jokolay
            )?)),
        );

        let theme_manager = Arc::new(RwLock::new(
            ThemeManager::new(Arc::clone(&root_dir), egui_context.clone())
                .wrap_err("failed to create theme manager")?,
        ));

        #[allow(clippy::arc_with_non_send_sync)]
        let glfw_backend = Arc::new(RwLock::new(GlfwBackend::new(GlfwConfig {
            glfw_callback: Box::new(|glfw_context| {
                glfw_context.window_hint(
                    egui_window_glfw_passthrough::glfw::WindowHint::SRgbCapable(true),
                );
                glfw_context.window_hint(egui_window_glfw_passthrough::glfw::WindowHint::Floating(
                    true,
                ));
                glfw_context.window_hint(
                    egui_window_glfw_passthrough::glfw::WindowHint::ContextVersion(4, 6),
                );
            }),
            opengl_window: Some(true),
            transparent_window: Some(true),
            window_title: "Jokolay".to_string(),
            ..Default::default()
        })));
        let _ = component_manager.register(
            "ui:window_manager",
            Arc::new(RwLock::new(WindowManager::new(Arc::clone(&glfw_backend)))),
        );

        let package_manager_ui = Arc::new(RwLock::new(PackageUIManager::new(
            egui_context.clone(),
            JokoRenderer::get_z_near(),
        )));
        let _ =
            component_manager.register("ui:jokolay_package_manager", package_manager_ui.clone());

        let renderer_ui = Arc::new(RwLock::new(JokoRenderer::new(
            Arc::clone(&glfw_backend),
            egui_context.clone(),
        )));
        let _ = component_manager.register("ui:jokolay_renderer", renderer_ui.clone());

        let editable_path = jokolay_to_editable_path(&root_path)
            .to_str()
            .unwrap()
            .to_string();

        let configuration_ui = Arc::new(RwLock::new(ui_parameters::JokolayUIConfiguration::new(
            Arc::clone(&glfw_backend),
            egui_context.clone(),
            editable_path.clone(),
            root_path.to_str().unwrap().to_owned(),
        )));
        let _ = component_manager.register("ui:configuration", configuration_ui.clone());

        let _ = component_manager.register(
            "back:configuration",
            Arc::new(RwLock::new(ui_parameters::JokolayConfiguration::new(
                Arc::clone(&root_dir),
            ))),
        );

        let menu_panel = Arc::new(RwLock::new(MenuPanelManager::new(
            Arc::clone(&glfw_backend),
            egui_context.clone(),
        )));

        let _ = component_manager.register("ui:menu_panel", menu_panel.clone());

        match component_manager.build_routes() {
            Ok(_) => {}
            Err(e) => {
                panic!("Could not build component routes. {}", e);
            }
        }

        /*
        Configuration
        Themes
        Mumble Manager
        Package Manager
        File Manader => where ?

        close all
        exit
         */
        if let Ok(mut menu_panel) = menu_panel.write() {
            menu_panel.register(configuration_ui);
            menu_panel.register(theme_manager);
            menu_panel.register(mumble_ui);
            menu_panel.register(package_manager_ui);
            menu_panel.register(renderer_ui);
        }

        let gui = JokolayGui {
            glfw_backend,
            egui_context,
            menu_panel,
        };
        //let gui = Mutex::new(gui);
        //let gui = Arc::new(gui);
        //let gui = Box::new(gui);
        Ok(Self {
            gui,
            app: component_manager,
        })
    }

    fn start_background_loop(mut executor: ComponentExecutor) {
        let _background_thread = std::thread::spawn(move || {
            tracing::info!("Initialize the background components");
            executor.init();
            let _ = Self::background_loop(executor);
        });
    }

    fn background_loop(mut executor: ComponentExecutor) -> Result<()> {
        tracing::info!("entering background event loop");
        let _span_guard = info_span!("background event loop").entered();
        let mut loop_index: u128 = 0;
        let start = std::time::SystemTime::now();
        loop {
            tracing::trace!("background loop tick: {}", loop_index);
            let latest_time = start.elapsed().into_diagnostic()?.as_secs_f64();
            executor.tick(latest_time);

            thread::sleep(std::time::Duration::from_millis(10));
            loop_index += 1;
        }
        #[allow(unreachable_code)]
        {
            drop(_span_guard);
            unreachable!("Program broke out a never ending loop !")
        }
    }

    pub fn enter_event_loop(&mut self) {
        // do all the non-gui stuff
        Self::start_background_loop(self.app.executor("back"));

        tracing::info!("entering glfw event loop");
        let span_guard = info_span!("glfw event loop").entered();
        let mut ui_executor = self.app.executor("ui");

        ui_executor.init();

        loop {
            let JokolayGui {
                menu_panel,
                egui_context,
                glfw_backend,
            } = &mut self.gui;

            let latest_time = {
                let mut glfw_backend = glfw_backend.write().unwrap();
                let latest_time = glfw_backend.glfw.get_time();

                // gather events
                glfw_backend.glfw.poll_events();
                glfw_backend.tick();
                if glfw_backend.window.should_close() {
                    tracing::warn!("should close is true. So, exiting event loop");
                    break;
                }
                let mut input = glfw_backend.take_raw_input();
                input.time = Some(latest_time);

                egui_context.begin_frame(input);
                latest_time
            };
            ui_executor.tick(latest_time);

            if let Ok(mut menu_panel) = menu_panel.write() {
                menu_panel.gui(latest_time);
                JokolayTracingLayer::gui(egui_context, &mut menu_panel.show_tracing_window);
            } else {
                println!("cannot update GUI due to lock issues");
            }
            // show notifications
            JokolayTracingLayer::show_notifications(egui_context);

            // end gui stuff
            //egui_context.request_repaint();

            /*
            let animation_time = if ui_configuration.display_parameters.animate {
                latest_time
            } else {
                0.0
            };*/
        }
        drop(span_guard);
    }
}

pub fn start_jokolay() {
    let jokolay_dir = match get_jokolay_dir() {
        Ok(jdir) => jdir,
        Err(e) => {
            eprintln!("failed to create jokolay dir: {e:#?}");
            panic!("failed to create jokolay_dir: {e:#?}");
        }
    };
    let jokolay_path = get_jokolay_path().unwrap();

    let log_file_flush_guard = match JokolayTracingLayer::install_tracing(&jokolay_dir) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("failed to install tracing: {e:#?}");
            panic!("failed to install tracing: {e:#?}");
        }
    };

    if let Err(e) = rayon::ThreadPoolBuilder::default()
        .panic_handler(|panic_info| {
            error!(?panic_info, "rayon thread paniced.");
        })
        .build_global()
    {
        error!(
            ?e,
            "failed to set panic handler and build global threadpool for rayon"
        );
    }

    match Jokolay::new(jokolay_dir.into(), jokolay_path) {
        Ok(mut jokolay) => {
            jokolay.enter_event_loop();
        }
        Err(e) => {
            error!(?e, "failed to create Jokolay App");
        }
    };
    std::mem::drop(log_file_flush_guard);
}
