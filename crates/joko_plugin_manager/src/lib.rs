use std::path::PathBuf;

use joko_component_models::{
    default_component_result, Component, ComponentChannels, ComponentResult,
};

pub struct JokolayPlugin {}

pub struct JokolayPluginManager {
    #[allow(dead_code)]
    path: PathBuf,
}

impl JokolayPluginManager {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
    pub fn create(&mut self, _name: String) -> JokolayPlugin {
        JokolayPlugin {}
    }
}
impl Component for JokolayPluginManager {
    fn init(&mut self) {}
    fn flush_all_messages(&mut self) {}
    fn tick(&mut self, _timestamp: f64) -> ComponentResult {
        default_component_result()
    }
    fn bind(&mut self, _channels: ComponentChannels) {}
}

impl Component for JokolayPlugin {
    fn init(&mut self) {
        println!("initialize dummy plugin");
    }
    fn flush_all_messages(&mut self) {}
    fn tick(&mut self, _timestamp: f64) -> ComponentResult {
        default_component_result()
    }
    fn bind(&mut self, _channels: ComponentChannels) {}
    fn requirements(&self) -> Vec<&str> {
        vec!["back:mumble_link"]
    }
}
