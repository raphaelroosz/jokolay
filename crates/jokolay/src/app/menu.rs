use std::sync::{Arc, RwLock};

use egui_window_glfw_passthrough::GlfwBackend;
use joko_component_models::{
    default_component_result, from_broadcast, Component, ComponentChannels, ComponentResult,
};
use joko_link_models::{MumbleLinkResult, UISize};
use joko_ui_models::{UIArea, UIPanel};
use tracing::info;

use super::window::{MINIMAL_WINDOW_HEIGHT, MINIMAL_WINDOW_WIDTH};

struct MenuPanel {
    panel: Arc<RwLock<dyn UIPanel>>,
    areas: Vec<UIArea>,
    nb_draw: u128,
    draw_time: std::time::Duration,
}

struct MenuPanelManagerChannels {
    subscription_mumblelink: tokio::sync::broadcast::Receiver<ComponentResult>,
}

/// Guild Wars 2 has an array of menu icons on top left corner of the game.
/// Its size is affected by four different factors
/// 1. UISZ:
///     This is a setting in graphics options of gw2 and it comes in 4 variants
///     small, normal, large and larger.
///     This is something we can get from mumblelink's context.
/// 2. DPI scaling
///     This is a setting in graphics options too. When scaling is enabled, sizes of menu become bigger according to the dpi of gw2 window
///     This is something we get from gw2's config file in AppData/Roaming and store in mumble link as dpi scaling
///     We also get dpi of gw2 window and store it in mumble link.
/// 3. Dimensions of the gw2 window
///     This is something we get from mumble link and win32 api. We store this as client pos/size in mumble link
///     It is not just the width or height, but their ratio to the 1024x768 resolution
///
/// 1. By default, with dpi 96 (scale 1.0), at resolution 1024x768 these are the sizes of menu at different uisz settings
///     UISZ   -> WIDTH   HEIGHT
///     small  -> 288     27
///     normal -> 319     31
///     large  -> 355     34
///     larger -> 391     38
///     all units are in raw pixels.
///     
///     If we think of small uisz as the default. Then, we can express the rest of the sizes as ratio to small.
///     small = 1.0
///     normal = 1.1
///     large = 1.23
///     larger = 1.35
///     
///     So, just multiply small (288) with these ratios to get the actual pixels of each uisz.
/// 2. When dpi doubles, so do the sizes. 288 -> 576, 319 -> 638 etc.. So, when dpi scaling is enabled, we must multiply the above uisz ratio with dpi scale ratio to get the combined scaling ratio.
/// 3. The dimensions thing is a little complicated. So, i will just list the actual steps here.
///     1. take gw2's actual width in raw pixels. lets call this gw2_width.
///     2. take 1024 as reference minimum width. If dpi scaling is enabled, multiply 1024 * dpi scaling ratio. lets call this reference_width.
///     3. Now, get the smaller value out of the two. lets call this minimum_width.
///     4. finally, do (minimum_width / reference_width) to get "width scaling ratio".
///     5. repeat steps 1 - 4, but for height. use 768 as the reference width (with approapriate dpi scaling).
///     6. now just take the minimum of "width scaling ratio" and "height scaling ratio". lets call this "aspect ratio scaling".
///
/// Finally, just multiply the width 288 or height 27 with these three values.
/// eg: menu width = 288 * uisz_ratio * dpi_scaling_ratio * aspect_ratio_scaling;
/// do the same with 288 replaced by 27 for height.
pub struct MenuPanelManager {
    pub pos: egui::Pos2,
    pub ui_scaling_factor: f32,
    pub show_tracing_window: bool,
    glfw_backend: Arc<RwLock<GlfwBackend>>,
    egui_context: egui::Context,
    menus: Vec<MenuPanel>,
    channels: Option<MenuPanelManagerChannels>,
}

unsafe impl Send for MenuPanelManager {}
unsafe impl Sync for MenuPanelManager {}

impl MenuPanelManager {
    pub const WIDTH: f32 = 288.0;
    pub const HEIGHT: f32 = 27.0;

    pub fn new(glfw_backend: Arc<RwLock<GlfwBackend>>, egui_context: egui::Context) -> Self {
        Self {
            glfw_backend,
            egui_context,
            pos: Default::default(),
            show_tracing_window: Default::default(),
            ui_scaling_factor: Default::default(),
            menus: Default::default(),
            channels: None,
        }
    }

    pub fn register(&mut self, component: Arc<RwLock<dyn UIPanel>>) {
        self.menus.push(MenuPanel {
            panel: component.clone(),
            areas: component.read().unwrap().areas(),
            nb_draw: 0,
            draw_time: Default::default(),
        })
    }

    pub fn gui(&mut self, latest_time: f64) {
        //let mut glfw_backend = self.glfw_backend.();
        // do the gui stuff now
        egui::Area::new("menu panel")
            .fixed_pos(self.pos)
            .interactable(true)
            .order(egui::Order::Foreground)
            .show(&self.egui_context, |ui| {
                ui.style_mut().visuals.widgets.inactive.weak_bg_fill = egui::Color32::TRANSPARENT;
                ui.horizontal(|ui| {
                    ui.menu_button(
                        egui::RichText::new("JKL")
                            .size((MenuPanelManager::HEIGHT - 2.0) * self.ui_scaling_factor)
                            .background_color(egui::Color32::TRANSPARENT),
                        |ui: &mut egui::Ui| {
                            let mut any_open = false;
                            for panel in self.menus.iter_mut() {
                                for area in panel.areas.iter_mut() {
                                    if area.name.is_empty() {
                                        continue;
                                    }
                                    ui.checkbox(&mut area.is_open, &area.name);
                                    any_open = any_open || area.is_open;
                                }
                            }
                            //ui.checkbox(&mut menu_panel.show_tracing_window, "Show Logs");
                            if any_open && ui.button("Close all panels").clicked() {
                                for panel in self.menus.iter_mut() {
                                    for area in panel.areas.iter_mut() {
                                        area.is_open = false;
                                    }
                                }
                            }
                            if ui.button("exit").clicked() {
                                info!("exiting jokolay");
                                self.glfw_backend
                                    .write()
                                    .unwrap()
                                    .window
                                    .set_should_close(true);
                            }
                        },
                    );
                    for panel in self.menus.iter_mut() {
                        let handle = &mut panel.panel.write().unwrap();
                        handle.menu_ui(ui);
                    }
                });
            });
        for panel in self.menus.iter_mut() {
            let handle = &mut panel.panel.write().unwrap();
            let start = std::time::SystemTime::now();
            for area in panel.areas.iter_mut() {
                handle.gui(&mut area.is_open, &area.id, latest_time);
            }
            panel.nb_draw += 1;
            panel.draw_time += start.elapsed().unwrap();
        }
    }
}

fn convert_uisz_to_scale(uisize: UISize) -> f32 {
    const SMALL: f32 = 288.0;
    const NORMAL: f32 = 319.0;
    const LARGE: f32 = 355.0;
    const LARGER: f32 = 391.0;
    const SMALL_SCALING_RATIO: f32 = 1.0;
    const NORMAL_SCALING_RATIO: f32 = NORMAL / SMALL;
    const LARGE_SCALING_RATIO: f32 = LARGE / SMALL;
    const LARGER_SCALING_RATIO: f32 = LARGER / SMALL;
    match uisize {
        UISize::Small => SMALL_SCALING_RATIO,
        UISize::Normal => NORMAL_SCALING_RATIO,
        UISize::Large => LARGE_SCALING_RATIO,
        UISize::Larger => LARGER_SCALING_RATIO,
    }
}
/*
Just some random measurements to verify in the future (or write tests for :))
with dpi enabled, there's some math involved it seems.
Linux ->
width 1920 pixels. height 2113 pixels. ratio 0.91. fov 1.01. scaling 2.0. dpi enabled
small  -> 540     53
normal -> 599     59
large  -> 667     65
larger -> 734     72


Windows ->
width 1920 pixels. height 2113 pixels. ratio 0.91. fov 1.01. scaling 2.0. dpi enabled.
small  -> 540     53
normal -> 599     59
large  -> 667     65
larger -> 734     72

width 1914 pixels. height 2072 pixels. ratio 0.92. fov 1.01. scaling 3.0. dpi enabled. dpi 288
small  -> 538     52
normal -> 598     58
large  -> 665     65
larger -> 731     72

width 3840. height 2160. ratio 1.78. scaling 3. dpi true. dpi 288 (windowed fullscreen)
small  -> 810     80
normal -> 900     89
large  -> 1000    99
larger -> 1100    109

width 1916 pixels. height 2113 pixels. ratio 0.91. fov 1.01. scaling 1.5. dpi enabled. dpi 144
small  -> 432     42
normal -> 480     47
large  -> 533     52
larger -> 586     57

width 1000 pixels. height 1000 pixels. ratio 1. fov 1.01. scaling 2.0. dpi enabled.
small  -> 281     26
normal -> 312     29
large  -> 347     33
larger -> 382     36

width 2000 pixels. height 1000 pixels. ratio 2. fov 1.01. scaling 2.0. dpi enabled.
small  -> 375     36
normal -> 416     40
large  -> 463     45
larger -> 509     49

width 2000 pixels. height 2000 pixels. ratio 1. fov 1.01. scaling 2.0. dpi enabled.
small  -> 562     55
normal -> 624     61
large  -> 694     68
larger -> 764     75


*/

impl Component for MenuPanelManager {
    fn init(&mut self) {}
    fn bind(&mut self, mut channels: ComponentChannels) {
        let channels = MenuPanelManagerChannels {
            subscription_mumblelink: channels.requirements.remove(&0).unwrap(),
        };
        self.channels = Some(channels);
    }
    fn accept_notifications(&self) -> bool {
        false
    }
    fn flush_all_messages(&mut self) {}
    fn requirements(&self) -> Vec<&str> {
        vec!["ui:mumble_link"]
    }
    fn tick(&mut self, _latest_time: f64) -> ComponentResult {
        assert!(
            self.channels.is_some(),
            "channels must be initialized before interacting with component."
        );
        let egui_context = &self.egui_context;
        let raw_link = {
            let channels = self.channels.as_mut().unwrap();
            channels.subscription_mumblelink.try_recv().unwrap()
        };
        let link_result: MumbleLinkResult = from_broadcast(&raw_link);

        let mut ui_scaling_factor = 1.0;
        if let Some(link) = link_result.link.as_ref() {
            let gw2_scale: f32 = if link.dpi_scaling == 1 || link.dpi_scaling == -1 {
                (if link.dpi == 0 { 96.0 } else { link.dpi as f32 }) / 96.0
            } else {
                1.0
            };

            ui_scaling_factor *= gw2_scale;
            let uisz_scale = convert_uisz_to_scale(link.uisz);
            ui_scaling_factor *= uisz_scale;

            let min_width = MINIMAL_WINDOW_WIDTH as f32 * gw2_scale;
            let min_height = MINIMAL_WINDOW_HEIGHT as f32 * gw2_scale;
            let gw2_width = link.client_size.0.x.max(MINIMAL_WINDOW_WIDTH) as f32;
            let gw2_height = link.client_size.0.y.max(MINIMAL_WINDOW_HEIGHT) as f32;
            let min_width_ratio = min_width.min(gw2_width) / min_width;
            let min_height_ratio = min_height.min(gw2_height) / min_height;

            let min_ratio = min_height_ratio.min(min_width_ratio);
            ui_scaling_factor *= min_ratio;

            let egui_scale = egui_context.pixels_per_point();
            ui_scaling_factor /= egui_scale;
        }

        self.pos.x = ui_scaling_factor * (Self::WIDTH + 8.0); // add 8 pixels padding just for some space
        self.ui_scaling_factor = ui_scaling_factor;
        default_component_result()
    }
}
