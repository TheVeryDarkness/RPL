use std::path::{Path, PathBuf};
use std::{env, fs};

use crate::{ConfigError, RplConfig};

pub(crate) fn resolve_base_dir(manifest_path: Option<&Path>) -> Result<PathBuf, ConfigError> {
    let base = if let Some(manifest_path) = manifest_path {
        let manifest_path = if manifest_path.is_absolute() {
            manifest_path.to_path_buf()
        } else {
            env::current_dir()
                .map_err(|source| ConfigError::Io {
                    path: PathBuf::from("."),
                    source: Box::new(source),
                })?
                .join(manifest_path)
        };
        manifest_path.parent().unwrap_or(Path::new(".")).to_path_buf()
    } else {
        env::current_dir().map_err(|source| ConfigError::Io {
            path: PathBuf::from("."),
            source: Box::new(source),
        })?
    };

    Ok(base)
}

pub(crate) fn read_config(path: &Path) -> Result<RplConfig, ConfigError> {
    let contents = fs::read_to_string(path).map_err(|source| ConfigError::Io {
        path: path.to_path_buf(),
        source: Box::new(source),
    })?;
    toml::from_str(&contents).map_err(|source| ConfigError::Toml {
        path: path.to_path_buf(),
        source: Box::new(source),
    })
}
