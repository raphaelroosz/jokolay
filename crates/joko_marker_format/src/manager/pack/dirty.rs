
use ordered_hash_map::OrderedHashSet;

use crate::pack::RelativePath;

#[derive(Debug, Default, Clone)]
pub(crate) struct Dirty {
    pub all: bool,
    /// whether categories need to be saved
    pub cats: bool,
    /// whether cats selection needs to be saved
    pub cats_selection: bool,
    /// Whether any mapdata needs saving
    pub map_dirty: OrderedHashSet<u32>,
    /// whether any texture needs saving
    pub texture: OrderedHashSet<RelativePath>,
    /// whether any tbin needs saving
    pub tbin: OrderedHashSet<RelativePath>,
}

impl Dirty {
    pub fn is_dirty(&self) -> bool {
        self.cats
            || self.cats_selection
            || !self.map_dirty.is_empty()
            || !self.texture.is_empty()
            || !self.tbin.is_empty()
    }
}