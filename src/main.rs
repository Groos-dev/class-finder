use anyhow::{Context, Result};
use clap::Parser;
use class_finder::buffer::{BufferConfig, PendingWrite, WriteBuffer};
use class_finder::cache::{PersistentCache, ReadOnlyCache};
use class_finder::catalog;
use class_finder::cfr::Cfr;
use class_finder::cli::{Cli, Commands, OutputFormat};
use class_finder::config::{clear_db, resolve_cfr_path, resolve_db_path, resolve_m2_repo};
use class_finder::hotspot::HotspotTracker;
use class_finder::parse::{hash_content, parse_decompiled_output};
use class_finder::probe::{find_class_fqns_in_jar, jar_contains_class};
use class_finder::registry::ClassRegistry;
use class_finder::scan::{
    class_name_to_class_path, extract_version_from_maven_path, infer_scan_path, infer_search_paths,
    scan_jars,
};
use class_finder::structure::{ClassStructure, parse_class_structure};
use rayon::prelude::*;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Instant;

fn main() -> Result<()> {
    let cli = parse_cli()?;

    match cli.command.clone() {
        Commands::Clear => {
            let db_path = resolve_db_path(&cli)?;
            clear_db(&db_path)?;
        }
        Commands::Index { path } => {
            let db_path = resolve_db_path(&cli)?;
            let output = {
                let cache = PersistentCache::open(db_path.clone())?;
                let registry = ClassRegistry::new(cache.db());
                let root = path.unwrap_or(resolve_m2_repo(&cli)?);
                index_repo(&registry, root)?
            };
            println!("{}", serde_json::to_string_pretty(&output)?);
        }
        Commands::Stats => {
            let db_path = resolve_db_path(&cli)?;
            let cache = ReadOnlyCache::open(db_path)?;
            let stats = cache.stats()?;
            println!("{}", serde_json::to_string_pretty(&stats)?);
        }
        Commands::Load { jar_path } => {
            let cfr = Cfr::new(resolve_cfr_path(&cli)?);
            let db_path = resolve_db_path(&cli)?;
            let output = {
                let cache = PersistentCache::open(db_path.clone())?;
                let registry = ClassRegistry::new(cache.db());
                let hotspot = HotspotTracker::new(cache.db(), 2);
                let mut buffer = WriteBuffer::new(
                    cache.db(),
                    BufferConfig::default(),
                    cache.pending_gauge_path(),
                );
                let output = load_jar(&cache, &registry, &buffer, &cfr, &jar_path)?;
                buffer.shutdown_and_flush()?;
                if !output.skipped {
                    cache.mark_jar_loaded(&output.jar_path)?;
                    let _ = hotspot.mark_warmed(&output.jar_path, output.classes_loaded as u32);
                }
                output
            };
            println!("{}", serde_json::to_string_pretty(&output)?);
        }
        Commands::Warmup {
            jar_path,
            hot,
            group,
            top,
            limit,
        } => {
            let cfr = Cfr::new(resolve_cfr_path(&cli)?);
            let db_path = resolve_db_path(&cli)?;
            let output = {
                let cache = PersistentCache::open(db_path.clone())?;
                let registry = ClassRegistry::new(cache.db());
                let hotspot = HotspotTracker::new(cache.db(), 2);
                let mut buffer = WriteBuffer::new(
                    cache.db(),
                    BufferConfig::default(),
                    cache.pending_gauge_path(),
                );
                let m2_repo = resolve_m2_repo(&cli)?;
                let deps = WarmupDeps {
                    cache: &cache,
                    registry: &registry,
                    hotspot: &hotspot,
                    buffer: &buffer,
                    cfr: &cfr,
                    m2_repo: &m2_repo,
                };
                let params = WarmupParams {
                    jar_path: jar_path.as_deref(),
                    hot,
                    group: group.as_deref(),
                    top,
                    limit,
                };
                let output = warmup_targets(&deps, params)?;
                buffer.shutdown_and_flush()?;
                for (jar_key, class_count) in &output.loaded_jars {
                    cache.mark_jar_loaded(jar_key)?;
                    let _ = hotspot.mark_warmed(jar_key, *class_count);
                }
                output
            };
            println!("{}", serde_json::to_string_pretty(&output)?);
        }
        Commands::Find {
            class_name,
            format,
            code_only,
            version,
            output,
        } => {
            let cfr = Cfr::new(resolve_cfr_path(&cli)?);
            let db_path = resolve_db_path(&cli)?;
            let cache = PersistentCache::open(db_path)?;
            let registry = ClassRegistry::new(cache.db());
            let effective_format = if code_only {
                OutputFormat::Code
            } else {
                format
            };
            let class_name = normalize_class_name(&class_name);
            let m2_repo = resolve_m2_repo(&cli)?;
            let deps = FindDeps {
                cache: &cache,
                registry: &registry,
                cfr: &cfr,
                m2_repo: &m2_repo,
            };
            let result = find_class(&deps, &class_name, version)?;
            write_find_output(&result, effective_format, output.as_deref())?;
            backfill_find_cache(&cache, &registry, &cfr, &result);
        }
    }

    Ok(())
}

fn parse_cli() -> Result<Cli> {
    let args: Vec<String> = std::env::args().collect();
    Ok(Cli::parse_from(rewrite_args_for_implicit_find(args)))
}

fn rewrite_args_for_implicit_find(mut args: Vec<String>) -> Vec<String> {
    if args.len() <= 1 {
        return args;
    }

    let subcommands = ["find", "load", "warmup", "index", "stats", "clear", "help"];

    let mut idx = 1usize;
    while idx < args.len() {
        let a = args[idx].as_str();
        if a == "--" {
            idx += 1;
            break;
        }

        if a == "--m2" || a == "--cfr" || a == "--db" {
            idx += 2;
            continue;
        }

        if a.starts_with("--m2=") || a.starts_with("--cfr=") || a.starts_with("--db=") {
            idx += 1;
            continue;
        }

        if a.starts_with('-') {
            idx += 1;
            continue;
        }

        break;
    }

    if idx < args.len() {
        let token = args[idx].as_str();
        if !subcommands.contains(&token) {
            args.insert(idx, "find".to_string());
        }
    }

    args
}

fn normalize_class_name(raw: &str) -> String {
    let mut s = raw.trim();
    if let Some(rest) = s.strip_prefix("import") {
        s = rest.trim();
    }
    if s.ends_with(';') {
        s = s.trim_end_matches(';').trim();
    }
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

#[derive(Debug, Serialize)]
struct FindVersion {
    version: Option<String>,
    jar_path: String,
    content_hash: String,
    content: String,
    cache_hit: bool,
    source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    structure: Option<ClassStructure>,
}

#[derive(Debug, Serialize)]
struct FindResult {
    class_name: String,
    scanned_root: String,
    matched_jars: usize,
    duration_ms: u64,
    versions: Vec<FindVersion>,
}

#[derive(Debug, Serialize)]
struct LoadResult {
    jar_path: String,
    classes_loaded: usize,
    skipped: bool,
    duration_ms: u64,
}

#[derive(Debug, Serialize)]
struct WarmupResult {
    targets: usize,
    succeeded: usize,
    failed: usize,
    duration_ms: u64,
    loads: Vec<LoadResult>,
    loaded_jars: Vec<(String, u32)>,
}

#[derive(Debug, Serialize)]
struct IndexResult {
    root: String,
    scanned_jars: usize,
    cataloged_jars_new: usize,
    indexed_classes: usize,
    duration_ms: u64,
    failed_jars: usize,
}

struct FindDeps<'a> {
    cache: &'a PersistentCache,
    registry: &'a ClassRegistry,
    cfr: &'a Cfr,
    m2_repo: &'a Path,
}

fn find_class(
    deps: &FindDeps<'_>,
    class_name: &str,
    version_filter: Option<String>,
) -> Result<FindResult> {
    let start = Instant::now();
    let m2_repo = deps.m2_repo;
    let (resolved_class_name, mut matched, scan_root, miss_source) = if class_name.contains('.') {
        let search_paths = infer_search_paths(m2_repo, class_name);
        let scan_root = search_paths
            .first()
            .cloned()
            .unwrap_or_else(|| infer_scan_path(m2_repo, class_name));
        let class_path = class_name_to_class_path(class_name);
        let mut registry_hits: Vec<PathBuf> = deps
            .registry
            .get_artifacts(class_name)?
            .into_iter()
            .map(PathBuf::from)
            .filter(|p| p.exists())
            .collect();

        if let Some(v) = version_filter.clone() {
            registry_hits
                .retain(|p| extract_version_from_maven_path(p).as_deref() == Some(v.as_str()));
        }

        registry_hits.retain(|jar| jar_contains_class(jar, &class_path).unwrap_or(false));

        if !registry_hits.is_empty() {
            (
                class_name.to_string(),
                registry_hits,
                scan_root,
                "registry".to_string(),
            )
        } else {
            let mut matched: Vec<PathBuf> = Vec::new();
            let mut used_scan_root = scan_root.clone();

            for candidate_root in search_paths.iter() {
                eprintln!(
                    "[class-finder] find scan root: {}",
                    candidate_root.display()
                );
                let jars = scan_jars(candidate_root)?;
                matched = jars
                    .par_iter()
                    .filter_map(|jar| match jar_contains_class(jar, &class_path) {
                        Ok(true) => Some(jar.clone()),
                        Ok(false) => None,
                        Err(_) => None,
                    })
                    .collect();
                if !matched.is_empty() {
                    used_scan_root = candidate_root.clone();
                    break;
                }
            }

            if matched.is_empty() && scan_root.as_path() != m2_repo {
                eprintln!(
                    "[class-finder] find fallback scan root: {}",
                    m2_repo.display()
                );
                let jars = scan_jars(m2_repo)?;
                matched = jars
                    .par_iter()
                    .filter_map(|jar| match jar_contains_class(jar, &class_path) {
                        Ok(true) => Some(jar.clone()),
                        Ok(false) => None,
                        Err(_) => None,
                    })
                    .collect();
                used_scan_root = m2_repo.to_path_buf();
            }

            (
                class_name.to_string(),
                matched,
                used_scan_root,
                "scan".to_string(),
            )
        }
    } else {
        let scan_root = m2_repo.to_path_buf();
        let jars = scan_jars(&scan_root)?;

        let mut fqn_to_jars: HashMap<String, Vec<PathBuf>> = HashMap::new();
        for jar in jars.iter() {
            let fqns = find_class_fqns_in_jar(jar, class_name).unwrap_or_default();
            for fqn in fqns.into_iter().take(1) {
                fqn_to_jars.entry(fqn).or_default().push(jar.clone());
            }
        }

        let (best_fqn, best_jars) = fqn_to_jars
            .into_iter()
            .max_by(|(a_name, a_jars), (b_name, b_jars)| {
                a_jars
                    .len()
                    .cmp(&b_jars.len())
                    .then_with(|| a_name.cmp(b_name))
            })
            .with_context(|| {
                format!(
                    "Class {class_name} not found (scan dir: {})",
                    scan_root.display()
                )
            })?;

        (best_fqn, best_jars, scan_root, "scan".to_string())
    };

    if let Some(v) = version_filter.clone() {
        matched.retain(|p| extract_version_from_maven_path(p).as_deref() == Some(v.as_str()));
    }

    matched.sort_by(|a, b| {
        extract_version_from_maven_path(a).cmp(&extract_version_from_maven_path(b))
    });

    if matched.is_empty() {
        anyhow::bail!(
            "Class {resolved_class_name} not found (scan dir: {})",
            scan_root.display()
        );
    }

    let mut versions = Vec::new();

    for jar_path in matched.iter() {
        let jar_key = jar_path.to_string_lossy().to_string();
        let cache_key = format!("{resolved_class_name}::{jar_key}");

        if let Some(content) = deps.cache.get_class_source(&cache_key)? {
            versions.push(FindVersion {
                version: extract_version_from_maven_path(jar_path),
                jar_path: jar_key,
                content_hash: hash_content(&content),
                content,
                cache_hit: true,
                source: "cache".to_string(),
                structure: None,
            });
            continue;
        }

        let decompiled = deps.cfr.decompile_class(jar_path, &resolved_class_name)?;
        let parsed = parse_decompiled_output(&decompiled);
        let content = parsed
            .iter()
            .find(|c| c.class_name == resolved_class_name)
            .map(|c| c.content.clone())
            .unwrap_or(decompiled);
        let content_hash = hash_content(&content);
        versions.push(FindVersion {
            version: extract_version_from_maven_path(jar_path),
            jar_path: jar_key,
            content_hash,
            content,
            cache_hit: false,
            source: miss_source.clone(),
            structure: None,
        });
    }

    Ok(FindResult {
        class_name: resolved_class_name,
        scanned_root: scan_root.to_string_lossy().to_string(),
        matched_jars: matched.len(),
        duration_ms: start.elapsed().as_millis() as u64,
        versions,
    })
}

fn backfill_find_cache(
    cache: &PersistentCache,
    registry: &ClassRegistry,
    cfr: &Cfr,
    result: &FindResult,
) {
    let mut target_jars = Vec::new();
    let mut seen = HashSet::new();

    for version in &result.versions {
        if version.cache_hit {
            continue;
        }
        if seen.insert(version.jar_path.clone()) {
            target_jars.push(PathBuf::from(&version.jar_path));
        }
    }

    if target_jars.is_empty() {
        return;
    }

    let mut buffer = WriteBuffer::new(
        cache.db(),
        BufferConfig::default(),
        cache.pending_gauge_path(),
    );
    let hotspot = HotspotTracker::new(cache.db(), 2);

    for jar_path in target_jars {
        eprintln!(
            "[class-finder] find backfill enqueue jar: {}",
            jar_path.display()
        );
        match load_jar(cache, registry, &buffer, cfr, &jar_path) {
            Ok(output) => {
                if !output.skipped {
                    if let Err(err) = cache.mark_jar_loaded(&output.jar_path) {
                        eprintln!(
                            "[class-finder] find backfill mark loaded failed: {} ({err})",
                            output.jar_path
                        );
                    }
                    let _ = hotspot.mark_warmed(&output.jar_path, output.classes_loaded as u32);
                }
            }
            Err(err) => eprintln!(
                "[class-finder] find backfill failed for {}: {err}",
                jar_path.display()
            ),
        }
    }

    if let Err(err) = buffer.shutdown_and_flush() {
        eprintln!("[class-finder] find backfill flush failed: {err}");
    }
}

fn load_jar(
    cache: &PersistentCache,
    registry: &ClassRegistry,
    buffer: &WriteBuffer,
    cfr: &Cfr,
    jar_path: &Path,
) -> Result<LoadResult> {
    let jar_key = jar_path.to_string_lossy().to_string();
    let start = Instant::now();

    if !registry.is_cataloged(&jar_key).unwrap_or(false)
        && let Ok(classes) = catalog::catalog(jar_path)
    {
        let _ = registry.update_registry_and_mark_cataloged(&jar_key, &classes);
    }

    if cache.is_jar_loaded(&jar_key)? {
        return Ok(LoadResult {
            jar_path: jar_key,
            classes_loaded: 0,
            skipped: true,
            duration_ms: 0,
        });
    }

    let decompiled = cfr.decompile_jar(jar_path)?;
    let classes = parse_decompiled_output(&decompiled);
    let classes_loaded = classes.len();

    for cls in classes {
        let key = format!("{}::{jar_key}", cls.class_name);
        let _ = buffer.enqueue(PendingWrite {
            key,
            source: cls.content,
        });
    }

    Ok(LoadResult {
        jar_path: jar_key,
        classes_loaded,
        skipped: false,
        duration_ms: start.elapsed().as_millis() as u64,
    })
}

struct WarmupDeps<'a> {
    cache: &'a PersistentCache,
    registry: &'a ClassRegistry,
    hotspot: &'a HotspotTracker,
    buffer: &'a WriteBuffer,
    cfr: &'a Cfr,
    m2_repo: &'a Path,
}

struct WarmupParams<'a> {
    jar_path: Option<&'a Path>,
    hot: bool,
    group: Option<&'a str>,
    top: usize,
    limit: Option<usize>,
}

fn warmup_targets(deps: &WarmupDeps<'_>, params: WarmupParams<'_>) -> Result<WarmupResult> {
    let start = Instant::now();
    let mut targets: Vec<PathBuf> = if params.hot {
        deps.hotspot
            .top_unwarmed_jars(params.top)?
            .into_iter()
            .map(PathBuf::from)
            .collect()
    } else if let Some(group) = params.group {
        let dir = deps.m2_repo.join(group.replace('.', "/"));
        if dir.exists() {
            scan_jars(&dir)?
        } else {
            Vec::new()
        }
    } else if let Some(jar_path) = params.jar_path {
        vec![jar_path.to_path_buf()]
    } else {
        anyhow::bail!("warmup requires jar_path, or use --hot / --group");
    };

    if let Some(limit) = params.limit {
        targets.truncate(limit);
    }

    let mut loads = Vec::new();
    let mut loaded_jars: Vec<(String, u32)> = Vec::new();
    let mut succeeded = 0usize;
    let mut failed = 0usize;

    for jar in targets.iter() {
        match load_jar(deps.cache, deps.registry, deps.buffer, deps.cfr, jar) {
            Ok(load) => {
                succeeded += 1;
                if !load.skipped {
                    loaded_jars.push((load.jar_path.clone(), load.classes_loaded as u32));
                }
                loads.push(load);
            }
            Err(_) => {
                failed += 1;
            }
        }
    }

    Ok(WarmupResult {
        targets: targets.len(),
        succeeded,
        failed,
        duration_ms: start.elapsed().as_millis() as u64,
        loads,
        loaded_jars,
    })
}

fn index_repo(registry: &ClassRegistry, root: PathBuf) -> Result<IndexResult> {
    let start = Instant::now();
    let jars = scan_jars(&root)?;
    let mut cataloged_jars_new = 0usize;
    let mut indexed_classes = 0usize;
    let mut failed_jars = 0usize;

    for jar_path in jars.iter() {
        let jar_key = jar_path.to_string_lossy().to_string();
        if registry.is_cataloged(&jar_key).unwrap_or(false) {
            continue;
        }

        match catalog::catalog(jar_path) {
            Ok(classes) => {
                indexed_classes += classes.len();
                let _ = registry.update_registry_and_mark_cataloged(&jar_key, &classes);
                cataloged_jars_new += 1;
            }
            Err(_) => {
                failed_jars += 1;
            }
        }
    }

    Ok(IndexResult {
        root: root.to_string_lossy().to_string(),
        scanned_jars: jars.len(),
        cataloged_jars_new,
        indexed_classes,
        duration_ms: start.elapsed().as_millis() as u64,
        failed_jars,
    })
}

fn write_find_output(
    result: &FindResult,
    format: OutputFormat,
    output: Option<&Path>,
) -> Result<()> {
    let content = match format {
        OutputFormat::Json => serde_json::to_string_pretty(result)?,
        OutputFormat::Text => {
            let mut out = String::new();
            out.push_str(&format!("class_name: {}\n", result.class_name));
            out.push_str(&format!("matched_jars: {}\n", result.matched_jars));
            out.push_str(&format!("duration_ms: {}\n", result.duration_ms));
            for v in &result.versions {
                out.push_str(&format!(
                    "- version: {:?}, source: {}, cache_hit: {}, jar: {}\n",
                    v.version, v.source, v.cache_hit, v.jar_path
                ));
            }
            out
        }
        OutputFormat::Code => {
            let chosen = choose_default_version(&result.versions)?;
            chosen.content.clone()
        }
        OutputFormat::Structure => {
            #[derive(Serialize)]
            struct StructureVersion<'a> {
                version: &'a Option<String>,
                jar_path: &'a str,
                #[serde(skip_serializing_if = "Option::is_none")]
                structure: Option<ClassStructure>,
            }
            #[derive(Serialize)]
            struct StructureOutput<'a> {
                class_name: &'a str,
                matched_jars: usize,
                duration_ms: u64,
                versions: Vec<StructureVersion<'a>>,
            }
            let versions: Vec<StructureVersion> = result
                .versions
                .iter()
                .map(|v| StructureVersion {
                    version: &v.version,
                    jar_path: &v.jar_path,
                    structure: parse_class_structure(&v.content),
                })
                .collect();
            let out = StructureOutput {
                class_name: &result.class_name,
                matched_jars: result.matched_jars,
                duration_ms: result.duration_ms,
                versions,
            };
            serde_json::to_string_pretty(&out)?
        }
    };

    if let Some(path) = output {
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, content)?;
    } else {
        print!("{content}");
        if !content.ends_with('\n') {
            println!();
        }
    }

    Ok(())
}

fn choose_default_version(versions: &[FindVersion]) -> Result<&FindVersion> {
    versions
        .iter()
        .rfind(|v| v.version.is_some())
        .or_else(|| versions.first())
        .context("No available decompiled result")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_class_name_strips_import_whitespace_and_semicolon() {
        let raw = "import org.springframework.stereotype. Component ;";
        assert_eq!(
            normalize_class_name(raw),
            "org.springframework.stereotype.Component"
        );
    }

    #[test]
    fn rewrite_args_for_implicit_find_inserts_find_after_global_flags() {
        let args = vec![
            "class-finder".to_string(),
            "--db".to_string(),
            "/tmp/cf.lmdb".to_string(),
            "org.example.Demo".to_string(),
        ];
        let rewritten = rewrite_args_for_implicit_find(args);
        assert_eq!(rewritten[3], "find");
        assert_eq!(rewritten[4], "org.example.Demo");
    }

    #[test]
    fn rewrite_args_for_implicit_find_keeps_explicit_subcommand() {
        let args = vec![
            "class-finder".to_string(),
            "--db=/tmp/cf.lmdb".to_string(),
            "stats".to_string(),
        ];
        let rewritten = rewrite_args_for_implicit_find(args);
        assert_eq!(
            rewritten,
            vec!["class-finder", "--db=/tmp/cf.lmdb", "stats"]
        );
    }

    #[test]
    fn choose_default_version_prefers_latest_entry_with_version() {
        let versions = vec![
            FindVersion {
                version: None,
                jar_path: "a.jar".to_string(),
                content_hash: "h1".to_string(),
                content: "A".to_string(),
                cache_hit: true,
                source: "cache".to_string(),
                structure: None,
            },
            FindVersion {
                version: Some("1.0.0".to_string()),
                jar_path: "b.jar".to_string(),
                content_hash: "h2".to_string(),
                content: "B".to_string(),
                cache_hit: false,
                source: "scan".to_string(),
                structure: None,
            },
            FindVersion {
                version: Some("1.1.0".to_string()),
                jar_path: "c.jar".to_string(),
                content_hash: "h3".to_string(),
                content: "C".to_string(),
                cache_hit: false,
                source: "registry".to_string(),
                structure: None,
            },
        ];

        let picked = choose_default_version(&versions).unwrap();
        assert_eq!(picked.version.as_deref(), Some("1.1.0"));
        assert_eq!(picked.jar_path, "c.jar");
    }

    #[test]
    fn choose_default_version_fails_when_empty() {
        let err = choose_default_version(&[]).unwrap_err().to_string();
        assert!(err.contains("No available decompiled result"));
    }
}
