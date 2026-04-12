use anyhow::{Context, Result};
use memmap2::Mmap;
use std::collections::HashSet;
use std::fs::File;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use zip::ZipArchive;

use crate::parse::{ParsedClass, extract_class_name, hash_content};

pub fn sources_jar_path(jar_path: &Path) -> Option<PathBuf> {
    if jar_path.extension().is_none_or(|ext| ext != "jar") {
        return None;
    }

    if jar_path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.ends_with("-sources.jar"))
    {
        return None;
    }

    let stem = jar_path.file_stem()?.to_str()?;
    Some(jar_path.with_file_name(format!("{stem}-sources.jar")))
}

pub fn find_sources_jar(jar_path: &Path) -> Option<PathBuf> {
    sources_jar_path(jar_path).filter(|path| path.exists())
}

pub fn read_class_source(jar_path: &Path, class_name: &str) -> Result<Option<String>> {
    let Some(sources_jar) = find_sources_jar(jar_path) else {
        return Ok(None);
    };

    let file = File::open(&sources_jar)
        .with_context(|| format!("Failed to open sources jar: {}", sources_jar.display()))?;
    // SAFETY: The file is opened read-only and remains valid for the lifetime of the mmap.
    let mmap = unsafe { Mmap::map(&file) }
        .with_context(|| format!("mmap sources jar failed: {}", sources_jar.display()))?;
    let mut archive = ZipArchive::new(Cursor::new(&mmap[..]))
        .with_context(|| format!("Failed to read sources jar: {}", sources_jar.display()))?;

    let entry_path = source_entry_path(class_name);
    match archive.by_name(&entry_path) {
        Ok(mut entry) => read_entry_to_string(&mut entry).map(Some),
        Err(_) => Ok(None),
    }
}

pub fn read_jar_sources(jar_path: &Path) -> Result<Vec<ParsedClass>> {
    let Some(sources_jar) = find_sources_jar(jar_path) else {
        return Ok(Vec::new());
    };
    read_sources_jar(&sources_jar)
}

pub fn read_sources_jar(sources_jar: &Path) -> Result<Vec<ParsedClass>> {
    let file = File::open(sources_jar)
        .with_context(|| format!("Failed to open sources jar: {}", sources_jar.display()))?;
    // SAFETY: The file is opened read-only and remains valid for the lifetime of the mmap.
    let mmap = unsafe { Mmap::map(&file) }
        .with_context(|| format!("mmap sources jar failed: {}", sources_jar.display()))?;
    let mut archive = ZipArchive::new(Cursor::new(&mmap[..]))
        .with_context(|| format!("Failed to read sources jar: {}", sources_jar.display()))?;

    let mut classes = Vec::new();
    let mut seen = HashSet::new();
    for idx in 0..archive.len() {
        let mut entry = archive.by_index(idx)?;
        let name = entry.name().to_string();
        if !name.ends_with(".java") || is_special_java_entry(&name) {
            continue;
        }

        let content = read_entry_to_string(&mut entry)?;
        let Some(class_name) =
            extract_class_name(&content).or_else(|| fqn_from_source_entry(&name))
        else {
            continue;
        };
        if !seen.insert(class_name.clone()) {
            continue;
        }

        let content_hash = hash_content(&content);
        classes.push(ParsedClass {
            class_name,
            content,
            content_hash,
        });
    }

    Ok(classes)
}

fn source_entry_path(class_name: &str) -> String {
    let top_level = class_name.split('$').next().unwrap_or(class_name);
    format!("{}.java", top_level.replace('.', "/"))
}

fn fqn_from_source_entry(entry_name: &str) -> Option<String> {
    let class_path = entry_name.strip_suffix(".java")?;
    if is_special_java_entry(entry_name) {
        return None;
    }
    Some(class_path.replace(['/', '\\'], "."))
}

fn is_special_java_entry(entry_name: &str) -> bool {
    entry_name.ends_with("package-info.java") || entry_name.ends_with("module-info.java")
}

fn read_entry_to_string(entry: &mut zip::read::ZipFile<'_>) -> Result<String> {
    let mut bytes = Vec::new();
    entry.read_to_end(&mut bytes)?;
    Ok(String::from_utf8_lossy(&bytes).replace("\r\n", "\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::time::{SystemTime, UNIX_EPOCH};
    use zip::write::FileOptions;

    fn temp_path(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "class_finder_sources_test_{}_{}_{}",
            std::process::id(),
            nanos,
            name
        ))
    }

    fn write_zip(path: &Path, entries: &[(&str, &str)]) -> Result<()> {
        let file = File::create(path)?;
        let mut zip = zip::ZipWriter::new(file);
        let options = FileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for (name, content) in entries {
            zip.start_file(*name, options)?;
            zip.write_all(content.as_bytes())?;
        }
        zip.finish()?;
        Ok(())
    }

    #[test]
    fn reads_source_from_sibling_sources_jar() -> Result<()> {
        let jar = temp_path("demo-1.0.jar");
        std::fs::write(&jar, b"not a real jar")?;
        let sources = sources_jar_path(&jar).unwrap();
        write_zip(
            &sources,
            &[(
                "org/example/A.java",
                "package org.example;\npublic class A {}\n",
            )],
        )?;

        let content = read_class_source(&jar, "org.example.A")?.unwrap();
        assert!(content.contains("public class A"));
        assert_eq!(read_jar_sources(&jar)?.len(), 1);

        let _ = std::fs::remove_file(jar);
        let _ = std::fs::remove_file(sources);
        Ok(())
    }
}
