use anyhow::{Context, Result};
use memmap2::Mmap;
use redb::Database;
use std::fs::File;
use std::io::Cursor;
use std::path::Path;
use zip::ZipArchive;

use crate::cache::ARTIFACT_MANIFEST_TABLE;

pub fn catalog(artifact_path: &Path) -> Result<Vec<String>> {
    let file = File::open(artifact_path)
        .with_context(|| format!("无法打开 jar: {}", artifact_path.display()))?;
    // SAFETY: The file is opened read-only and remains valid for the lifetime of the mmap.
    // The mmap is dropped before the file, ensuring memory safety.
    let mmap = unsafe { Mmap::map(&file) }
        .with_context(|| format!("mmap jar 失败: {}", artifact_path.display()))?;
    let mut archive = ZipArchive::new(Cursor::new(&mmap[..]))
        .with_context(|| format!("无法解析 zip(jar): {}", artifact_path.display()))?;

    let mut classes = Vec::new();
    for i in 0..archive.len() {
        let entry = archive.by_index(i)?;
        let name = entry.name();
        if !name.ends_with(".class") {
            continue;
        }
        if name.contains('$') {
            continue;
        }
        let class_name = name.trim_end_matches(".class").replace(['/', '\\'], ".");
        classes.push(class_name);
    }
    Ok(classes)
}

pub fn is_cataloged(db: &Database, jar_key: &str) -> Result<bool> {
    let txn = db.begin_read()?;
    let table = txn.open_table(ARTIFACT_MANIFEST_TABLE)?;
    Ok(table.get(jar_key)?.is_some())
}

pub fn mark_cataloged(db: &Database, jar_key: &str) -> Result<()> {
    let txn = db.begin_write()?;
    {
        let mut table = txn.open_table(ARTIFACT_MANIFEST_TABLE)?;
        table.insert(jar_key, "1")?;
    }
    txn.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::time::{SystemTime, UNIX_EPOCH};
    use zip::write::FileOptions;

    fn temp_path(name: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "class_finder_test_{}_{}_{}",
            std::process::id(),
            nanos,
            name
        ))
    }

    fn write_jar(path: &std::path::Path, entries: &[(&str, &[u8])]) -> Result<()> {
        let file = std::fs::File::create(path)?;
        let mut zip = zip::ZipWriter::new(file);
        let options = FileOptions::default().compression_method(zip::CompressionMethod::Deflated);

        for (name, content) in entries {
            zip.start_file(*name, options)?;
            zip.write_all(content)?;
        }

        zip.finish()?;
        Ok(())
    }

    #[test]
    fn catalog_extracts_top_level_classes() -> Result<()> {
        let jar = temp_path("catalog_ok.jar");
        write_jar(
            &jar,
            &[
                ("org/example/A.class", b""),
                ("org/example/A$Inner.class", b""),
                ("META-INF/MANIFEST.MF", b""),
            ],
        )?;

        let classes = catalog(&jar)?;
        assert!(classes.contains(&"org.example.A".to_string()));
        assert!(!classes.iter().any(|c| c.contains('$')));
        std::fs::remove_file(jar)?;
        Ok(())
    }

    #[test]
    fn catalog_handles_empty_jar() -> Result<()> {
        let jar = temp_path("catalog_empty.jar");
        write_jar(&jar, &[])?;
        let classes = catalog(&jar)?;
        assert!(classes.is_empty());
        std::fs::remove_file(jar)?;
        Ok(())
    }
}
