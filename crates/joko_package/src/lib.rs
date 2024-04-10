//! ReadOnly XML marker packs support for Jokolay
//!
//!

pub(crate) mod io;
pub(crate) mod manager;
pub mod message;

pub use manager::{
    PackageDataManager, 
    PackageUIManager, 
    LoadedPackData, 
    LoadedPackTexture, 
    load_all_from_dir, 
    build_from_core, 
    ImportStatus,
    import_pack_from_zip_file_path
};

// for compile time build info like pkg version or build timestamp or git hash etc..
// shadow_rs::shadow!(build);

// to filter the xml with rapidxml first
#[cxx::bridge(namespace = "rapid")]
mod ffi {
    unsafe extern "C++" {
        include!("joko_package/vendor/rapid/rapid.hpp");
        pub fn rapid_filter(src_xml: String) -> String;

    }
}

pub fn rapid_filter_rust(src_xml: String) -> String {
    ffi::rapid_filter(src_xml)
}

pub const INCHES_PER_METER: f32 = 39.37;

pub fn is_default<T: PartialEq + Default>(t: &T) -> bool {
    t == &T::default()
}

pub const BASE64_ENGINE: base64::engine::GeneralPurpose = base64::engine::GeneralPurpose::new(
    &base64::alphabet::STANDARD,
    base64::engine::GeneralPurposeConfig::new(),
);
