## 流程概览

```jsx
输入: 类名 或 类全限定名
       ↓
① 检查缓存（jar 是否已加载）
       ↓ 未加载
② 打开 jar 文件（zip 解析）
       ↓
③ 遍历所有 .class 文件
       ↓
④ 调用 CFR 反编译整个 jar
       ↓
⑤ 解析反编译输出，拆分每个类
       ↓
⑥ 批量写入缓存（redb）
       ↓
⑦ 标记 jar 已加载
       ↓
输出: 加载结果 JSON
```

---

## 模块拆分

### 1. JarLoader - 核心加载器

```rust
pub struct JarLoader {
    cache: Arc<PersistentCache>,
    cfr_path: PathBuf,
}

impl JarLoader {
    /// 加载单个 jar 的所有类
    pub fn load(&self, jar_path: &Path) -> Result<LoadResult>;
    
    /// 异步加载（后台线程）
    pub fn load_async(&self, jar_path: PathBuf);
    
    /// 检查 jar 是否已加载
    pub fn is_loaded(&self, jar_path: &Path) -> bool;
}
```

### 2. ClassParser - 类解析器

```rust
pub struct ClassParser;

impl ClassParser {
    /// 从反编译输出解析所有类
    pub fn parse_decompiled_output(content: &str) -> Vec<ParsedClass>;
    
    /// 提取类全限定名
    fn extract_class_name(content: &str) -> Option<String>;
    
    /// 提取 package 声明
    fn extract_package(content: &str) -> Option<String>;
}

pub struct ParsedClass {
    pub class_name: String,      // 全限定名
    pub content: String,         // 反编译源码
    pub content_hash: String,    // 内容 hash
}
```

### 3. Decompiler - 反编译器封装

```rust
pub struct Decompiler {
    cfr_path: PathBuf,
}

impl Decompiler {
    /// 反编译整个 jar
    pub fn decompile_jar(&self, jar_path: &Path) -> Result<String>;
    
    /// 反编译单个类（快速模式）
    pub fn decompile_class(&self, jar_path: &Path, class_filter: &str) -> Result<String>;
}
```

---

## 详细实现

### Step 1: 检查缓存

```rust
fn is_jar_loaded(&self, jar_path: &Path) -> Result<bool> {
    let jar_key = jar_path.to_string_lossy().to_string();
    let read_txn = self.db.begin_read()?;
    let table = read_txn.open_table(JARS_TABLE)?;
    Ok(table.get(jar_key.as_str())?.is_some())
}
```

### Step 2: 解析 jar 文件结构

```rust
fn list_classes_in_jar(jar_path: &Path) -> Result<Vec<String>> {
    let file = File::open(jar_path)?;
    let mut archive = ZipArchive::new(file)?;
    
    let class_names: Vec<String> = (0..archive.len())
        .filter_map(|i| {
            archive.by_index(i).ok().and_then(|entry| {
                let name = entry.name();
                // 过滤：只要 .class，排除内部类 ($)
                if name.ends_with(".class") && !name.contains('$') {
                    Some(name.trim_end_matches(".class")
                        .replace('/', ".")
                        .to_string())
                } else {
                    None
                }
            })
        })
        .collect();
    
    Ok(class_names)
}
```

### Step 3: 调用 CFR 反编译

```rust
fn decompile_jar(&self, jar_path: &Path) -> Result<String> {
    let output = Command::new("java")
        .args([
            "-jar",
            self.cfr_path.to_str().unwrap(),
            jar_path.to_str().unwrap(),
            "--silent", "true",           // 减少无用输出
            "--comments", "false",        // 不要注释
        ])
        .output()
        .context("反编译失败")?;

    if !output.status.success() {
        anyhow::bail!("CFR 退出码: {}", output.status);
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}
```

### Step 4: 解析反编译输出

CFR 输出格式：

```java
/*
 * Decompiled with CFR 0.152.
 */
package org.apache.commons.lang3;

public class StringUtils {
    ...
}
/*
 * Decompiled with CFR 0.152.
 */
package org.apache.commons.lang3;

public class ArrayUtils {
    ...
}
```

解析逻辑：

```rust
fn parse_decompiled_output(content: &str) -> Vec<ParsedClass> {
    let mut results = Vec::new();
    
    // 按 "/*\n * Decompiled" 分割
    let parts: Vec<&str> = content.split("/*\n * Decompiled").collect();
    
    for part in parts.iter().skip(1) {
        let class_content = format!("/*\n * Decompiled{}", part);
        
        if let Some(class_name) = extract_class_name(part) {
            let content_hash = hash_content(&class_content);
            results.push(ParsedClass {
                class_name,
                content: class_content,
                content_hash,
            });
        }
    }
    
    results
}

fn extract_class_name(content: &str) -> Option<String> {
    let mut package = String::new();
    let mut class_name = String::new();
    
    for line in content.lines() {
        let line = line.trim();
        
        // 提取 package
        if line.starts_with("package ") {
            package = line
                .trim_start_matches("package ")
                .trim_end_matches(';')
                .to_string();
        }
        
        // 提取类名
        for keyword in ["public class ", "class ", "public interface ", 
                        "interface ", "public enum ", "enum "] {
            if line.contains(keyword) {
                if let Some(idx) = line.find(keyword) {
                    let after = &line[idx + keyword.len()..];
                    if let Some(name) = after.split_whitespace().next() {
                        class_name = name
                            .trim_end_matches('{')
                            .trim_end_matches('<')
                            .to_string();
                        break;
                    }
                }
            }
        }
        
        if !class_name.is_empty() {
            break;
        }
    }
    
    if class_name.is_empty() {
        return None;
    }
    
    Some(if package.is_empty() {
        class_name
    } else {
        format!("{}.{}", package, class_name)
    })
}
```

### Step 5: 批量写入缓存

```rust
fn batch_save_classes(
    &self,
    jar_path: &str,
    classes: &[ParsedClass],
) -> Result<usize> {
    let write_txn = self.db.begin_write()?;
    let mut count = 0;
    
    {
        let mut table = write_txn.open_table(CLASSES_TABLE)?;
        for cls in classes {
            // key 格式: "类全限定名::jar路径"
            let key = format!("{}::{}", cls.class_name, jar_path);
            table.insert(key.as_str(), cls.content.as_str())?;
            count += 1;
        }
    }
    
    // 标记 jar 已加载
    {
        let mut jar_table = write_txn.open_table(JARS_TABLE)?;
        jar_table.insert(jar_path, "1")?;
    }
    
    write_txn.commit()?;
    Ok(count)
}
```

---

## 完整 Load 流程

```rust
pub fn load_jar(&self, jar_path: &Path) -> Result<LoadResult> {
    let jar_key = jar_path.to_string_lossy().to_string();
    let start = Instant::now();
    
    // 1. 检查是否已加载
    if self.is_jar_loaded(&jar_key)? {
        return Ok(LoadResult {
            jar_path: jar_key,
            classes_loaded: 0,
            skipped: true,
            duration_ms: 0,
        });
    }
    
    // 2. 反编译整个 jar
    let decompiled = self.decompiler.decompile_jar(jar_path)?;
    
    // 3. 解析所有类
    let classes = ClassParser::parse_decompiled_output(&decompiled);
    
    // 4. 批量写入缓存
    let count = self.batch_save_classes(&jar_key, &classes)?;
    
    let duration = start.elapsed().as_millis() as u64;
    
    Ok(LoadResult {
        jar_path: jar_key,
        classes_loaded: count,
        skipped: false,
        duration_ms: duration,
    })
}
```

---

## 异步加载

```rust
pub fn load_async(&self, jar_path: PathBuf) {
    let loader = self.clone(); // 需要 Clone trait
    
    thread::spawn(move || {
        match loader.load_jar(&jar_path) {
            Ok(result) => {
                if !result.skipped {
                    eprintln!(
                        "[后台加载完成] {} 个类, 耗时 {}ms - {}",
                        result.classes_loaded,
                        result.duration_ms,
                        result.jar_path
                    );
                }
            }
            Err(e) => {
                eprintln!("[后台加载失败] {} - {}", jar_path.display(), e);
            }
        }
    });
}
```

---

## 输出结构

```rust
#[derive(Serialize)]
pub struct LoadResult {
    pub jar_path: String,
    pub classes_loaded: usize,
    pub skipped: bool,         // true 表示已加载过，跳过
    pub duration_ms: u64,
}
```

输出示例：

```json
{
  "jar_path": "~/.m2/.../commons-lang3-3.12.0.jar",
  "classes_loaded": 156,
  "skipped": false,
  "duration_ms": 2340
}
```

---

## 错误处理

| 错误场景 | 处理方式 |
| --- | --- |
| jar 文件不存在 | 返回 `FileNotFound` 错误 |
| jar 损坏/无法解压 | 返回 `InvalidJar` 错误 |
| CFR 反编译失败 | 返回 `DecompileError`，包含 stderr |
| 解析失败（无法提取类名） | 跳过该类，继续处理其他 |
| 缓存写入失败 | 返回 `CacheError` |

---

## 性能优化点

- [ ]  **并行解析** - 反编译输出拆分后并行解析类名
- [ ]  **流式写入** - 边解析边写缓存，减少内存占用
- [ ]  **CFR 参数调优** - `--silent`、`--comments false` 减少输出

---

## CLI 设计（支持管道过滤）

### 设计原则

- **Unix 哲学** - 单一职责，支持管道组合
- **多种输出格式** - JSON、纯文本、仅源码
- **AI 友好** - 可结合 grep/jq/awk 过滤

### 命令行参数

```bash
class-finder find <类名> [OPTIONS]

OPTIONS:
    -f, --format <FORMAT>    输出格式 [json|text|code] (默认: json)
    -o, --output <FILE>      输出到文件
    -v, --version <VER>      指定版本号过滤
    --no-diff                不输出 diff 信息
    --code-only              仅输出源码（无 JSON 包装）
    --method <NAME>          过滤指定方法
    --fields <FIELDS>        过滤指定字段（逗号分隔）
```

### 输出格式

**1. JSON 格式（默认，AI 可解析）**

```bash
class-finder find org.apache.commons.lang3.StringUtils
```

```json
{
  "class_name": "org.apache.commons.lang3.StringUtils",
  "versions": [...],
  "has_diff": true
}
```

**2. 纯代码格式（方便 grep）**

```bash
class-finder find StringUtils --code-only
```

```java
package org.apache.commons.lang3;

public class StringUtils {
    public static boolean isEmpty(CharSequence cs) {
        return cs == null || cs.length() == 0;
    }
    ...
}
```

**3. 文本格式（简洁摘要）**

```bash
class-finder find StringUtils -f text
```

```
类名: org.apache.commons.lang3.StringUtils
Jar: ~/.m2/.../commons-lang3-3.12.0.jar
版本: 3.12.0
方法数: 156
```

### 管道组合示例

**1. 查找包含某方法的行**

```bash
class-finder find StringUtils --code-only | grep -n "isEmpty"
```

**2. 提取所有 public 方法签名**

```bash
class-finder find StringUtils --code-only | grep "public static"
```

**3. 用 jq 提取特定字段**

```bash
class-finder find StringUtils | jq '.versions[0].content'
```

**4. 查找并统计方法数**

```bash
class-finder find StringUtils --code-only | grep -c "public "
```

**5. 多类查找 + 合并**

```bash
for cls in StringUtils ArrayUtils ObjectUtils; do
  class-finder find "org.apache.commons.lang3.$cls" --code-only
done | grep "public static"
```

**6. AI Agent 典型用法**

```bash
# 查找类并过滤出 isEmpty 相关方法
class-finder find StringUtils --code-only | grep -A5 "isEmpty"

# 查找类并提取字段定义
class-finder find MyEntity --code-only | grep -E "private|protected" | grep -v "static"
```

### Rust 实现

```rust
use clap::{Parser, ValueEnum};

#[derive(Parser)]
#[command(name = "class-finder")]
#[command(about = "Java 类查找工具，支持管道过滤")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// 查找类
    Find {
        /// 类全限定名
        class_name: String,
        
        /// 输出格式
        #[arg(short, long, default_value = "json")]
        format: OutputFormat,
        
        /// 仅输出源码
        #[arg(long)]
        code_only: bool,
        
        /// 过滤指定方法
        #[arg(long)]
        method: Option<String>,
        
        /// 指定版本
        #[arg(short, long)]
        version: Option<String>,
    },
    /// 加载 jar
    Load { jar_path: String },
    /// 缓存统计
    Stats,
    /// 清除缓存
    Clear,
}

#[derive(Clone, ValueEnum)]
enum OutputFormat {
    Json,
    Text,
    Code,
}
```

### 输出处理

```rust
fn output_result(result: &Output, format: OutputFormat, code_only: bool) {
    if code_only {
        // 纯代码输出，方便 grep
        for ver in &result.versions {
            println!("{}", ver.content);
        }
        return;
    }
    
    match format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
        }
        OutputFormat::Text => {
            println!("类名: {}", result.class_name);
            for ver in &result.versions {
                println!("Jar: {}", ver.jar_path);
                println!("版本: {}", ver.version);
            }
        }
        OutputFormat::Code => {
            for ver in &result.versions {
                println!("// === {} ===", ver.version);
                println!("{}", ver.content);
            }
        }
    }
}
```

### 方法过滤功能

```rust
fn filter_by_method(content: &str, method_name: &str) -> String {
    let mut result = Vec::new();
    let mut in_method = false;
    let mut brace_count = 0;
    
    for line in content.lines() {
        if line.contains(method_name) && line.contains("(") {
            in_method = true;
        }
        
        if in_method {
            result.push(line);
            brace_count += line.matches('{').count() as i32;
            brace_count -= line.matches('}').count() as i32;
            
            if brace_count == 0 {
                in_method = false;
                result.push(""); // 空行分隔
            }
        }
    }
    
    result.join("\n")
}
```

### AI Agent 集成示例

```yaml
# Agent Skill 定义
name: class-finder
description: |
  查找 Java 类并返回源码。
  支持管道过滤，可结合 grep/jq 提取关心的内容。

usage_examples:
  - 查找类并提取 isEmpty 方法:
    class-finder find StringUtils --code-only | grep -A10 "isEmpty"
  
  - 查找类并统计方法数:
    class-finder find StringUtils --code-only | grep -c "public "
  
  - 查找并过滤特定版本:
    class-finder find StringUtils --version 3.12.0
  
  - 提取 JSON 中的特定字段:
    class-finder find StringUtils | jq '.versions[0].jar_path'
```