use std::{
    io::Read,
};
use tracing::{info};

use miette::{IntoDiagnostic, Result};
use crate::pack::PackCore;


#[derive(Debug, Default)]
pub enum ImportStatus {
    #[default]
    UnInitialized,
    WaitingForFileChooser,
    LoadingPack(std::path::PathBuf),
    PackDone(String, PackCore, bool),
    PackError(miette::Report),
}

pub fn import_pack_from_zip_file_path(file_path: std::path::PathBuf) -> Result<(String, PackCore)> {
    let mut taco_zip = vec![];
    std::fs::File::open(&file_path)
        .into_diagnostic()?
        .read_to_end(&mut taco_zip)
        .into_diagnostic()?;

    info!("starting to get pack from taco");
    crate::io::get_pack_from_taco_zip(&taco_zip).map(|pack| {
        (
            file_path
                .file_name()
                .map(|ostr| ostr.to_string_lossy().to_string())
                .unwrap_or_default(),
            pack,
        )
    })
}