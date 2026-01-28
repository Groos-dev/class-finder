## 目标与范围（MVP）
- 落地一个可编译、可运行的 Rust CLI：`class-finder`
- 支持最小闭环：在 `~/.m2/repository` 扫 jar → 命中包含目标类的 jar → 调用 CFR 反编译 → 输出源码（JSON/纯代码）
- 提供最小缓存能力（redb）：首次慢、二次快；并带 `clear/stats` 便于验证

## 交付的命令（MVP）
- `class-finder find <类全限定名>`
  - 默认输出 JSON（包含 class_name、jar_path、version(可选解析)、source）
  - `--code-only` 仅输出源码（便于 grep）
  - `--format json|text|code`（先支持 json/code，text 做最简摘要）
  - 可选：`--m2 <PATH>`、`--cfr <PATH>`、`--db <PATH>`
- `class-finder load <jar路径>`
  - 反编译整个 jar、解析为多类、批量写入缓存，并标记 jar 已加载（对应 load-flow.md）
- `class-finder stats`：输出缓存条目数、已加载 jar 数
- `class-finder clear`：清空/重建数据库

## 核心实现设计（按现有文档落地）
- 工程骨架
  - 新建 Cargo 工程：`Cargo.toml` + `src/main.rs` + `src/lib.rs`
  - 模块拆分：`cli`、`scan`（.m2 扫描）、`probe`（jar_contains_class）、`cfr`（反编译器封装）、`parse`（解析 CFR 输出）、`cache`（redb）
- 扫描与命中
  - `infer_scan_paths`：按类名前缀缩小扫描范围（plan.md 的推断逻辑）
  - `scan_jars`：用 `ignore::WalkBuilder` 并行枚举 jar（plan.md）
  - `jar_contains_class`：用 `memmap2 + zip` 只读目录判断是否存在 `org/.../Foo.class`（plan.md）
- CFR 调用（单类/整 jar）
  - 单类反编译：使用 CFR 官方建议的 classpath 方式：`java -jar cfr.jar --extraclasspath <jar> <FQN>`（依据 CFR 官方说明：可用“全限定名 + extraclasspath”反编译）
  - 整 jar：`java -jar cfr.jar <jar> --silent true --comments false`（对应 load-flow.md）
- 解析
  - 先实现稳定切分：按 `/*\n * Decompiled` 分割，提取 package + class/interface/enum 名（load-flow.md）
  - 计算 `content_hash`（MVP 用 SHA-256 或 blake3；最终选型以仓库依赖最少为准）
- 缓存（redb）
  - 表：`CLASSES_TABLE`（key=`FQN::jar_path` → 源码）、`JARS_TABLE`（jar_path → 1）
  - `find`：先查 `CLASSES_TABLE`，命中直接返回；未命中再扫描/反编译并写入
  - `load`：若 `JARS_TABLE` 已有则跳过，否则全量反编译并批量写入

## 验证方式（实现后立即做）
- 单元测试
  - `infer_scan_paths`：不同类名输入的路径选择逻辑
  - `jar_contains_class`：在测试里生成一个 zip/jar（只需包含伪 `.class` 文件名），验证命中逻辑
  - `parse_decompiled_output`：用文档中的 CFR 输出样例验证能拆出多个类
- 集成验证（非强制测试）
  - 若本机存在 `java` 且提供 `--cfr` 路径：执行一次 `find`（例如 `org.apache.commons.lang3.StringUtils`）确认能输出源码；再执行第二次确认缓存命中速度提升

## 约定与默认值
- 默认 `.m2`：`~/.m2/repository`
- 默认 DB：`~/.cache/class-finder/db.redb`（或项目内 `.class-finder/db.redb`，实现时选更不扰动用户工程的一种）
- CFR jar 路径：优先 `--cfr`，其次环境变量 `CFR_JAR`，最后报错提示如何安装（引用 plan.md 的安装段落）

确认后我会：创建 Rust 工程、实现上述命令与模块、补齐测试并在本机跑通构建与单测。