use anyhow::{Context, Result};
use clap::Parser;
use class_finder::cache::PersistentCache;
use class_finder::cfr::Cfr;
use class_finder::cli::{Cli, Commands, OutputFormat};
use class_finder::parse::{hash_content, parse_decompiled_output};
use class_finder::probe::{find_class_fqns_in_jar, jar_contains_class};
use class_finder::scan::{
    class_name_to_class_path, default_m2_repository, extract_version_from_maven_path,
    infer_scan_path, scan_jars,
};
use rayon::prelude::*;
use serde::Serialize;
use std::env;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

fn main() -> Result<()> {
    let cli = parse_cli()?;

    match cli.command.clone() {
        Commands::Clear => {
            let db_path = resolve_db_path(&cli)?;
            clear_db(&db_path)?;
        }
        Commands::Stats => {
            let db_path = resolve_db_path(&cli)?;
            let cache = PersistentCache::open(db_path)?;
            let stats = cache.stats()?;
            println!("{}", serde_json::to_string_pretty(&stats)?);
        }
        Commands::Load { jar_path } => {
            let cfr = Cfr::new(resolve_cfr_path(&cli)?);
            let db_path = resolve_db_path(&cli)?;
            let cache = PersistentCache::open(db_path)?;
            let output = load_jar(&cache, &cfr, &jar_path)?;
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
            let effective_format = if code_only { OutputFormat::Code } else { format };
            let class_name = normalize_class_name(&class_name);
            let result = find_class(&cache, &cfr, resolve_m2_repo(&cli)?, &class_name, version)?;
            write_find_output(&result, effective_format, output.as_deref())?;
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

    let subcommands = ["find", "load", "stats", "clear", "help"];

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

fn resolve_m2_repo(cli: &Cli) -> Result<PathBuf> {
    if let Some(p) = cli.m2.clone() {
        return Ok(p);
    }
    default_m2_repository()
}

fn resolve_db_path(cli: &Cli) -> Result<PathBuf> {
    if let Some(p) = cli.db.clone() {
        return Ok(p);
    }

    Ok(class_finder_home()?.join("db.redb"))
}

fn resolve_cfr_path(cli: &Cli) -> Result<PathBuf> {
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

fn clear_db(db_path: &Path) -> Result<()> {
    if db_path.exists() {
        std::fs::remove_file(db_path)
            .with_context(|| format!("无法删除 db 文件: {}", db_path.display()))?;
    }
    Ok(())
}

fn class_finder_home() -> Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("无法获取 home 目录"))?;
    Ok(home.join(".class-finder"))
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

    eprintln!("[class-finder] 未找到 CFR，正在下载到 {}", target_path.display());
    let status = std::process::Command::new("curl")
        .args([
            "-L",
            "--fail",
            "--silent",
            "--show-error",
            "-o",
            target_path.to_str().context("cfr.jar 目标路径不是有效 UTF-8")?,
            url,
        ])
        .status()
        .context("执行 curl 失败（请确认 macOS 已安装 curl，或使用 --cfr 指定 cfr.jar）")?;

    if !status.success() {
        anyhow::bail!("下载 CFR 失败（退出码: {status}）。可用 --cfr 指定本地 cfr.jar");
    }

    Ok(())
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

fn find_class(
    cache: &PersistentCache,
    cfr: &Cfr,
    m2_repo: PathBuf,
    class_name: &str,
    version_filter: Option<String>,
) -> Result<FindResult> {
    let start = Instant::now();
    let (resolved_class_name, mut matched, scan_root) = if class_name.contains('.') {
        let scan_root = infer_scan_path(&m2_repo, class_name);
        let jars = scan_jars(&scan_root)?;
        let class_path = class_name_to_class_path(class_name);

        let matched: Vec<PathBuf> = jars
            .par_iter()
            .filter_map(|jar| match jar_contains_class(jar, &class_path) {
                Ok(true) => Some(jar.clone()),
                Ok(false) => None,
                Err(_) => None,
            })
            .collect();

        (class_name.to_string(), matched, scan_root)
    } else {
        let scan_root = m2_repo.clone();
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
            .with_context(|| format!("未找到类 {class_name}（扫描目录: {}）", scan_root.display()))?;

        (best_fqn, best_jars, scan_root)
    };

    if let Some(v) = version_filter.clone() {
        matched.retain(|p| extract_version_from_maven_path(p).as_deref() == Some(v.as_str()));
    }

    matched.sort_by(|a, b| extract_version_from_maven_path(a).cmp(&extract_version_from_maven_path(b)));

    if matched.is_empty() {
        anyhow::bail!(
            "未找到类 {resolved_class_name}（扫描目录: {}）",
            scan_root.display()
        );
    }

    let mut versions = Vec::new();
    let mut pending_writes: Vec<(String, String)> = Vec::new();

    for jar_path in matched.iter() {
        let jar_key = jar_path.to_string_lossy().to_string();
        let cache_key = format!("{resolved_class_name}::{jar_key}");

        if let Some(content) = cache.get_class_source(&cache_key)? {
            versions.push(FindVersion {
                version: extract_version_from_maven_path(jar_path),
                jar_path: jar_key,
                content_hash: hash_content(&content),
                content,
                cache_hit: true,
            });
            continue;
        }

        let decompiled = cfr.decompile_class(jar_path, &resolved_class_name)?;
        let parsed = parse_decompiled_output(&decompiled);
        let content = parsed
            .iter()
            .find(|c| c.class_name == resolved_class_name)
            .map(|c| c.content.clone())
            .unwrap_or(decompiled);
        let content_hash = hash_content(&content);

        pending_writes.push((cache_key, content.clone()));
        versions.push(FindVersion {
            version: extract_version_from_maven_path(jar_path),
            jar_path: jar_key,
            content_hash,
            content,
            cache_hit: false,
        });
    }

    let _ = cache.put_class_sources(&pending_writes)?;

    Ok(FindResult {
        class_name: resolved_class_name,
        scanned_root: scan_root.to_string_lossy().to_string(),
        matched_jars: matched.len(),
        duration_ms: start.elapsed().as_millis() as u64,
        versions,
    })
}

fn load_jar(cache: &PersistentCache, cfr: &Cfr, jar_path: &Path) -> Result<LoadResult> {
    let jar_key = jar_path.to_string_lossy().to_string();
    let start = Instant::now();

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

    let mut entries = Vec::with_capacity(classes.len());
    for cls in classes {
        let key = format!("{}::{jar_key}", cls.class_name);
        entries.push((key, cls.content));
    }

    let classes_loaded = cache.put_class_sources(&entries)?;
    cache.mark_jar_loaded(&jar_key)?;

    Ok(LoadResult {
        jar_path: jar_key,
        classes_loaded,
        skipped: false,
        duration_ms: start.elapsed().as_millis() as u64,
    })
}

fn write_find_output(result: &FindResult, format: OutputFormat, output: Option<&Path>) -> Result<()> {
    let content = match format {
        OutputFormat::Json => serde_json::to_string_pretty(result)?,
        OutputFormat::Text => {
            let mut out = String::new();
            out.push_str(&format!("class_name: {}\n", result.class_name));
            out.push_str(&format!("matched_jars: {}\n", result.matched_jars));
            out.push_str(&format!("duration_ms: {}\n", result.duration_ms));
            for v in &result.versions {
                out.push_str(&format!(
                    "- version: {:?}, cache_hit: {}, jar: {}\n",
                    v.version, v.cache_hit, v.jar_path
                ));
            }
            out
        }
        OutputFormat::Code => {
            let chosen = choose_default_version(&result.versions)?;
            chosen.content.clone()
        }
    };

    if let Some(path) = output {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
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
        .filter(|v| v.version.is_some())
        .last()
        .or_else(|| versions.first())
        .context("没有可用的反编译结果")
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
    fn rewrite_args_for_implicit_find_skips_global_option_values() {
        let args = vec![
            "class-finder".to_string(),
            "--cfr".to_string(),
            "/tmp/cfr.jar".to_string(),
            "--db".to_string(),
            "/tmp/db.redb".to_string(),
            "org.springframework.stereotype.Component".to_string(),
            "--code-only".to_string(),
        ];

        let rewritten = rewrite_args_for_implicit_find(args);
        assert_eq!(rewritten[1], "--cfr");
        assert_eq!(rewritten[2], "/tmp/cfr.jar");
        assert_eq!(rewritten[3], "--db");
        assert_eq!(rewritten[4], "/tmp/db.redb");
        assert_eq!(rewritten[5], "find");
        assert_eq!(rewritten[6], "org.springframework.stereotype.Component");
    }
}
