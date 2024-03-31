mod common;
mod marker;
mod trail;
mod route;

use std::{collections::{HashMap, HashSet}, str::FromStr};

use indexmap::IndexMap;
use ordered_hash_map;

use tracing::info;

pub use common::*;
pub(crate) use marker::*;
use smol_str::SmolStr;
pub(crate) use trail::*;
pub(crate) use route::*;
use uuid::Uuid;


#[derive(Default, Debug, Clone)]
pub(crate) struct PackCore {
    pub textures: ordered_hash_map::OrderedHashMap<RelativePath, Vec<u8>>,
    pub tbins: ordered_hash_map::OrderedHashMap<RelativePath, TBin>,
    pub categories: IndexMap<String, Category>,
    pub all_categories: HashMap<String, Uuid>,
    pub entities_parents: HashMap<Uuid, Uuid>,
    pub source_files: ordered_hash_map::OrderedHashMap<String, bool>,//TODO: have a reference containing pack name and maybe even path inside the package
    pub maps: ordered_hash_map::OrderedHashMap<u32, MapData>,
}

impl PackCore {
    pub fn register_uuid(&mut self, full_category_name: &String, uuid: &Uuid) {
        if let Some(parent_uuid) = self.all_categories.get(full_category_name) {
            self.entities_parents.insert(*uuid, *parent_uuid);
        } else {
            //FIXME: this means a broken package, we could fix it by making usage of the relative category the node is in.
            info!("Can't register world entity {} {}, no associated category found.", full_category_name, uuid);
        }
    }
    pub fn register_categories(&mut self) {
        let mut entities_parents: HashMap<Uuid, Uuid> = Default::default();
        let mut all_categories: HashMap<String, Uuid> = Default::default();
        self.recursive_register_categories(&mut entities_parents, &self.categories, &mut all_categories, None);
        self.entities_parents.extend(entities_parents);
        info!("Catepories registered: {}", all_categories.len());
        self.all_categories = all_categories;
    }
    fn recursive_register_categories(
        &self, 
        entities_parents: &mut HashMap<Uuid, Uuid>, 
        categories: &IndexMap<String, Category>, 
        all_categories: &mut HashMap<String, Uuid>,
        parent_name: Option<String>
    ) {
        for (name, cat) in categories.iter() {
            let full_category_name: String = if let Some(parent_name) = &parent_name {
                format!("{}.{}", parent_name, name)
            } else {
                name.to_string()
            };
            //println!("Register catepory {} {} {:?}", full_category_name, cat.guid, cat.parent);
            all_categories.insert(full_category_name.clone(), cat.guid);
            if let Some(parent) = cat.parent {
                entities_parents.insert(cat.guid, parent);
            }
            self.recursive_register_categories(entities_parents, &cat.children, all_categories, Some(full_category_name));
        }
    }
}

#[derive(Default, Debug, Clone)]
pub(crate) struct MapData {
    pub markers: IndexMap<Uuid, Marker>,
    pub routes: IndexMap<Uuid, Route>,
    pub trails: IndexMap<Uuid, Trail>,
}

#[derive(Debug, Clone)]
pub(crate) struct Category {
    pub guid: Uuid,
    pub parent: Option<Uuid>,
    pub display_name: String,
    pub separator: bool,
    pub default_enabled: bool,
    pub props: CommonAttributes,
    pub children: IndexMap<String, Category>,
}

/// This newtype is used to represents relative paths in marker packs
/// 1. It won't start with `/` or `C:` like roots, because its a relative path
/// 2. It can be empty to represent current directory
/// 3. No expansion of special characters like  `.` or `..` stuff.
/// 4. It is always lowercase to avoid platform specific quirks.
/// 5. It will use `/` as the path separator.
/// 6. It doesn't mean that the path is valid. It may contain many of the utf-8 characters which are not valid path names on linux/windows
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RelativePath(SmolStr);
#[allow(unused)]
impl RelativePath {
    pub fn join_str(&self, path: &str) -> Self {
        let path = path.trim_start_matches('/');
        if path.is_empty() {
            return Self(self.0.clone());
        }
        let lower_case = path.to_lowercase();
        if self.0.is_empty() {
            // no need to push `/` if we are empty, as that would make it an absolute path
            return Self(lower_case.into());
        }

        let mut new = self.0.to_string();
        if !self.0.ends_with('/') {
            new.push('/');
        }
        new.push_str(&lower_case);
        Self(new.into())
    }

    pub fn ends_with(&self, ext: &str) -> bool {
        self.0.ends_with(ext)
    }
    pub fn is_png(&self) -> bool {
        self.ends_with(".png")
    }
    pub fn is_tbin(&self) -> bool {
        self.ends_with(".trl")
    }
    pub fn is_xml(&self) -> bool {
        self.ends_with(".xml")
    }
    pub fn is_dir(&self) -> bool {
        self.ends_with("/")
    }
    pub fn parent(&self) -> Option<&str> {
        let path = self.0.trim_end_matches('/');
        if path.is_empty() {
            return None;
        }
        path.rfind('/').map(|index| &path[..=index])
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for RelativePath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl From<RelativePath> for String {
    fn from(val: RelativePath) -> String {
        val.0.into()
    }
}
impl FromStr for RelativePath {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let path = s.trim_start_matches('/');
        if path.is_empty() {
            return Ok(Self::default());
        }
        Ok(Self(path.to_lowercase().into()))
    }
}
