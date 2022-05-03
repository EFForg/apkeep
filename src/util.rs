use std::error::Error;
use std::fmt;
use std::fs;
use std::path::PathBuf;

#[derive(Debug)]
pub enum ConfigDirError {
    NotFound,
    CouldNotCreate,
}

impl Error for ConfigDirError {}

impl fmt::Display for ConfigDirError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::NotFound => write!(f, "NotFound"),
            Self::CouldNotCreate => write!(f, "CouldNotCreate"),
        }
    }
}

pub fn create_dir(config_dir: &PathBuf) -> Result<(), ConfigDirError> {
    if !config_dir.is_dir() {
        fs::create_dir(config_dir).map_err(|_| { ConfigDirError::CouldNotCreate } )?;
    }
    Ok(())
}

pub fn config_dir() -> Result<PathBuf, ConfigDirError> {
    let mut config_dir = dirs::config_dir().ok_or(ConfigDirError::NotFound)?;
    create_dir(&config_dir)?;
    config_dir.push("apkeep");
    create_dir(&config_dir)?;
    Ok(config_dir)
}
