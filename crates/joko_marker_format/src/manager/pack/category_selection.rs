use std::collections::{HashSet, HashMap};
use ordered_hash_map::{OrderedHashMap};

use indexmap::IndexMap;
use uuid::Uuid;

use crate::{
    pack::{Category, CommonAttributes, PackCore},
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct CategorySelection {
    #[serde(skip)]
    pub guid: HashSet<Uuid>,//should be a HashSet of all the children markers/trails uuid and self (but not sub categories)
    pub selected: bool,
    pub separator: bool,
    pub display_name: String,
    pub children: OrderedHashMap<String, CategorySelection>,
}

pub struct SelectedCategoryManager {
    data: OrderedHashMap<String, CommonAttributes>,

}
impl<'a> SelectedCategoryManager {
    pub fn new(
        selected_categories: &OrderedHashMap<String, CategorySelection>,
        core_categories: &IndexMap<String, Category>
    ) -> Self {
        let mut list_of_enabled_categories = Default::default();
        CategorySelection::recursive_get_full_names(
            &selected_categories,
            &core_categories,
            &mut list_of_enabled_categories,
            "",
            &Default::default(),
        );
        
        Self { data: list_of_enabled_categories }
    }
    pub fn cloned_data(&self) -> OrderedHashMap<String, CommonAttributes> {
        self.data.clone()
    }
    pub fn is_selected(&self, category: &String) -> bool {
        self.data.contains_key(category)
    }
    pub fn get(&self, key: &String) -> &CommonAttributes {
        self.data.get(key).unwrap()
    }
    pub fn len(&self) -> usize {
        self.data.len()
    }
    pub fn keys(&'a self ) -> ordered_hash_map::ordered_map::Keys<'a, String, CommonAttributes> {
        self.data.keys()
    }
}

static mut once: bool = true;

impl CategorySelection {
    pub fn default_from_pack_core(pack: &PackCore) -> OrderedHashMap<String, CategorySelection> {
        let mut selection = OrderedHashMap::new();
        Self::recursive_create_category_selection(&mut selection, &pack.categories);
        selection
    }
    fn recursive_get_full_names(
        selection: &OrderedHashMap<String, CategorySelection>,
        core_categories: &IndexMap<String, Category>,
        list_of_enabled_categories: &mut OrderedHashMap<String, CommonAttributes>,
        parent_name: &str,
        parent_common_attributes: &CommonAttributes,
    ) {
        for (name, cat) in core_categories {
            if let Some(selected_cat) = selection.get(name) {
                if !selected_cat.selected {
                    continue;
                }
                let full_name = if parent_name.is_empty() {
                    name.clone()
                } else {
                    format!("{parent_name}.{name}")
                }.to_lowercase();
                let mut common_attributes = cat.props.clone();
                common_attributes.inherit_if_attr_none(parent_common_attributes);
                Self::recursive_get_full_names(
                    &selected_cat.children,
                    &cat.children,
                    list_of_enabled_categories,
                    &full_name,
                    &common_attributes,
                );
                list_of_enabled_categories.insert(full_name, common_attributes);
            }
        }
    }
    pub fn recursive_populate_guids(
        selection: &mut OrderedHashMap<String, CategorySelection>,
        all_pack_guids: &HashMap<String, HashSet<Uuid>>,
        parent_name: Option<String>,
    ) {
        for (cat_name, cat) in selection.iter_mut() {
            let current_name = if let Some(parent_name) = &parent_name {
                format!("{}.{}", parent_name, cat_name)
            } else {
                cat_name.clone()
            };
            if let Some(other_existing_uuid) = all_pack_guids.get(&current_name) {
                cat.guid.extend(other_existing_uuid);
            }
            Self::recursive_populate_guids(&mut cat.children, all_pack_guids, Some(current_name));
                for child in cat.children.values() {
                    cat.guid.extend(&child.guid);
                }
            //assert!(cat.guid.len() > 0);
        }
    }
    fn recursive_create_category_selection(
        selection: &mut OrderedHashMap<String, CategorySelection>,
        cats: &IndexMap<String, Category>,
    ) {
        for (cat_name, cat) in cats.iter() {
            if !selection.contains_key(cat_name) {
                let mut all_uuids: HashSet<Uuid> = Default::default();
                all_uuids.insert(cat.guid);
                let to_insert = CategorySelection {
                    guid: all_uuids,
                    selected: cat.default_enabled,
                    separator: cat.separator,
                    display_name: cat.display_name.clone(),
                    children: Default::default(),
                };
                selection.insert(cat_name.clone(), to_insert);
            }
            let s = selection.get_mut(cat_name).unwrap();
            Self::recursive_create_category_selection(&mut s.children, &cat.children);
        }
    }

    pub fn recursive_selection_ui(
        selection: &mut OrderedHashMap<String, CategorySelection>,
        ui: &mut egui::Ui,
        is_dirty: &mut bool,
        on_screen: &HashSet<Uuid>,
    ) {
        if selection.is_empty() {
            return;
        }
        egui::ScrollArea::vertical().show(ui, |ui| {
            for (_name, cat) in selection.iter_mut() {
                ui.horizontal(|ui| {
                    if cat.separator {
                        ui.add_space(3.0);
                    } else {
                        let cb = ui.checkbox(&mut cat.selected, "");
                        if cb.changed() {
                            *is_dirty = true;
                        }
                    }
                    let mut is_current_branch_displayed = on_screen.intersection(&cat.guid).count() > 0;
                    let color = if is_current_branch_displayed {
                        egui::Color32::LIGHT_GREEN
                    } else {
                        egui::Color32::GRAY
                    };
                    let label = egui::RichText::new(&cat.display_name).color(color);
                    if cat.children.is_empty() {
                        ui.label(label);
                    } else {
                        ui.menu_button(label, |ui: &mut egui::Ui| {
                            Self::recursive_selection_ui(&mut cat.children, ui, is_dirty, on_screen);
                        });
                    }
                });
            }
        });
    }
}

