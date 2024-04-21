use cap_std::{ambient_authority, fs_utf8::camino::Utf8PathBuf, fs_utf8::Dir};
use miette::{Context, IntoDiagnostic, Result};

/// Jokolay Configuration
/// We will read a path from env `JOKOLAY_DATA_DIR` or create a folder at data_local_dir/jokolay, where data_local_dir is platform specific
/// Inside this directory, we will store all of jokolay's data like configuration files, themes, logs etc..

//TODO: isn't directories-next better for introspection ?
pub fn get_jokolay_path() -> Result<std::path::PathBuf> {
    if let Some(project_dir) = directories_next::ProjectDirs::from("com.jokolay", "", "jokolay") {
        Ok(project_dir.data_local_dir().to_path_buf())
    } else {
        Err(miette::miette!(
            "getting project path failed for some reason"
        ))
    }
}

pub fn get_jokolay_dir() -> Result<cap_std::fs_utf8::Dir> {
    let authoratah = ambient_authority();
    let jdir = if let Ok(env_dir) = std::env::var("JOKOLAY_DATA_DIR") {
        let jkl_path = Utf8PathBuf::from(&env_dir); //may still be an invalid path

        cap_std::fs_utf8::Dir::create_ambient_dir_all(&jkl_path, authoratah)
            .into_diagnostic()
            .wrap_err(jkl_path.clone())
            .wrap_err("failed to create jokolay directory")?;
        Dir::open_ambient_dir(&jkl_path, authoratah)
            .into_diagnostic()
            .wrap_err(jkl_path)
            .wrap_err("failed to open jokolay data dir")?
    } else {
        let project_dir =
            cap_directories::ProjectDirs::from("com.jokolay", "", "jokolay", authoratah);
        let dir = project_dir
            .ok_or(miette::miette!(
                "getting project dirs failed for some reason"
            ))?
            .data_local_dir()
            .into_diagnostic()
            .wrap_err("failed ot get data local dir using capstd")?;
        Dir::from_cap_std(dir) // into utf-8 dir
    };
    Ok(jdir)
}
