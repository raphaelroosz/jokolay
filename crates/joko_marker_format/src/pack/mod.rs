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
    pub all_guids: HashMap<String, HashSet<Uuid>>,
    pub source_files: ordered_hash_map::OrderedHashMap<String, bool>,//TODO: have a reference containing pack name and maybe even path inside the package
    pub maps: ordered_hash_map::OrderedHashMap<u32, MapData>,
}

impl PackCore {
    pub fn register_uuid(&mut self, full_category_name: &String, uuid: &Uuid) {
        if !self.all_guids.contains_key(full_category_name) {
            self.all_guids.insert(full_category_name.clone(), HashSet::default());
        }
        if let Some(all_guid) = self.all_guids.get_mut(full_category_name) {
            all_guid.insert(*uuid);
        } else {
            panic!("Can't register {} {}", full_category_name, uuid);
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
