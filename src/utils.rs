use anyhow::{anyhow, Result};
use std::env;
use std::path::PathBuf;

pub fn home_dir() -> Result<PathBuf> {
    #[cfg(windows)]
    {
        if let Ok(u) = env::var("USERPROFILE") {
            return Ok(PathBuf::from(u));
        }
        let drive = env::var("HOMEDRIVE").unwrap_or_default();
        let path = env::var("HOMEPATH").unwrap_or_default();
        if !drive.is_empty() && !path.is_empty() {
            return Ok(PathBuf::from(format!("{drive}{path}")));
        }
        Err(anyhow!("no home dir"))
    }
    #[cfg(not(windows))]
    {
        env::var("HOME")
            .map(PathBuf::from)
            .map_err(|_| anyhow!("no $HOME"))
    }
}
