## 目标（仅 macOS）
- 生成 README.md：用户只需 `class-finder <ClassName|ClassFull>`，不关心 CFR/redb 位置。
- 调整默认目录：所有运行时文件默认放在 `~/.class-finder/`。
- 生成 macOS 可分发的打包文件（tar.gz）与安装脚本。

## CLI 体验（用户侧）
- 直接查类（隐式 find）：
  - `class-finder org.springframework.stereotype.Component --code-only`
  - `class-finder Component --code-only`
- 保留子命令：`find/load/stats/clear` 仍可用（README 以“直接查类”为主）。
- 支持从 IDE 粘贴：`import org.foo.Bar;`（自动去掉 import/空格/分号）。

## 实现改动（代码层）
- **隐式 find**：在 clap 解析前重写 argv：
  - 找到第一个“非 - 开头”的参数；若它不是 `find/load/stats/clear/help`，就在该位置插入 `find`。
- **ClassName 支持**：
  - 输入不含 `.` 时：用 mmap+zip 遍历 jar entry 名，匹配 `*/<ClassName>.class`，并排除包含 `$` 的内部类。
  - 输入含 `.` 时：继续使用当前精确路径探测（`org/.../Foo.class`）。

## 默认路径（用户无需传参）
- 基准目录：`~/.class-finder/`
  - DB：`~/.class-finder/db.redb`
  - CFR：`~/.class-finder/tools/cfr.jar`
- CFR 缺失时自动安装（macOS）：
  - 通过 `curl -L` 下载 CFR 0.152 到上述路径（并在 stderr 打一行提示）。
  - 仍允许高级用户用 `--cfr/--db` 或 `CFR_JAR` 覆盖，但 README 不强调。

## README.md（中文）
- 快速开始（两种输入：ClassFull / ClassName）
- 输出说明：`--code-only` / `--format json|text` / `--version`
- 缓存说明：默认目录 `~/.class-finder/`、`stats/clear`
- 常见问题：首次慢二次快；找不到类；Spring 多版本选择

## 打包与安装（macOS）
- 新增：`scripts/package-macos.sh`
  - `cargo build --release`
  - 生成 `dist/class-finder-macos-<arch>.tar.gz`（包含 `class-finder` 二进制 + README）
- 新增：`scripts/install-macos.sh`
  - 将二进制复制到 `~/.local/bin/class-finder`（如不存在则创建目录）
  - 提示把 `~/.local/bin` 加入 PATH

## 验证（实现后执行）
- `cargo test`：补充 argv 重写与 ClassName 探测的单测
- 端到端（真实 .m2）：
  - `class-finder org.springframework.stereotype.Component --code-only`
  - `class-finder Component --code-only`
  - `class-finder stats` / `class-finder clear`
- 校验落盘：确认 `~/.class-finder/db.redb` 与 `~/.class-finder/tools/cfr.jar` 生效

确认后我会按该计划改代码、补 README 与脚本，并跑完上述端到端验证。