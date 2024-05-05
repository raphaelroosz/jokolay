use egui::Ui;

pub struct UIArea {
    pub is_open: bool,
    pub name: String,
    pub id: String,
}
pub trait UIPanel {
    fn init(&mut self);
    fn gui(&mut self, is_open: &mut bool, area_id: &str);
    fn menu_ui(&mut self, _ui: &mut Ui) {}
    fn areas(&self) -> Vec<UIArea>;
}
