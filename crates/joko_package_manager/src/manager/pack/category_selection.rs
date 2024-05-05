use joko_component_models::{to_data, ComponentMessage};
use joko_package_models::{
    attributes::CommonAttributes,
    category::Category,
    package::{PackCore, PackageImportReport},
};
use ordered_hash_map::OrderedHashMap;
use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::message::MessageToPackageBack;

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct CategorySelection {
    //#[serde(skip)]
    pub uuid: Uuid, //FIXME: if not present, one MUST fix it or mark the current import as a failure and reset all information
    #[serde(skip)]
    pub parent: Option<Uuid>,
    pub is_selected: bool, //has it been selected in configuration to be displayed
    pub is_active: bool,   //currently being displayed (i.e.: active)
    pub separator: bool,
    pub display_name: String,
    pub children: OrderedHashMap<String, CategorySelection>,
}

pub struct SelectedCategoryManager {
    data: OrderedHashMap<Uuid, CommonAttributes>,
}
impl<'a> SelectedCategoryManager {
    pub fn new(
        selected_categories: &OrderedHashMap<String, CategorySelection>,
        categories: &OrderedHashMap<Uuid, Category>,
    ) -> Self {
        let mut list_of_enabled_categories = Default::default();
        CategorySelection::get_list_of_enabled_categories(
            selected_categories,
            categories,
            &mut list_of_enabled_categories,
            &Default::default(),
        );

        Self {
            data: list_of_enabled_categories,
        }
    }
    #[allow(dead_code)]
    pub fn cloned_data(&self) -> OrderedHashMap<Uuid, CommonAttributes> {
        self.data.clone()
    }
    pub fn is_selected(&self, category: &Uuid) -> bool {
        self.data.contains_key(category)
    }
    pub fn get(&self, key: &Uuid) -> &CommonAttributes {
        self.data.get(key).unwrap()
    }
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.data.len()
    }
    pub fn keys(&'a self) -> ordered_hash_map::ordered_map::Keys<'a, Uuid, CommonAttributes> {
        self.data.keys()
    }
}

impl CategorySelection {
    pub fn default_from_pack_core(pack: &PackCore) -> OrderedHashMap<String, CategorySelection> {
        let mut selectable_categories = OrderedHashMap::new();
        Self::recursive_create_selectable_categories(&mut selectable_categories, &pack.categories);
        selectable_categories
    }
    fn get_list_of_enabled_categories(
        selection: &OrderedHashMap<String, CategorySelection>,
        categories: &OrderedHashMap<Uuid, Category>,
        list_of_enabled_categories: &mut OrderedHashMap<Uuid, CommonAttributes>,
        parent_common_attributes: &CommonAttributes,
    ) {
        for (_, cat) in categories {
            if let Some(selectable_category) = selection.get(&cat.relative_category_name) {
                if !selectable_category.is_selected {
                    continue;
                }
                let mut common_attributes = cat.props.clone();
                common_attributes.inherit_if_attr_none(parent_common_attributes);
                Self::get_list_of_enabled_categories(
                    &selectable_category.children,
                    &cat.children,
                    list_of_enabled_categories,
                    &common_attributes,
                );
                list_of_enabled_categories.insert(cat.guid, common_attributes);
            }
        }
    }
    pub fn get(
        selection: &mut OrderedHashMap<String, CategorySelection>,
        uuid: Uuid,
    ) -> Option<&mut CategorySelection> {
        if selection.is_empty() {
            None
        } else {
            for cat in selection.values_mut() {
                if cat.uuid == uuid {
                    return Some(cat);
                }
                if let Some(res) = Self::get(&mut cat.children, uuid) {
                    return Some(res);
                }
            }
            None
        }
    }
    #[allow(dead_code)]
    pub fn recursive_populate_guids(
        selection: &mut OrderedHashMap<String, CategorySelection>,
        entities_parents: &mut HashMap<Uuid, Uuid>,
        parent_uuid: Option<Uuid>,
    ) {
        for cat in selection.values_mut() {
            if cat.uuid.is_nil() {
                cat.uuid = Uuid::new_v4();
            }
            cat.parent = parent_uuid;
            Self::recursive_populate_guids(&mut cat.children, entities_parents, Some(cat.uuid));
            if let Some(parent_uuid) = parent_uuid {
                entities_parents.insert(cat.uuid, parent_uuid);
            }
            //assert!(cat.guid.len() > 0);
        }
    }
    fn recursive_create_selectable_categories(
        selectable_categories: &mut OrderedHashMap<String, CategorySelection>,
        cats: &OrderedHashMap<Uuid, Category>,
    ) {
        for (_, cat) in cats.iter() {
            if !selectable_categories.contains_key(&cat.relative_category_name) {
                let to_insert = CategorySelection {
                    uuid: cat.guid,
                    parent: cat.parent,
                    is_selected: cat.default_enabled,
                    is_active: !cat.separator, //by default separators are not considered active since they contain nothing
                    separator: cat.separator,
                    display_name: cat.display_name.clone(),
                    children: Default::default(),
                };
                //println!("recursive_create_category_selection {} {}", cat_name, to_insert.uuid);
                selectable_categories.insert(cat.relative_category_name.clone(), to_insert);
            }
            let s = selectable_categories
                .get_mut(&cat.relative_category_name)
                .unwrap();
            Self::recursive_create_selectable_categories(&mut s.children, &cat.children);
        }
    }

    pub fn recursive_set(
        selection: &mut OrderedHashMap<String, CategorySelection>,
        uuid: Uuid,
        status: bool,
    ) -> bool {
        if selection.is_empty() {
            false
        } else {
            for cat in selection.values_mut() {
                if cat.separator {
                    continue;
                }
                if cat.uuid == uuid {
                    cat.is_selected = status;
                    return true;
                }
                if Self::recursive_set(&mut cat.children, uuid, status) {
                    return true;
                }
            }
            false
        }
    }
    pub fn recursive_set_all(
        selection: &mut OrderedHashMap<String, CategorySelection>,
        status: bool,
    ) {
        if selection.is_empty() {
            return;
        }
        for cat in selection.values_mut() {
            if cat.separator {
                continue;
            }
            cat.is_selected = status;
            Self::recursive_set_all(&mut cat.children, status);
        }
    }

    pub fn recursive_update_active_categories(
        selection: &mut OrderedHashMap<String, CategorySelection>,
        active_elements: &HashSet<Uuid>,
    ) -> bool {
        let mut is_active = false;
        if selection.is_empty() {
            //println!("recursive_update_active_categories is_empty");
            return is_active;
        }
        for cat in selection.values_mut() {
            cat.is_active = active_elements.contains(&cat.uuid)
                || Self::recursive_update_active_categories(&mut cat.children, active_elements);
            if cat.is_active {
                is_active = true;
            }
        }
        is_active
    }

    fn context_menu(
        u2b_sender: &tokio::sync::mpsc::Sender<ComponentMessage>,
        cs: &mut CategorySelection,
        ui: &mut egui::Ui,
    ) {
        if ui.button("Activate branch").clicked() {
            cs.is_selected = true;
            CategorySelection::recursive_set_all(&mut cs.children, true);
            let _ = u2b_sender.blocking_send(to_data(
                MessageToPackageBack::CategoryActivationBranchStatusChange(cs.uuid, true),
            ));
            ui.close_menu();
        }
        if ui.button("Deactivate branch").clicked() {
            CategorySelection::recursive_set_all(&mut cs.children, false);
            cs.is_selected = false;
            let _ = u2b_sender.blocking_send(to_data(
                MessageToPackageBack::CategoryActivationBranchStatusChange(cs.uuid, false),
            ));
            ui.close_menu();
        }
    }

    pub fn recursive_selection_ui(
        back_end_notifier: &tokio::sync::mpsc::Sender<ComponentMessage>,
        selection: &mut OrderedHashMap<String, CategorySelection>,
        ui: &mut egui::Ui,
        is_dirty: &mut bool,
        show_only_active: bool,
        import_quality_report: &PackageImportReport,
    ) {
        if selection.is_empty() {
            return;
        }
        egui::ScrollArea::vertical().show(ui, |ui| {
            for cat in selection.values_mut() {
                if !cat.is_active && show_only_active && !cat.separator {
                    continue;
                }
                ui.horizontal(|ui| {
                    if cat.separator {
                        ui.add_space(3.0);
                    } else {
                        let cb = ui.checkbox(&mut cat.is_selected, "");
                        if cb.changed() {
                            let _ = back_end_notifier.blocking_send(to_data(
                                MessageToPackageBack::CategoryActivationElementStatusChange(
                                    cat.uuid,
                                    cat.is_selected,
                                ),
                            ));
                            *is_dirty = true;
                        }
                    }
                    //println!("Look for {} {} among displayed elements {}", name,  cat.uuid, on_screen.contains(&cat.uuid));
                    let color = if import_quality_report.is_category_discovered_late(cat.uuid) {
                        egui::Color32::LIGHT_RED
                    } else if cat.is_active {
                        egui::Color32::LIGHT_GREEN
                    } else {
                        egui::Color32::GRAY
                    };
                    let label = egui::RichText::new(&cat.display_name).color(color);
                    if cat.children.is_empty() {
                        ui.label(label);
                    } else {
                        ui.menu_button(label, |ui: &mut egui::Ui| {
                            Self::recursive_selection_ui(
                                back_end_notifier,
                                &mut cat.children,
                                ui,
                                is_dirty,
                                show_only_active,
                                import_quality_report,
                            );
                        })
                        .response
                        .context_menu(|ui| Self::context_menu(back_end_notifier, cat, ui));
                    }
                });
            }
        });
    }
}
