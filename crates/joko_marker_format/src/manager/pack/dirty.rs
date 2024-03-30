
use ordered_hash_map::OrderedHashSet;

use crate::pack::RelativePath;

#[derive(Debug, Default, Clone)]
pub(crate) struct DirtyMarker {
    pub all: bool,
    /// whether categories need to be saved
    pub categories: bool,
    /// whether selected categories  needs to be saved
    pub selected_categories: bool,
    /// Whether any mapdata needs saving
    pub map: OrderedHashSet<u32>,
    /// whether any texture needs saving
    pub texture: OrderedHashSet<RelativePath>,
    /// whether any tbin needs saving
    pub tbin: OrderedHashSet<RelativePath>,
}

impl DirtyMarker {
    pub fn is_dirty(&self) -> bool {
        self.categories
            || self.selected_categories
            || !self.map.is_empty()
            || !self.texture.is_empty()
            || !self.tbin.is_empty()
    }
}