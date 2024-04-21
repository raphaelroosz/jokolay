use joko_package_models::package::PackCore;
use tracing::info;

use miette::Result;

#[derive(Debug, Default)]
pub enum ImportStatus {
    #[default]
    UnInitialized,
    WaitingForFileChooser,
    LoadingPack(std::path::PathBuf),
    WaitingLoading(std::path::PathBuf),
    PackDone(String, PackCore, bool),
    PackError(miette::Report),
}

pub fn import_pack_from_zip_file_path(
    file_path: std::path::PathBuf,
    extract_temporary_path: &std::path::PathBuf,
) -> Result<(String, PackCore)> {
    info!("starting to get pack from taco");
    crate::io::get_pack_from_taco_zip(file_path.clone(), extract_temporary_path).map(|pack| {
        (
            file_path
                .file_name()
                .map(|ostr| ostr.to_string_lossy().to_string())
                .unwrap_or_default(),
            pack,
        )
    })
}
