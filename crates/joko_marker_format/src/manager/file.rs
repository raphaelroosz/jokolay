use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

use cap_std::fs_utf8::Dir;
use egui::{CollapsingHeader, ColorImage, TextureHandle, Window};
use image::EncodableLayout;

use tracing::{error, info, info_span};

use jokolink::MumbleLink;
use miette::{Context, IntoDiagnostic, Result};

use crate::manager::pack::loaded::LoadedPack;

pub const FILE_MANAGER_DIRECTORY_NAME: &str = "file_manager";

pub struct FileManager {
    /// holds data that is useful for the ui
    ui_data: FileManagerUI,
    /// marker manager directory. not useful yet, but in future we could be using this to store config files etc..
    /// These are the marker packs
    /// The key is the name of the pack
    /// The value is a loaded pack that contains additional data for live marker packs like what needs to be saved or category selections etc..
    packs: BTreeMap<String, LoadedPack>,
    missing_texture: Option<TextureHandle>,
    missing_trail: Option<TextureHandle>,
    /// This is the interval in number of seconds when we check if any of the packs need to be saved due to changes.
    /// This allows us to avoid saving the pack too often.
    pub save_interval: f64,
}

#[derive(Debug, Default)]
pub(crate) struct FileManagerUI {
    // tf is this type supposed to be? maybe we should have used a ECS for this reason.
    
}


impl FileManager {
    pub fn new(jdir: &Dir) -> Result<Self> {
        jdir.create_dir_all(FILE_MANAGER_DIRECTORY_NAME)
            .into_diagnostic()
            .wrap_err("failed to create file manager directory")?;
        let mut packs: BTreeMap<String, LoadedPack> = Default::default();

        Ok(Self {
            packs,
            ui_data: Default::default(),
            save_interval: 0.0,
            missing_texture: None,
            missing_trail: None
        })
    }

    pub fn tick(
        &mut self,
        etx: &egui::Context,
        timestamp: f64,
        joko_renderer: &mut joko_render::JokoRenderer,
        link: &Option<Arc<MumbleLink>>,
    ) {
    }
    pub fn menu_ui(&mut self, ui: &mut egui::Ui) {
        ui.menu_button("Files", |ui| {
            for pack in self.packs.values_mut() {
                pack.category_sub_menu(ui);
            }
        });
    }

    pub fn gui(&mut self, etx: &egui::Context, open: &mut bool) {
        Window::new("File Manager").open(open).show(etx, |ui| -> Result<()> {
            //TODO: display list of currently loaded files
            Ok(())
        });
    }
}

