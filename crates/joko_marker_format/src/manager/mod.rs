//! How should the pack be stored by jokolay?
//! 1. Inside a directory called packs, we will have a separate directory for each pack.
//! 2. the name of the directory will serve as an ID for each pack.
//! 3. Inside the directory, we will have
//!     1. categories.xml -> The xml file which contains the whole category tree
//!     2. $mapid.xml -> where the $mapid is the id (u16) of a map which contains markers/trails belonging to that particular map.
//!     3. **/{.png | .trl} -> Any number of png images or trl binaries, in any location within this pack directory.

/*
expensive:
categories being a tree with order among siblings (better to use a tree crate?)
markers/trails referring to a category via full path.
editing a category's name/path means that you have to load all the maps that refer to the category and change the reference.

We will make not having a valid category/texture/tbin path as allowed. So, users can deal with the headache themselves.

*/

mod marker;
mod pack;
mod file;

pub use marker::MarkerManager;
pub use file::FileManager;

