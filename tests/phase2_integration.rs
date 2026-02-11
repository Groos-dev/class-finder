use serde_json::Value;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_dir(name: &str) -> std::path::PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "class_finder_it_{}_{}_{}",
        std::process::id(),
        nanos,
        name
    ))
}

fn write_file(path: &std::path::Path, content: &str) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, content)?;
    Ok(())
}

fn write_jar(path: &std::path::Path, entries: &[(&str, &[u8])]) -> anyhow::Result<()> {
    use std::io::Write;
    use zip::write::FileOptions;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
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

#[cfg(unix)]
fn make_executable(path: &std::path::Path) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(path)?.permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(path, perms)?;
    Ok(())
}

#[cfg(not(unix))]
fn make_executable(_path: &std::path::Path) -> anyhow::Result<()> {
    Ok(())
}

fn run_json(bin: &str, args: &[&str], envs: &[(&str, &str)]) -> anyhow::Result<Value> {
    let mut cmd = Command::new(bin);
    cmd.args(args);
    for (k, v) in envs {
        cmd.env(k, v);
    }
    let out = cmd.output()?;
    if !out.status.success() {
        return Err(anyhow::anyhow!(
            "command failed: status={:?}, stderr={}",
            out.status.code(),
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(serde_json::from_slice(&out.stdout)?)
}

#[test]
fn phase2_three_layer_flow_works() -> anyhow::Result<()> {
    let base = temp_dir("phase2_flow");
    let m2 = base.join("m2");
    let db = base.join("db.lmdb");
    let fake_cfr = base.join("cfr.jar");
    write_file(&fake_cfr, "stub")?;

    let jar = m2.join("org/example/demo/1.0/demo-1.0.jar");
    write_jar(
        &jar,
        &[
            ("org/example/pkg/A.class", b""),
            ("org/example/pkg/B.class", b""),
        ],
    )?;

    let fake_bin_dir = base.join("bin");
    let fake_java = fake_bin_dir.join("java");
    write_file(
        &fake_java,
        r#"#!/bin/sh
set -e
if [ "$3" = "--extraclasspath" ]; then
  cls="$5"
  case "$cls" in
    org.example.pkg.A)
      cat <<'EOF'
/*
 * Decompiled with CFR 0.152.
 */
package org.example.pkg;

public class A {
}
EOF
      ;;
    org.example.pkg.B)
      cat <<'EOF'
/*
 * Decompiled with CFR 0.152.
 */
package org.example.pkg;

public class B {
}
EOF
      ;;
    *)
      echo "package org.example.pkg; public class Unknown {}"
      ;;
  esac
else
  cat <<'EOF'
/*
 * Decompiled with CFR 0.152.
 */
package org.example.pkg;

public class A {
}
/*
 * Decompiled with CFR 0.152.
 */
package org.example.pkg;

public class B {
}
EOF
fi
"#,
    )?;
    make_executable(&fake_java)?;

    let bin = env!("CARGO_BIN_EXE_class-finder");
    let path_env = format!(
        "{}:{}",
        fake_bin_dir.to_string_lossy(),
        std::env::var("PATH").unwrap_or_default()
    );
    let envs = [("PATH", path_env.as_str())];

    let first = run_json(
        bin,
        &[
            "--m2",
            m2.to_string_lossy().as_ref(),
            "--db",
            db.to_string_lossy().as_ref(),
            "--cfr",
            fake_cfr.to_string_lossy().as_ref(),
            "find",
            "org.example.pkg.A",
        ],
        &envs,
    )?;
    assert_eq!(first["versions"][0]["cache_hit"], Value::Bool(false));
    assert_eq!(
        first["versions"][0]["source"],
        Value::String("scan".to_string())
    );

    let second = run_json(
        bin,
        &[
            "--m2",
            m2.to_string_lossy().as_ref(),
            "--db",
            db.to_string_lossy().as_ref(),
            "--cfr",
            fake_cfr.to_string_lossy().as_ref(),
            "find",
            "org.example.pkg.A",
        ],
        &envs,
    )?;
    assert_eq!(second["versions"][0]["cache_hit"], Value::Bool(true));
    assert_eq!(
        second["versions"][0]["source"],
        Value::String("cache".to_string())
    );

    let load = run_json(
        bin,
        &[
            "--db",
            db.to_string_lossy().as_ref(),
            "--cfr",
            fake_cfr.to_string_lossy().as_ref(),
            "load",
            jar.to_string_lossy().as_ref(),
        ],
        &envs,
    )?;
    assert_eq!(load["skipped"], Value::Bool(true));

    let third = run_json(
        bin,
        &[
            "--m2",
            m2.to_string_lossy().as_ref(),
            "--db",
            db.to_string_lossy().as_ref(),
            "--cfr",
            fake_cfr.to_string_lossy().as_ref(),
            "find",
            "org.example.pkg.B",
        ],
        &envs,
    )?;
    assert_eq!(third["versions"][0]["cache_hit"], Value::Bool(true));
    assert_eq!(
        third["versions"][0]["source"],
        Value::String("cache".to_string())
    );

    let stats_after_load = run_json(
        bin,
        &["--db", db.to_string_lossy().as_ref(), "stats"],
        &envs,
    )?;
    assert!(stats_after_load["source_entries"].as_u64().unwrap_or(0) >= 2);
    assert!(stats_after_load["indexed_classes"].as_u64().unwrap_or(0) >= 2);
    assert!(stats_after_load["cataloged_jars"].as_u64().unwrap_or(0) >= 1);
    assert!(stats_after_load["loaded_jars"].as_u64().unwrap_or(0) >= 1);

    let db2 = base.join("db2.lmdb");
    let warm = run_json(
        bin,
        &[
            "--m2",
            m2.to_string_lossy().as_ref(),
            "--db",
            db2.to_string_lossy().as_ref(),
            "--cfr",
            fake_cfr.to_string_lossy().as_ref(),
            "warmup",
            jar.to_string_lossy().as_ref(),
        ],
        &envs,
    )?;
    assert!(warm["succeeded"].as_u64().unwrap_or(0) >= 1);

    let after_warm = run_json(
        bin,
        &[
            "--m2",
            m2.to_string_lossy().as_ref(),
            "--db",
            db2.to_string_lossy().as_ref(),
            "--cfr",
            fake_cfr.to_string_lossy().as_ref(),
            "find",
            "org.example.pkg.B",
        ],
        &envs,
    )?;
    assert_eq!(after_warm["versions"][0]["cache_hit"], Value::Bool(true));
    assert_eq!(
        after_warm["versions"][0]["source"],
        Value::String("cache".to_string())
    );

    let _ = std::fs::remove_dir_all(base);
    Ok(())
}

#[test]
fn phase2_implicit_find_with_global_flags_works() -> anyhow::Result<()> {
    let base = temp_dir("phase2_implicit_find");
    let m2 = base.join("m2");
    let db = base.join("db.lmdb");
    let fake_cfr = base.join("cfr.jar");
    write_file(&fake_cfr, "stub")?;

    let jar = m2.join("org/example/demo/1.0/demo-1.0.jar");
    write_jar(&jar, &[("org/example/pkg/A.class", b"")])?;

    let fake_bin_dir = base.join("bin");
    let fake_java = fake_bin_dir.join("java");
    write_file(
        &fake_java,
        r#"#!/bin/sh
set -e
if [ "$3" = "--extraclasspath" ]; then
  cat <<'EOF'
/*
 * Decompiled with CFR 0.152.
 */
package org.example.pkg;

public class A {
}
EOF
else
  cat <<'EOF'
/*
 * Decompiled with CFR 0.152.
 */
package org.example.pkg;

public class A {
}
EOF
fi
"#,
    )?;
    make_executable(&fake_java)?;

    let bin = env!("CARGO_BIN_EXE_class-finder");
    let path_env = format!(
        "{}:{}",
        fake_bin_dir.to_string_lossy(),
        std::env::var("PATH").unwrap_or_default()
    );
    let envs = [("PATH", path_env.as_str())];

    let result = run_json(
        bin,
        &[
            "--m2",
            m2.to_string_lossy().as_ref(),
            "--db",
            db.to_string_lossy().as_ref(),
            "--cfr",
            fake_cfr.to_string_lossy().as_ref(),
            "org.example.pkg.A",
        ],
        &envs,
    )?;

    assert_eq!(
        result["class_name"],
        Value::String("org.example.pkg.A".to_string())
    );
    assert_eq!(result["matched_jars"], Value::from(1));
    assert_eq!(result["versions"][0]["cache_hit"], Value::Bool(false));
    assert_eq!(
        result["versions"][0]["source"],
        Value::String("scan".to_string())
    );

    let _ = std::fs::remove_dir_all(base);
    Ok(())
}
