# class-finder

在本地 Maven 仓库（`~/.m2/repository`）中查找 Java 类所在的 jar，并返回反编译后的源码。

运行时会自动管理反编译器（CFR）与缓存（redb），用户不需要关心它们的存放位置。

## 安装

### 一键安装（推荐）

- Linux / macOS：

```bash
curl -fsSL https://github.com/Groos-dev/class-finder/releases/latest/download/install.sh | sh
```

- Windows（PowerShell）：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -Command "irm https://github.com/Groos-dev/class-finder/releases/latest/download/install.ps1 | iex"
```

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

- 纯文本摘要：

```bash
class-finder org.springframework.stereotype.Component --format text
```

### 4）指定版本（从 maven 路径解析版本号）

```bash
class-finder org.springframework.stereotype.Component --version 6.2.8 --code-only
```

## 缓存

- 查看缓存统计：

```bash
class-finder stats
```

- 清空缓存：

```bash
class-finder clear
```

第一次查询会较慢（需要扫描 jar 并反编译），后续查询命中本地缓存会显著加速（可用 `class-finder stats` 查看缓存路径与统计）。

## 常见问题

### 找不到类

- 确认该类对应的依赖已下载到 `~/.m2/repository`
- 对 ClassName 查询：如果同名类很多，可能会优先选择“出现次数最多”的全限定名

### CFR 下载失败

如果首次运行自动下载失败，可以临时使用：

```bash
class-finder --cfr /path/to/cfr.jar org.springframework.stereotype.Component
```

## 开发与测试

```bash
cargo test
```
