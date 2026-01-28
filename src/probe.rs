use anyhow::{Context, Result};
use memmap2::Mmap;
use std::fs::File;
use std::io::Cursor;
use std::path::Path;
use zip::ZipArchive;

pub fn jar_contains_class(jar_path: &Path, class_path: &str) -> Result<bool> {
    let file = File::open(jar_path).with_context(|| format!("无法打开 jar: {}", jar_path.display()))?;
    let mmap = unsafe { Mmap::map(&file).with_context(|| format!("mmap 失败: {}", jar_path.display()))? };
    let mut archive = ZipArchive::new(Cursor::new(&mmap[..]))
        .with_context(|| format!("无法读取 zip 结构: {}", jar_path.display()))?;
    Ok(archive.by_name(class_path).is_ok())
}

pub fn find_class_fqns_in_jar(jar_path: &Path, simple_class_name: &str) -> Result<Vec<String>> {
    let file = File::open(jar_path).with_context(|| format!("无法打开 jar: {}", jar_path.display()))?;
    let mmap = unsafe { Mmap::map(&file).with_context(|| format!("mmap 失败: {}", jar_path.display()))? };
    let mut archive = ZipArchive::new(Cursor::new(&mmap[..]))
        .with_context(|| format!("无法读取 zip 结构: {}", jar_path.display()))?;

    let wanted_suffix = format!("/{simple_class_name}.class");
    let mut results = Vec::new();

    for i in 0..archive.len() {
        let entry = archive.by_index(i)?;
        let name = entry.name();
        if !name.ends_with(".class") {
            continue;
        }
        if name.contains('$') {
            continue;
        }
        if name.ends_with(&wanted_suffix) {
            let fqn = name.trim_end_matches(".class").replace('/', ".");
            results.push(fqn);
        }
    }

    results.sort();
    results.dedup();
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use zip::write::{FileOptions, ZipWriter};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_jar_path() -> PathBuf {
        let mut p = std::env::temp_dir();
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        p.push(format!(
            "class-finder-probe-{}-{}-{}.jar",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis(),
            n
        ));
        p
    }

    #[test]
    fn jar_contains_class_works() {
        let jar_path = temp_jar_path();
        let file = fs::File::create(&jar_path).unwrap();
        let mut zip = ZipWriter::new(file);

        zip.start_file(
            "org/apache/commons/lang3/StringUtils.class",
            FileOptions::default(),
        )
        .unwrap();
        zip.write_all(b"dummy").unwrap();
        zip.finish().unwrap();

        assert!(jar_contains_class(&jar_path, "org/apache/commons/lang3/StringUtils.class").unwrap());
        assert!(!jar_contains_class(&jar_path, "org/apache/commons/lang3/ArrayUtils.class").unwrap());

        let _ = fs::remove_file(&jar_path);
    }

    #[test]
    fn find_class_fqns_in_jar_finds_by_basename() {
        let jar_path = temp_jar_path();
        let file = fs::File::create(&jar_path).unwrap();
        let mut zip = ZipWriter::new(file);

        zip.start_file(
            "org/springframework/stereotype/Component.class",
            FileOptions::default(),
        )
        .unwrap();
        zip.write_all(b"dummy").unwrap();

        zip.start_file(
            "org/springframework/stereotype/Component$Inner.class",
            FileOptions::default(),
        )
        .unwrap();
        zip.write_all(b"dummy").unwrap();

        zip.finish().unwrap();

        let fqns = find_class_fqns_in_jar(&jar_path, "Component").unwrap();
        assert_eq!(fqns, vec!["org.springframework.stereotype.Component"]);

        let _ = fs::remove_file(&jar_path);
    }
}
