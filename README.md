# class-finder

[English](README.en.md) | 简体中文

在本地 Maven 仓库（`~/.m2/repository`）中查找 Java 类所在的 jar，并返回反编译后的源码。

运行时会自动管理反编译器（CFR）与缓存（LMDB via heed），用户不需要关心它们的存放位置。

## 安装

### 一键安装（推荐）

- Linux / macOS：

```bash
curl -fsSL https://github.com/Groos-dev/class-finder/releases/latest/download/install.sh | sh
```

- Windows（PowerShell）：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -Command "$s=irm https://github.com/Groos-dev/class-finder/releases/latest/download/install.ps1; & ([scriptblock]::Create($s))"
```

安装脚本会自动完成三件事：
- 下载对应平台的 Release 产物，并用 `SHA256SUMS` 做完整性校验
- 预下载 CFR 到默认数据目录（避免首次运行再下载）：
  - macOS：`~/Library/Application Support/class-finder/tools/cfr.jar`
  - Linux：`~/.local/share/class-finder/tools/cfr.jar`（或 `$XDG_DATA_HOME/class-finder/tools/cfr.jar`）
  - Windows：`%LOCALAPPDATA%\\class-finder\\tools\\cfr.jar`
- 安装 `find-class` Skill 到：
  - macOS/Linux：`~/.claude/skills/find-class/SKILL.md`
  - Windows：`%USERPROFILE%\\.claude\\skills\\find-class\\SKILL.md`

#### 指定版本与安装目录

- Linux / macOS（指定版本）：

```bash
VERSION=v0.0.1-beta.4 curl -fsSL https://github.com/Groos-dev/class-finder/releases/latest/download/install.sh | sh
```

- Linux / macOS（安装到指定目录）：

```bash
INSTALL_DIR="$HOME/.local/bin" curl -fsSL https://github.com/Groos-dev/class-finder/releases/latest/download/install.sh | sh
```

- Linux / macOS（安装预发布版本：自动选择最新 beta/rc/alpha）：

```bash
ALLOW_PRERELEASE=1 curl -fsSL https://github.com/Groos-dev/class-finder/releases/latest/download/install.sh | sh
```

- Windows（指定版本）：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -Command "$s=irm https://github.com/Groos-dev/class-finder/releases/latest/download/install.ps1; & ([scriptblock]::Create($s)) -Version 'v0.0.1-beta.4'"
```

- Windows（安装预发布版本：自动选择最新 beta/rc/alpha）：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -Command "$s=irm https://github.com/Groos-dev/class-finder/releases/latest/download/install.ps1; & ([scriptblock]::Create($s)) -AllowPrerelease"
```

#### 其他参数

- 只安装/更新 Skill（指定 ref）：`SKILL_REF=main ... | sh` / `-SkillRef main`
- CFR 下载地址覆盖：`CFR_URL=... ... | sh` / `-CfrUrl ...`

### 源码构建

本项目是 Rust CLI：

```bash
cargo build --release
```

生成的二进制在：

```bash
target/release/class-finder
```

## 快速开始

### 1）用全限定名查类（推荐）

```bash
class-finder org.springframework.stereotype.Component --code-only
```

等价写法：

```bash
class-finder find org.springframework.stereotype.Component --code-only
```

### 2）只知道类名（ClassName）也可以

```bash
class-finder Component --code-only
```

说明：当输入不包含 `.` 时，会在 jar 里按 `*/Component.class` 规则探测并推断出全限定名（会自动排除 `$` 内部类）。

### 3）输出格式

- 默认输出 JSON（便于 AI / jq 处理）：

```bash
class-finder org.springframework.stereotype.Component
```

- 仅输出源码（便于 grep）：

```bash
class-finder org.springframework.stereotype.Component --code-only
```

等价写法：

```bash
class-finder org.springframework.stereotype.Component --format code
```

- 纯文本摘要：

```bash
class-finder org.springframework.stereotype.Component --format text
```

- 输出到文件（自动创建父目录）：

```bash
class-finder org.springframework.stereotype.Component --code-only --output /tmp/Component.java
```

### 4）指定版本（从 maven 路径解析版本号）

```bash
class-finder org.springframework.stereotype.Component --version 6.2.8 --code-only
```

### 5）常用全局参数

- `--m2 <PATH>`：指定 Maven 仓库根目录（默认 `~/.m2/repository`）
- `--db <FILE>`：指定缓存 DB 文件路径（默认本地数据目录下 `class-finder/db.lmdb`）
- `--cfr <FILE>`：指定本地 `cfr.jar` 路径
- `CFR_JAR`：未传 `--cfr` 时，可用环境变量指定 `cfr.jar` 路径

示例：

```bash
class-finder --m2 /data/m2 --db /data/class-finder.lmdb --cfr /tools/cfr.jar find org.example.Foo
```

### 6）隐式 find 规则

如果你没有显式写子命令（`find/load/warmup/index/stats/clear`），`class-finder` 会把第一个非全局参数当作 `find` 的参数。

例如下面两条等价：

```bash
class-finder --db /tmp/cf.lmdb org.springframework.stereotype.Component
class-finder --db /tmp/cf.lmdb find org.springframework.stereotype.Component
```

## 高级功能

### 索引构建

构建类名到 JAR 的映射索引，加速后续查询：

```bash
class-finder index
```

指定扫描路径：

```bash
class-finder index --path /path/to/maven/repo
```

### 手动加载 JAR

手动加载指定 JAR 文件，解析所有类并缓存：

```bash
class-finder load /path/to/your.jar
```

### 预热系统

预热常用 JAR，提前缓存反编译结果：

- 预热访问频率最高的 JAR：

```bash
class-finder warmup --hot
```

- 预热指定 Maven group 的所有 JAR：

```bash
class-finder warmup --group org.springframework
```

- 预热前 N 个热点 JAR（需配合 `--hot`）：

```bash
class-finder warmup --hot --top 10
```

- 结合 `--limit` 截断本次预热目标数量：

```bash
class-finder warmup --hot --top 50 --limit 10
```

- 预热指定 JAR：

```bash
class-finder warmup /path/to/your.jar
```

说明：`warmup` 必须满足以下其一：
- 传入 `JAR` 位置参数
- 或使用 `--hot`
- 或使用 `--group <GROUP>`

## 缓存管理

- 查看缓存统计：

```bash
class-finder stats
```

输出包括：
- 缓存的源码数量
- 已索引的类数量
- 已加载的 JAR 数量
- 热点 JAR 统计
- 预热状态

- 清空缓存：

```bash
class-finder clear
```

### 并发读与快照

- 底层存储已切换为 LMDB（通过 heed）。
- `index` / `load` / `warmup` 等写操作会更新主库（默认路径名 `db.lmdb`），并在完成后发布一个只读快照（默认路径名 `db.snapshot.lmdb`）。
- `find` / `stats` 默认从只读快照读取，避免与写进程争抢主库写锁。
- 快照是最终一致的：读请求可能短时间内看不到最新写入，下一次快照发布后会可见。
- 如果你通过 `--db` 指定了自定义主库路径，快照路径会跟随该路径自动推导。

第一次查询会较慢（需要扫描 jar 并反编译），后续查询命中本地缓存会显著加速。使用 `index` 和 `warmup` 命令可以提前构建索引和缓存，进一步提升查询速度。

## 常见问题

### 找不到类

- 确认该类对应的依赖已下载到 `~/.m2/repository`
- 对 ClassName 查询：如果同名类很多，可能会优先选择"出现次数最多"的全限定名

### CFR 下载失败

如果首次运行自动下载失败，可以临时使用：

```bash
class-finder --cfr /path/to/cfr.jar org.springframework.stereotype.Component
```

## 开发与测试

```bash
cargo test
```

## 贡献

见 [CONTRIBUTING.md](CONTRIBUTING.md)

## License

MIT，见 [LICENSE](LICENSE)
