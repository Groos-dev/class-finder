use anyhow::{Context, Result};
use std::env;
use std::path::{Path, PathBuf};

use crate::cli::Cli;
use crate::scan::default_m2_repository;

pub fn resolve_m2_repo(cli: &Cli) -> Result<PathBuf> {
    if let Some(p) = cli.m2.clone() {
        return Ok(p);
    }
    default_m2_repository()
}

pub fn resolve_db_path(cli: &Cli) -> Result<PathBuf> {
    if let Some(p) = cli.db.clone() {
        return Ok(p);
    }

    Ok(class_finder_home()?.join("db.lmdb"))
}

pub fn resolve_snapshot_db_path(cli: &Cli) -> Result<PathBuf> {
    let db_path = resolve_db_path(cli)?;
    Ok(snapshot_db_path(&db_path))
}

pub fn snapshot_db_path(db_path: &Path) -> PathBuf {
    let stem = db_path
        .file_stem()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("db");
    let file_name = format!("{stem}.snapshot.lmdb");

    match db_path.parent() {
        Some(parent) => parent.join(file_name),
        None => PathBuf::from(file_name),
    }
}

pub fn resolve_cfr_path(cli: &Cli) -> Result<PathBuf> {
    if let Some(p) = cli.cfr.clone() {
        return Ok(p);
    }

    if let Ok(p) = env::var("CFR_JAR") {
        return Ok(PathBuf::from(p));
    }

    let default_path = class_finder_home()?.join("tools").join("cfr.jar");
    if default_path.exists() {
        return Ok(default_path);
    }

    install_cfr_if_missing(&default_path)?;
    Ok(default_path)
}

pub fn clear_db(db_path: &Path) -> Result<()> {
    remove_file_if_exists(db_path, "db")?;
    remove_file_if_exists(&lmdb_lock_path(db_path), "db lock")?;

    let snapshot = snapshot_db_path(db_path);
    remove_file_if_exists(&snapshot, "snapshot db")?;
    remove_file_if_exists(&lmdb_lock_path(&snapshot), "snapshot db lock")?;

    Ok(())
}

pub fn publish_snapshot(main_db_path: &Path, snapshot_db_path: &Path) -> Result<()> {
    if !main_db_path.exists() {
        return Ok(());
    }

    if let Some(parent) = snapshot_db_path.parent() {
        std::fs::create_dir_all(parent).with_context(|| {
            format!("Failed to create snapshot directory: {}", parent.display())
        })?;
    }

    let mut tmp_os = snapshot_db_path.as_os_str().to_os_string();
    tmp_os.push(".tmp");
    let tmp = PathBuf::from(tmp_os);
    std::fs::copy(main_db_path, &tmp).with_context(|| {
        format!(
            "Failed to copy snapshot file: {} -> {}",
            main_db_path.display(),
            tmp.display()
        )
    })?;

    if snapshot_db_path.exists() {
        let _ = std::fs::remove_file(snapshot_db_path);
    }
    std::fs::rename(&tmp, snapshot_db_path).with_context(|| {
        format!(
            "Failed to atomically replace snapshot file: {}",
            snapshot_db_path.display()
        )
    })?;
    Ok(())
}

fn class_finder_home() -> Result<PathBuf> {
    let base = dirs::data_local_dir()
        .or_else(dirs::cache_dir)
        .or_else(dirs::home_dir)
        .ok_or_else(|| anyhow::anyhow!("Failed to resolve data directory"))?;
    Ok(base.join("class-finder"))
}

fn lmdb_lock_path(db_path: &Path) -> PathBuf {
    let mut os = db_path.as_os_str().to_os_string();
    os.push("-lock");
    PathBuf::from(os)
}

fn remove_file_if_exists(path: &Path, kind: &str) -> Result<()> {
    if path.exists() {
        std::fs::remove_file(path)
            .with_context(|| format!("Failed to remove {kind} file: {}", path.display()))?;
    }
    Ok(())
}

fn install_cfr_if_missing(target_path: &Path) -> Result<()> {
    if target_path.exists() {
        return Ok(());
    }

    let url = "https://github.com/leibnitz27/cfr/releases/download/0.152/cfr-0.152.jar";
    if let Some(parent) = target_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
    }

    eprintln!(
        "[class-finder] CFR not found, downloading to {}",
        target_path.display()
    );
    let status = std::process::Command::new("curl")
        .args([
            "-L",
            "--fail",
            "--silent",
            "--show-error",
            "-o",
            target_path
                .to_str()
                .context("cfr.jar target path is not valid UTF-8")?,
            url,
        ])
        .status()
        .context(
            "Failed to execute curl (ensure curl is installed, or use --cfr to specify cfr.jar)",
        )?;

    if !status.success() {
        if cfg!(windows) {
            let ps_status = std::process::Command::new("powershell")
                .args([
                    "-NoProfile",
                    "-ExecutionPolicy",
                    "Bypass",
                    "-Command",
                    &format!(
                        "Invoke-WebRequest -Uri '{url}' -OutFile '{}'",
                        target_path.display()
                    ),
                ])
                .status();

            if let Ok(s) = ps_status
                && s.success()
            {
                return Ok(());
            }
        }

        anyhow::bail!("Failed to download CFR. You can use --cfr to specify local cfr.jar");
    }

    Ok(())
}
