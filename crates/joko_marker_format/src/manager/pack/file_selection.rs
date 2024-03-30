use std::{
    collections::BTreeMap,
};
use ordered_hash_map::{OrderedHashMap};

pub struct SelectedFileManager {
    data: OrderedHashMap<String, bool>,

}
impl<'a> SelectedFileManager {
    pub fn new(
        selected_files: &OrderedHashMap<String, bool>, 
        pack_source_files: &OrderedHashMap<String, bool>,
        currently_used_files: &BTreeMap<String, bool>,
    ) -> Self {
        //TODO: build data
        let mut list_of_enabled_files: OrderedHashMap<String, bool> = Default::default();
        SelectedFileManager::recursive_get_full_names(
            &selected_files,
            &pack_source_files,
            &currently_used_files,
            &mut list_of_enabled_files,
        );
        Self { data: list_of_enabled_files }
    }
    fn recursive_get_full_names(
        _selected_files: &OrderedHashMap<String, bool>, 
        _pack_source_files: &OrderedHashMap<String, bool>,
        currently_used_files: &BTreeMap<String, bool>,
        list_of_enabled_files: &mut OrderedHashMap<String, bool>
    ){
        for (key, v) in currently_used_files.iter() {
            list_of_enabled_files.insert(key.clone(), *v);
        }
    }
    pub fn cloned_data(&self) -> OrderedHashMap<String, bool> {
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
