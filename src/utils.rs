use anyhow::{anyhow, Result};
use std::env;
use std::path::{Path, PathBuf};
use std::fs;

pub fn secure_create_dir_all(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        let mut builder = fs::DirBuilder::new();
        builder.recursive(true).mode(0o700);
        builder.create(path)?;
    }
    #[cfg(not(unix))]
    {
        fs::create_dir_all(path)?;
    }
    Ok(())
}

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
