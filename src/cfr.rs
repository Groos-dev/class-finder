use anyhow::{bail, Context, Result};
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone)]
pub struct Cfr {
    cfr_jar: std::path::PathBuf,
}

impl Cfr {
    pub fn new(cfr_jar: std::path::PathBuf) -> Self {
        Self { cfr_jar }
    }

    pub fn decompile_class(&self, jar_path: &Path, class_name: &str) -> Result<String> {
        let output = Command::new("java")
            .args([
                "-jar",
                self.cfr_jar.to_str().context("cfr.jar 路径不是有效 UTF-8")?,
                "--extraclasspath",
                jar_path.to_str().context("jar 路径不是有效 UTF-8")?,
                class_name,
                "--silent",
                "true",
                "--comments",
                "false",
            ])
            .output()
            .context("执行 java 失败（请确认已安装 JRE/JDK）")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("CFR 反编译失败: {}", stderr.trim());
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    pub fn decompile_jar(&self, jar_path: &Path) -> Result<String> {
        let output = Command::new("java")
            .args([
                "-jar",
                self.cfr_jar.to_str().context("cfr.jar 路径不是有效 UTF-8")?,
                jar_path.to_str().context("jar 路径不是有效 UTF-8")?,
                "--silent",
                "true",
                "--comments",
                "false",
            ])
            .output()
            .context("执行 java 失败（请确认已安装 JRE/JDK）")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("CFR 反编译失败: {}", stderr.trim());
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
}
