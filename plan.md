## 项目概述

构建一个高性能的 Java 类查找工具，供 AI Agent 使用。核心功能：

- 在 `.m2` 仓库中查找类所在的 jar 包
- 反编译并返回类源码
- 支持多版本对比
- 持久化缓存 + 懒加载

---

## 技术栈

| 组件 | 选型 | 说明 |
| --- | --- | --- |
| 语言 | Rust | 高性能、无 GC |
| 缓存 | redb | 嵌入式 KV 数据库，持久化 |
| 并行 | rayon | 并行扫描 jar |
| Diff | similar | 文本差异对比 |
| 反编译 | CFR | Java 反编译器 |
| CLI | clap | 命令行解析 |
| 文件扫描 | ignore | fd/rg 底层库，内置并行 |
| jar 解析 | zip + memmap2 | 内存映射，按需加载 |

---

## 核心流程

```jsx
输入: 类名（如 org.apache.commons.lang3.StringUtils）
       ↓
① 推断扫描范围（org/apache/commons/）
       ↓
② ignore 并行扫描 jar 文件
       ↓
③ mmap 快速检查每个 jar 是否包含目标类（只读 zip 目录，~0 内存）
       ↓ 找到匹配的 jar
④ CFR 反编译整个 jar
       ↓
⑤ 立即返回目标类给用户
       ↓ 同时（异步）
⑥ 后台将其他类写入 cache（空间局部性预热）
       ↓
输出: JSON 结果
```

### 空间局部性收益

| 场景 | 行为 | 效果 |
| --- | --- | --- |
| 查 `StringUtils` | 预热 `ArrayUtils`、`ObjectUtils`... | 同 jar 其他类入库 |
| 下次查同 jar 的类 | 缓存命中 | 秒返回，无需反编译 |

### 用户感知

- **等待时间**：只等目标类反编译
- **后台静默**：其他类异步入库，不阻塞响应

---

## 核心功能

- [ ]  **基础查找** - 根据类名查找 jar 并返回源码
- [ ]  **智能扫描范围缩小** - 根据类名推断 groupId，只扫描对应目录
- [ ]  **持久化缓存** - redb 存储，重启不丢失
- [ ]  **懒加载策略** - 查询单个类时，后台异步加载整个 jar
- [ ]  **多版本支持** - 扫描所有包含该类的 jar
- [ ]  **Diff 对比** - 检测版本差异，生成摘要
- [ ]  **多格式输出** - JSON / 纯代码 / 文本摘要
- [ ]  **管道过滤支持** - 支持 grep/jq/awk 组合过滤

---

## 阶段计划

### Phase 1：基础框架 (Day 1-2)

- [ ]  初始化 Rust 项目
- [ ]  实现 jar 扫描逻辑（walkdir + rayon 并行）
- [ ]  集成 CFR 反编译器
- [ ]  实现单类查找并返回 JSON
- **交付物**: `class-finder find <类名>` 命令可用

### Phase 2：缓存层 (Day 3-4)

- [ ]  集成 redb 持久化存储
- [ ]  实现缓存读写逻辑
- [ ]  缓存 key 设计：`类名::jar路径`
- [ ]  实现 `stats` 和 `clear` 命令
- **交付物**: 首次查询后，后续秒返回

### Phase 3：异步预热 (Day 5)

- [ ]  查询时先返回结果
- [ ]  后台线程加载整个 jar 的所有类
- [ ]  标记已加载的 jar，避免重复
- **交付物**: `background_loading: true` 字段

### Phase 4：多版本对比 (Day 6-7)

- [ ]  扫描所有包含该类的 jar（并行）
- [ ]  提取版本号（从路径解析）
- [ ]  内容 hash 快速比较
- [ ]  集成 similar 库生成 diff 摘要
- [ ]  输出 `recommendation` 字段引导 AI
- **交付物**: 多版本差异检测完整可用

### Phase 5：优化与发布 (Day 8)

- [ ]  性能优化（并行度调优）
- [ ]  错误处理完善
- [ ]  编写 README
- [ ]  发布到 GitHub
- [ ]  编写 Agent Skill 定义文件

---

## 扫描范围优化

Maven 仓库目录结构按 `groupId` 组织，可利用类名推断扫描路径：

### 推断逻辑

```
import org.apache.commons.lang3.StringUtils
       ↓ 解析
groupId 前缀: org.apache.commons
       ↓ 转路径
~/.m2/repository/org/apache/commons/
       ↓ 
只扫描这个目录下的 jar
```

### 效果对比

| 场景 | 扫描范围 | 预估 jar 数量 |
| --- | --- | --- |
| 全量扫描 ~/.m2/repository | 全部 | 5000+ |
| org.apache.commons.lang3.StringUtils | org/apache/commons/ | ~50 |
| [com.google](http://com.google).common.collect.Lists | com/google/ | ~100 |
| cn.hutool.core.util.StrUtil | cn/hutool/ | ~20 |

### 实现代码

```rust
/// 从类全限定名推断扫描路径
fn infer_scan_paths(class_name: &str) -> Vec<PathBuf> {
    let home = dirs::home_dir().unwrap();
    let m2_base = home.join(".m2/repository");
    
    // org.apache.commons.lang3.StringUtils → ["org", "apache", ...]
    let parts: Vec<&str> = class_name.split('.').collect();
    
    let mut paths = Vec::new();
    
    // 逐级尝试：org/apache/commons → org/apache → org
    for i in (2..parts.len().saturating_sub(1)).rev() {
        let prefix = parts[..i].join("/");
        let path = m2_base.join(&prefix);
        if path.exists() {
            paths.push(path);
            break; // 找到最精确的路径即可
        }
    }
    
    // 兆底：全量扫描
    if paths.is_empty() {
        paths.push(m2_base);
    }
    
    paths
}
```

**收益**：扫描时间从 **10秒+** 降到 **<1秒**

---

## 扫描方案选型

### 方案对比

| 方案 | 效率 | 内存 | 优点 | 缺点 |
| --- | --- | --- | --- | --- |
| **fd** | ⭐⭐⭐⭐⭐ | 低 | 极快，并行 | 外部依赖，需 spawn 进程 |
| **walkdir + rayon** | ⭐⭐⭐⭐ | 可控 | 零依赖，灵活 | 需自己写并行逻辑 |
| **ignore crate** ✅ | ⭐⭐⭐⭐⭐ | 低 | fd/rg 底层库，内置并行 | 多一个依赖 |

> ripgrep 主要是**内容搜索**，不适合文件扫描场景
> 

### 选择：ignore crate

**原因：**

1. **ignore 就是 fd 的底层** — 性能一致，但无需 spawn 进程
2. **边扫边处理** — 找到 jar 直接处理，不用等全部扫完
3. **内存可控** — 不会一次性加载所有路径到内存
4. **零外部工具依赖** — 用户无需安装 fd

### 性能预估（扫描 5000 个 jar）

| 方案 | 耗时 | 内存峰值 |
| --- | --- | --- |
| `find` (系统命令) | ~3s | ~10MB |
| `fd` | ~200ms | ~15MB |
| `walkdir` (单线程) | ~800ms | ~5MB |
| `ignore` (并行) | ~200ms | ~8MB |

### 实现代码

```rust
use ignore::WalkBuilder;
use crossbeam_channel;

fn scan_jars(base_path: &Path) -> Vec<PathBuf> {
    let walker = WalkBuilder::new(base_path)
        .hidden(false)           // 不跳过隐藏文件
        .git_ignore(false)       // 忽略 .gitignore
        .build_parallel();       // 并行遍历
    
    let (tx, rx) = crossbeam_channel::unbounded();
    
    walker.run(|| {
        let tx = tx.clone();
        Box::new(move |entry| {
            if let Ok(entry) = entry {
                let path = entry.path();
                if path.extension().map_or(false, |e| e == "jar") {
                    tx.send(path.to_path_buf()).ok();
                }
            }
            ignore::WalkState::Continue
        })
    });
    
    drop(tx);
    rx.iter().collect()
}
```

---

## Jar 解析方案

### 技术选型

- **反编译器**：CFR（支持 Java 21+，可单类反编译）
- **jar 读取**：zip + memmap2（内存映射，按需加载）

### 为什么用 mmap

| 方案 | 内存占用 | 多次读取 | 说明 |
| --- | --- | --- | --- |
| 普通 File::open | 每次读取都加载 | 重复 IO | 简单但不高效 |
| **memmap2** ✅ | 按需加载页面 | OS 缓存复用 | 多次访问同一 jar 更快 |

### 类名匹配策略

```jsx
输入: org.apache.commons.lang3.StringUtils
       ↓ 转换为路径
org/apache/commons/lang3/StringUtils.class
       ↓ 检查 jar 是否包含该文件
ZipArchive::by_name("org/.../StringUtils.class")
       ↓ 存在
只反编译这一个类（不是整个 jar）
```

### 实现代码

```rust
use memmap2::Mmap;
use zip::ZipArchive;
use std::io::Cursor;

/// 检查 jar 是否包含指定类
fn jar_contains_class(jar_path: &Path, class_name: &str) -> Result<bool> {
    // 类名转路径: org.apache.commons.lang3.StringUtils → org/apache/.../StringUtils.class
    let class_path = class_name.replace('.', "/") + ".class";
    
    // mmap 打开 jar
    let file = File::open(jar_path)?;
    let mmap = unsafe { Mmap::map(&file)? };
    let mut archive = ZipArchive::new(Cursor::new(&mmap[..]))?;
    
    // 检查是否存在
    Ok(archive.by_name(&class_path).is_ok())
}

/// 反编译指定类
fn decompile_class(jar_path: &Path, class_name: &str) -> Result<String> {
    let output = Command::new("java")
        .args([
            "-jar", CFR_PATH,
            jar_path.to_str().unwrap(),
            "--methodname", class_name,  // 只反编译指定类
            "--silent", "true",
        ])
        .output()?;
    
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// 完整查找流程
fn find_class(class_name: &str) -> Result<Vec<ClassResult>> {
    let scan_paths = infer_scan_paths(class_name);
    let jars = scan_jars(&scan_paths);
    
    // 并行检查所有 jar
    let matches: Vec<_> = jars
        .par_iter()
        .filter(|jar| jar_contains_class(jar, class_name).unwrap_or(false))
        .collect();
    
    // 反编译匹配的 jar
    let results: Vec<_> = matches
        .par_iter()
        .map(|jar| {
            let content = decompile_class(jar, class_name)?;
            let version = extract_version_from_path(jar);
            Ok(ClassResult { jar_path: jar.clone(), version, content })
        })
        .collect();
    
    Ok(results)
}
```

### 内存占用分析

| 操作 | 内存 | 说明 |
| --- | --- | --- |
| mmap 打开 jar | ~0 | 只建立映射，OS 按需加载页面 |
| 读取 zip 目录 | ~5-20KB | 只加载目录索引部分 |
| by_name 检查 | ~0 | 只查目录，无 IO |
| 解压单个 .class | ~10-100KB | 只解压目标文件 |

---

## 数据结构设计

### 多版本对比逻辑

```jsx
多个 jar 版本（如 3.10.0, 3.11.0, 3.12.0）
       ↓ 反编译同一个类
多个类版本
       ↓ 按 content_hash 分组
合并相同内容的版本
       ↓
输出：每种不同内容只输出一次，关联对应版本号列表
```

**示例：**

- 3.10.0, 3.11.0 内容相同 → 合并为一个 `content_variant`，`versions: ["3.10.0", "3.11.0"]`
- 3.12.0 内容不同 → 输出另一个 `content_variant`，`versions: ["3.12.0"]`

### 输出 JSON

```json
{
  "class_name": "org.apache.commons.lang3.StringUtils",
  "total_jar_versions": 3,
  "unique_content_variants": 2,
  "has_diff": true,
  "content_variants": [
    {
      "content_hash": "a1b2c3d4",
      "content": "public class StringUtils {...}",
      "versions": [
        { "version": "3.10.0", "jar_path": "~/.m2/.../commons-lang3-3.10.0.jar" },
        { "version": "3.11.0", "jar_path": "~/.m2/.../commons-lang3-3.11.0.jar" }
      ]
    },
    {
      "content_hash": "e5f6g7h8",
      "content": "public class StringUtils { /* 新版本 */ ...}",
      "versions": [
        { "version": "3.12.0", "jar_path": "~/.m2/.../commons-lang3-3.12.0.jar" }
      ]
    }
  ],
  "diff_summary": "3.12.0 版本新增 isEmpty(CharSequence) 方法...",
  "recommendation": "发现 2 种不同内容，建议使用最新版 3.12.0"
}
```

### 存储层设计

```jsx
SourceCache 表: "类全限定名::artifact路径" -> "反编译源码"
ClassRegistry 表: "类名" -> "[artifact路径1, artifact路径2, ...]"
ArtifactManifest 表: "artifact路径" -> "1"
```

### 命名规范

| 概念 | 命名 | 说明 |
| --- | --- | --- |
| jar 包 | `Artifact` | Maven 构件，符合行业术语 |
| 类查找 | `resolve` | 解析类位置并返回源码 |
| 检查 jar 是否包含类 | `probe` | 探测构件 |
| 构建索引 | `catalog` | 编目/索引构件 |
| 加载 jar 到缓存 | `hydrate` | 填充缓存 |
| 版本差异 | `SourceRevision` | 源码修订版本 |
| 待写入条目 | `PendingWrite` | 写缓冲条目 |
| 写缓冲刷新 | `WriteBufferFlusher` | 异步批量写入 |

---

## ClassRegistry 索引层

### 三层查询架构

```jsx
resolve(StringUtils)
       ↓
① 查 SourceCache（已反编译的源码）
       ↓ miss
② 查 ClassRegistry（class → artifact 映射）
       ↓ hit
③ hydrate 对应 artifact，无需重新扫描
```

### 收益分析

| 场景 | 无索引 | 有索引 |
| --- | --- | --- |
| 首次查询 | 扫描 ~50 jar | 扫描 ~50 jar（相同） |
| 二次查询（cache miss） | 重新扫描 ~500ms | **查 index → load jar ~10ms** |

### 索引构建时机

| 时机 | 行为 |
| --- | --- |
| 首次启动 | 全量扫描 .m2，构建 class_index |
| load jar | 解析 jar 内所有 .class，更新 index |
| 增量更新 | 检测 .m2 变化，增量更新 index |

### 实现代码

```rust
/// 编目 artifact（类名 → artifact 路径）
fn catalog(artifact_path: &Path) -> Result<HashMap<String, Vec<String>>> {
    let file = File::open(artifact_path)?;
    let mmap = unsafe { Mmap::map(&file)? };
    let mut archive = ZipArchive::new(Cursor::new(&mmap[..]))?;
    
    let mut registry: HashMap<String, Vec<String>> = HashMap::new();
    
    for i in 0..archive.len() {
        let entry = archive.by_index(i)?;
        let name = entry.name();
        
        // 只索引 .class 文件，排除内部类
        if name.ends_with(".class") && !name.contains('$') {
            let class_name = name
                .trim_end_matches(".class")
                .replace('/', ".");
            
            registry.entry(class_name)
                .or_default()
                .push(artifact_path.to_string_lossy().to_string());
        }
    }
    
    Ok(registry)
}

/// 解析类（三层查询）
fn resolve(class_name: &str) -> Result<ResolveResult> {
    // 1. 查 SourceCache
    if let Some(source) = source_cache.get(class_name)? {
        return Ok(source);
    }
    
    // 2. 查 ClassRegistry
    if let Some(artifacts) = class_registry.lookup(class_name)? {
        // 直接 hydrate 这些 artifact，无需扫描
        for artifact in artifacts {
            hydrate(&artifact)?;
        }
        return source_cache.get(class_name);
    }
    
    // 3. 兆底：全量扫描（新类/新 artifact）
    scan_and_catalog(class_name)?;
    source_cache.get(class_name)
}
```

---

## WriteBuffer 写缓冲设计

### 设计思路

```jsx
resolve 完成
       ↓
① 顺序写 WriteBuffer（追加写，极快）
       ↓
② 立即返回用户
       ↓ 后台异步
③ WriteBufferFlusher 批量刷入 SourceCache
```

### 收益

| 对比 | 直接写 SourceCache | WriteBuffer 模式 |
| --- | --- | --- |
| 写入方式 | 随机写（B+树） | **顺序追加** |
| 用户等待 | 等写完 | **立即返回** |
| 批量优化 | 无 | **合并多次写入** |
| 吞吐量 | 低 | **高** |

### 数据流

```jsx
┌─────────────┐    顺序写    ┌─────────────┐    flush    ┌─────────────┐
│  Resolver   │ ────────▶ │ WriteBuffer │ ────────▶ │ SourceCache │
└─────────────┘   (channel)  └─────────────┘  (Flusher)   └─────────────┘
      │                                                      
      ▼                                                      
   立即返回用户
```

### 实现代码

```rust
use std::sync::mpsc::{channel, Sender, Receiver};

/// 待写入条目
struct PendingWrite {
    class_name: String,
    artifact_path: String,
    source: String,
}

/// 写入 WriteBuffer（主线程，非阻塞）
fn enqueue(tx: &Sender<PendingWrite>, entry: PendingWrite) {
    tx.send(entry).ok(); // 顺序写入 channel，不阻塞
}

/// WriteBufferFlusher（后台线程）
fn flusher(rx: Receiver<PendingWrite>, source_cache: Arc<SourceCache>) {
    let mut batch = Vec::with_capacity(100);
    
    loop {
        // 收集一批
        while let Ok(entry) = rx.try_recv() {
            batch.push(entry);
            if batch.len() >= 100 {
                break;
            }
        }
        
        // 批量刷入 SourceCache
        if !batch.is_empty() {
            source_cache.batch_put(&batch).ok();
            batch.clear();
        }
        
        // 等待更多数据
        if let Ok(entry) = rx.recv_timeout(Duration::from_millis(50)) {
            batch.push(entry);
        }
    }
}

/// 解析流程
fn resolve(class_name: &str, write_buffer: &Sender<PendingWrite>) -> Result<ResolveResult> {
    // ... 反编译得到结果 ...
    
    // 写入 WriteBuffer（不阻塞）
    enqueue(write_buffer, PendingWrite {
        class_name: class_name.to_string(),
        artifact_path: artifact_path.to_string(),
        source: source.clone(),
    });
    
    // 立即返回
    Ok(result)
}
```

---

## 命令行接口

### 基础命令

```bash
# 查找类
class-finder find <类全限定名> [OPTIONS]

# 预加载 jar
class-finder load <jar路径>

# 缓存统计
class-finder stats

# 清除缓存
class-finder clear
```

### Find 命令参数

```bash
OPTIONS:
    -f, --format <FORMAT>    输出格式 [json|text|code] (默认: json)
    --code-only              仅输出源码（方便 grep）
    --method <NAME>          过滤指定方法
    -v, --version <VER>      指定版本号
    -o, --output <FILE>      输出到文件
```

### 输出格式

| 格式 | 说明 | 用途 |
| --- | --- | --- |
| `json` | JSON 结构化输出 | AI 解析、jq 提取 |
| `code` | 纯源码输出 | grep 过滤 |
| `text` | 简洁摘要 | 快速查看 |

### 管道组合示例（AI Agent 常用）

```bash
# 提取 isEmpty 方法实现
class-finder find StringUtils --code-only | grep -A10 "isEmpty"

# 提取所有 public 方法签名
class-finder find StringUtils --code-only | grep "public static"

# 统计方法数量
class-finder find StringUtils --code-only | grep -c "public "

# 提取字段定义
class-finder find MyEntity --code-only | grep -E "private|protected"

# jq 提取 jar 路径
class-finder find StringUtils | jq '.versions[0].jar_path'

# 多类查找合并
for cls in StringUtils ArrayUtils; do
  class-finder find "org.apache.commons.lang3.$cls" --code-only
done | grep "public static"
```

---

## 依赖安装

```bash
# 安装 CFR 反编译器
mkdir -p ~/.local/lib
curl -L -o ~/.local/lib/cfr.jar https://github.com/leibnitz27/cfr/releases/download/0.152/cfr-0.152.jar

# 安装 Rust（如未安装）
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

---

## Agent Skill 定义

```yaml
name: class-finder
description: |
  查找 Java 类所在的 jar 包并返回反编译源码。
  支持多版本对比，自动检测差异。
  支持管道过滤，可结合 grep/jq 提取关心内容。

usage: class-finder find <类全限定名> [OPTIONS]

usage_examples:
  - 查找类并返回 JSON:
    class-finder find org.apache.commons.lang3.StringUtils
  
  - 提取某方法实现:
    class-finder find StringUtils --code-only | grep -A10 "isEmpty"
  
  - 统计方法数:
    class-finder find StringUtils --code-only | grep -c "public "
  
  - 指定版本查找:
    class-finder find StringUtils --version 3.12.0
  
  - jq 提取字段:
    class-finder find StringUtils | jq '.versions[0].jar_path'

output: |
  JSON 格式（默认），包含：
  - class_name: 类全限定名
  - versions: 所有版本的源码和 jar 路径
  - has_diff: 是否存在版本差异
  - diff_summary: 差异摘要
  - recommendation: AI 决策建议
  
  纯代码格式（--code-only）：
  - 直接输出 Java 源码，方便 grep 过滤
```

---

## 风险与注意事项

| 风险 | 应对 |
| --- | --- |
| 首次扫描 .m2 慢 | 并行 + 后台预热 |
| jar 过多内存爆 | redb 持久化，不全加载到内存 |
| CFR 反编译失败 | 捕获异常，返回错误信息 |
| 多版本过多 | 限制最多返回 10 个版本 |

---

## 参考资源

- [CFR Decompiler](https://github.com/leibnitz27/cfr)
- [redb](https://github.com/cberner/redb)
- [rayon](https://github.com/rayon-rs/rayon)
- [similar](https://github.com/mitsuhiko/similar)

[Load 流程设计](https://www.notion.so/Load-92139d4f1dd64f2fae0650fff196777c?pvs=21)