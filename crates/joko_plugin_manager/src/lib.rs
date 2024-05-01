use joko_component_models::{
    default_data_exchange, ComponentChannels, ComponentDataExchange, JokolayComponent,
};

pub struct JokolayPlugin {}

pub struct JokolayPluginManager {}

impl JokolayComponent for JokolayPlugin {
    fn flush_all_messages(&mut self) {}
    fn tick(&mut self, _timestamp: f64) -> ComponentDataExchange {
        default_data_exchange()
    }
    fn bind(&mut self, _channels: ComponentChannels) {}
    fn requirements(&self) -> Vec<&str> {
        vec!["back:mumble_link"]
    }
}
