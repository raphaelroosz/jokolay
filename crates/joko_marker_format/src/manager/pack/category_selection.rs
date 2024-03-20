use ordered_hash_map::{OrderedHashMap};

use indexmap::IndexMap;

use crate::{
    pack::{Category, CommonAttributes, PackCore},
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct CategorySelection {
    pub selected: bool,
    pub separator: bool,
    pub display_name: String,
    pub children: OrderedHashMap<String, CategorySelection>,
}

impl CategorySelection {
    pub fn default_from_pack_core(pack: &PackCore) -> OrderedHashMap<String, CategorySelection> {
        let mut selection = OrderedHashMap::new();
        Self::recursive_create_category_selection(&mut selection, &pack.categories);
        selection
    }
    pub fn recursive_get_full_names(
        selection: &OrderedHashMap<String, CategorySelection>,
        cats: &IndexMap<String, Category>,
        list_of_enabled_categories: &mut OrderedHashMap<String, CommonAttributes>,
        parent_name: &str,
        parent_common_attributes: &CommonAttributes,
    ) {
        for (name, cat) in cats {
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
    fn recursive_create_category_selection(
        selection: &mut OrderedHashMap<String, CategorySelection>,
        cats: &IndexMap<String, Category>,
    ) {
        for (cat_name, cat) in cats.iter() {
            if !selection.contains_key(cat_name) {
                let mut to_insert = CategorySelection::default();
                to_insert.selected = cat.default_enabled;
                to_insert.separator = cat.separator;
                to_insert.display_name = cat.display_name.clone();
                selection.insert(cat_name.clone(), to_insert);
            }
            let s = selection.get_mut(cat_name).unwrap();
            Self::recursive_create_category_selection(&mut s.children, &cat.children);
        }
    }

    pub fn recursive_selection_ui(
        selection: &mut OrderedHashMap<String, CategorySelection>,
        ui: &mut egui::Ui,
        changed: &mut bool,
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
                            *changed = true;
                        }
                    }
                    if cat.children.is_empty() {
                        ui.label(&cat.display_name);
                    } else {
                        ui.menu_button(&cat.display_name, |ui: &mut egui::Ui| {
                            Self::recursive_selection_ui(&mut cat.children, ui, changed);
                        });
                    }
                });
            }
        });
    }
}

