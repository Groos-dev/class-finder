use anyhow::{Context, Result, bail};
use std::path::Path;
use std::process::Command;

fn java_command(args: &[&str]) -> Result<std::process::Output> {
    let java_bin = std::env::var("CLASS_FINDER_JAVA").unwrap_or_else(|_| "java".to_string());

    #[cfg(windows)]
    {
        let lower = java_bin.to_ascii_lowercase();
        if lower.ends_with(".cmd") || lower.ends_with(".bat") {
            return Command::new("cmd")
                .arg("/C")
                .arg(&java_bin)
                .args(args)
                .output()
                .context("Failed to execute java (ensure JRE/JDK is installed)");
        }
    }

    Command::new(&java_bin)
        .args(args)
        .output()
        .context("Failed to execute java (ensure JRE/JDK is installed)")
}

#[derive(Debug, Clone)]
pub struct Cfr {
    cfr_jar: std::path::PathBuf,
}

impl Cfr {
    pub fn new(cfr_jar: std::path::PathBuf) -> Self {
        Self { cfr_jar }
    }

    pub fn decompile_class(&self, jar_path: &Path, class_name: &str) -> Result<String> {
        let output = java_command(&[
            "-jar",
            self.cfr_jar
                .to_str()
                .context("cfr.jar path is not valid UTF-8")?,
            "--extraclasspath",
            jar_path.to_str().context("jar path is not valid UTF-8")?,
            class_name,
            "--silent",
            "true",
            "--comments",
            "false",
        ])?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("CFR decompilation failed: {}", stderr.trim());
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    pub fn decompile_jar(&self, jar_path: &Path) -> Result<String> {
        let output = java_command(&[
            "-jar",
            self.cfr_jar
                .to_str()
                .context("cfr.jar path is not valid UTF-8")?,
            jar_path.to_str().context("jar path is not valid UTF-8")?,
            "--silent",
            "true",
            "--comments",
            "false",
        ])?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("CFR decompilation failed: {}", stderr.trim());
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::{Mutex, OnceLock};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn path_env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn temp_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "class_finder_cfr_test_{}_{}_{}",
            std::process::id(),
            nanos,
            name
        ))
    }

    fn write_file(path: &std::path::Path, content: &str) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, content)?;
        Ok(())
    }

    fn make_executable(path: &std::path::Path) -> Result<()> {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(path)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms)?;
        Ok(())
    }

    #[test]
    fn decompile_class_uses_extraclasspath() -> Result<()> {
        let _guard = path_env_lock().lock().expect("PATH test lock poisoned");
        let base = temp_dir("decompile_class");
        let fake_cfr = base.join("cfr.jar");
        let fake_jar = base.join("demo.jar");
        let fake_bin = base.join("bin");
        let fake_java = fake_bin.join("java");

        write_file(&fake_cfr, "stub")?;
        write_file(&fake_jar, "stub")?;
        write_file(
            &fake_java,
            r#"#!/bin/sh
set -e
if [ "$3" = "--extraclasspath" ]; then
  cat <<'EOF'
package org.example;

public class Demo {
}
EOF
else
  echo "unexpected args" >&2
  exit 1
fi
"#,
        )?;
        make_executable(&fake_java)?;

        let old_path = std::env::var("PATH").unwrap_or_default();
        let new_path = format!("{}:{}", fake_bin.to_string_lossy(), old_path);
        // SAFETY: Guarded by path_env_lock and restored before returning.
        unsafe { std::env::set_var("PATH", &new_path) };

        let result: Result<()> = {
            let cfr = Cfr::new(fake_cfr);
            let out = cfr.decompile_class(&fake_jar, "org.example.Demo")?;
            assert!(out.contains("public class Demo"));
            Ok(())
        };

        // SAFETY: Guarded by path_env_lock and restored before returning.
        unsafe { std::env::set_var("PATH", old_path) };
        let _ = fs::remove_dir_all(base);
        result
    }

    #[test]
    fn decompile_jar_returns_cfr_error_stderr() -> Result<()> {
        let _guard = path_env_lock().lock().expect("PATH test lock poisoned");
        let base = temp_dir("decompile_jar_error");
        let fake_cfr = base.join("cfr.jar");
        let fake_jar = base.join("demo.jar");
        let fake_bin = base.join("bin");
        let fake_java = fake_bin.join("java");

        write_file(&fake_cfr, "stub")?;
        write_file(&fake_jar, "stub")?;
        write_file(
            &fake_java,
            r#"#!/bin/sh
echo "boom from fake cfr" >&2
exit 1
"#,
        )?;
        make_executable(&fake_java)?;

        let old_path = std::env::var("PATH").unwrap_or_default();
        let new_path = format!("{}:{}", fake_bin.to_string_lossy(), old_path);
        // SAFETY: Guarded by path_env_lock and restored before returning.
        unsafe { std::env::set_var("PATH", &new_path) };

        let result: Result<()> = {
            let cfr = Cfr::new(fake_cfr);
            let err = cfr.decompile_jar(&fake_jar).unwrap_err().to_string();
            assert!(err.contains("CFR decompilation failed"));
            assert!(err.contains("boom from fake cfr"));
            Ok(())
        };

        // SAFETY: Guarded by path_env_lock and restored before returning.
        unsafe { std::env::set_var("PATH", old_path) };
        let _ = fs::remove_dir_all(base);
        result
    }
}
