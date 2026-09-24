use anyhow::{anyhow, Result};
use std::env;
use std::path::{Path, PathBuf};

/// Creates a directory and all its parent components, setting strict permissions
/// on Unix systems to prevent unauthorized access.
pub fn secure_create_dir_all<P: AsRef<Path>>(path: P) -> Result<()> {
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);

    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }

    builder.create(path)?;
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
