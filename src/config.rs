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

    Ok(class_finder_home()?.join("db.redb"))
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
    if db_path.exists() {
        std::fs::remove_file(db_path)
            .with_context(|| format!("无法删除 db 文件: {}", db_path.display()))?;
    }
    Ok(())
}

fn class_finder_home() -> Result<PathBuf> {
    let base = dirs::data_local_dir()
        .or_else(dirs::cache_dir)
        .or_else(dirs::home_dir)
        .ok_or_else(|| anyhow::anyhow!("无法确定数据目录"))?;
    Ok(base.join("class-finder"))
}

fn install_cfr_if_missing(target_path: &Path) -> Result<()> {
    if target_path.exists() {
        return Ok(());
    }

    let url = "https://github.com/leibnitz27/cfr/releases/download/0.152/cfr-0.152.jar";
    if let Some(parent) = target_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("无法创建目录: {}", parent.display()))?;
    }

    eprintln!(
        "[class-finder] 未找到 CFR，正在下载到 {}",
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
                .context("cfr.jar 目标路径不是有效 UTF-8")?,
            url,
        ])
        .status()
        .context("执行 curl 失败（请确认系统已安装 curl，或使用 --cfr 指定 cfr.jar）")?;

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

        anyhow::bail!("下载 CFR 失败。可用 --cfr 指定本地 cfr.jar");
    }

    Ok(())
}
