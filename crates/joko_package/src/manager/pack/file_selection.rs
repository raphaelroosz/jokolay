use std::collections::BTreeMap;

use uuid::Uuid;

pub struct SelectedFileManager {
    data: BTreeMap<Uuid, bool>,

}
impl<'a> SelectedFileManager {
    pub fn new(
        selected_files: &BTreeMap<Uuid, bool>, 
        pack_source_files: &BTreeMap<Uuid, bool>,
        currently_used_files: &BTreeMap<Uuid, bool>,
    ) -> Self {
        let mut list_of_enabled_files: BTreeMap<Uuid, bool> = Default::default();
        SelectedFileManager::recursive_get_full_names(
            &selected_files,
            &pack_source_files,
            &currently_used_files,
            &mut list_of_enabled_files,
        );
        Self { data: list_of_enabled_files }
    }
    fn recursive_get_full_names(
        _selected_files: &BTreeMap<Uuid, bool>, 
        _pack_source_files: &BTreeMap<Uuid, bool>,
        currently_used_files: &BTreeMap<Uuid, bool>,
        list_of_enabled_files: &mut BTreeMap<Uuid, bool>
    ){
        for (key, v) in currently_used_files.iter() {
            list_of_enabled_files.insert(key.clone(), *v);
        }
    }
    pub fn cloned_data(&self) -> BTreeMap<Uuid, bool> {
        self.data.clone()
    }
    pub fn is_selected(&self, source_file_uuid: &Uuid) -> bool {
        let default = false;
        self.data.is_empty() || *self.data.get(source_file_uuid).unwrap_or(&default)
    }
    pub fn len(&self) -> usize {
        self.data.len()
    }
}
