use std::{
    collections::BTreeMap,
};

pub struct SelectedFileManager {
    data: BTreeMap<String, bool>,

}
impl<'a> SelectedFileManager {
    pub fn new(
        selected_files: &BTreeMap<String, bool>, 
        pack_source_files: &BTreeMap<String, bool>,
        currently_used_files: &BTreeMap<String, bool>,
    ) -> Self {
        let mut list_of_enabled_files: BTreeMap<String, bool> = Default::default();
        SelectedFileManager::recursive_get_full_names(
            &selected_files,
            &pack_source_files,
            &currently_used_files,
            &mut list_of_enabled_files,
        );
        Self { data: list_of_enabled_files }
    }
    fn recursive_get_full_names(
        _selected_files: &BTreeMap<String, bool>, 
        _pack_source_files: &BTreeMap<String, bool>,
        currently_used_files: &BTreeMap<String, bool>,
        list_of_enabled_files: &mut BTreeMap<String, bool>
    ){
        for (key, v) in currently_used_files.iter() {
            list_of_enabled_files.insert(key.clone(), *v);
        }
    }
    pub fn cloned_data(&self) -> BTreeMap<String, bool> {
        self.data.clone()
    }
    pub fn is_selected(&self, source_file_name: &String) -> bool {
        let default = false;
        self.data.is_empty() || *self.data.get(source_file_name).unwrap_or(&default)
    }
    pub fn len(&self) -> usize {
        self.data.len()
    }
}
