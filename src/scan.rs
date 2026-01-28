use anyhow::Result;
use ignore::WalkBuilder;
use std::path::{Path, PathBuf};
use std::sync::mpsc;

pub fn default_m2_repository() -> Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("无法获取 home 目录"))?;
    Ok(home.join(".m2").join("repository"))
}

pub fn infer_scan_path(m2_repo: &Path, class_name: &str) -> PathBuf {
    let parts: Vec<&str> = class_name.split('.').collect();
    if parts.len() < 3 {
        return m2_repo.to_path_buf();
    }

    for i in (2..parts.len().saturating_sub(1)).rev() {
        let prefix = parts[..i].join("/");
        let path = m2_repo.join(prefix);
        if path.exists() {
            return path;
        }
    }

    m2_repo.to_path_buf()
}

pub fn scan_jars(base_path: &Path) -> Result<Vec<PathBuf>> {
    let (tx, rx) = mpsc::channel();

    let walker = WalkBuilder::new(base_path)
        .hidden(false)
        .git_ignore(false)
        .git_global(false)
        .git_exclude(false)
        .build_parallel();

    walker.run(|| {
        let tx = tx.clone();
        Box::new(move |entry| {
            if let Ok(entry) = entry {
                let path = entry.path();
                if path.extension().is_some_and(|e| e == "jar") {
                    let _ = tx.send(path.to_path_buf());
                }
            }
            ignore::WalkState::Continue
        })
    });

    drop(tx);
    Ok(rx.iter().collect())
}

pub fn class_name_to_class_path(class_name: &str) -> String {
    format!("{}.class", class_name.replace('.', "/"))
}

pub fn extract_version_from_maven_path(jar_path: &Path) -> Option<String> {
    jar_path
        .parent()
        .and_then(|p| p.file_name())
        .map(|s| s.to_string_lossy().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_dir(prefix: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "{prefix}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis()
        ));
        p
    }

    #[test]
    fn infer_scan_path_picks_existing_prefix() {
        let base = temp_dir("class-finder-scan");
        let m2 = base.join("repository");
        let target = m2.join("org/apache/commons");
        fs::create_dir_all(&target).unwrap();

        let inferred = infer_scan_path(&m2, "org.apache.commons.lang3.StringUtils");
        assert_eq!(inferred, target);
    }
}
