use directories::ProjectDirs;
use std::path::PathBuf;

/// Resolved on-disk locations for FaintSignal data.
#[derive(Debug, Clone)]
pub struct FaintPaths {
    pub data_dir: PathBuf,
    pub config_dir: PathBuf,
    /// Presage / libsignal SQLCipher database.
    pub signal_db: PathBuf,
    /// App UI history database.
    pub app_db: PathBuf,
    pub attachments_dir: PathBuf,
    pub logs_dir: PathBuf,
}

/// Resolve XDG-style project directories for FaintSignal.
pub fn paths() -> anyhow::Result<FaintPaths> {
    let dirs = ProjectDirs::from("dev", "faintsignal", "FaintSignal")
        .ok_or_else(|| anyhow::anyhow!("cannot resolve project directories"))?;
    let data_dir = dirs.data_dir().to_path_buf();
    let config_dir = dirs.config_dir().to_path_buf();
    Ok(FaintPaths {
        signal_db: data_dir.join("signal.db3"),
        app_db: data_dir.join("app.json"),
        attachments_dir: data_dir.join("attachments"),
        logs_dir: data_dir.join("logs"),
        data_dir,
        config_dir,
    })
}

impl FaintPaths {
    pub fn ensure_dirs(&self) -> anyhow::Result<()> {
        std::fs::create_dir_all(&self.data_dir)?;
        std::fs::create_dir_all(&self.config_dir)?;
        std::fs::create_dir_all(&self.attachments_dir)?;
        std::fs::create_dir_all(&self.logs_dir)?;
        Ok(())
    }
}
