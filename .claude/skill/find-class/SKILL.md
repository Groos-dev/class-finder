---
name: "find-class"
description: "当无法在工作目录查到某个类的实现时，用 class-finder 从本地 Maven 仓库的 jar 中查询并输出该类源码。需要定位 jar 类实现时调用。"
---

# find-class

## 何时使用
- 当无法在工作目录查到某个类的实现时，可以通过这个 Skill 查询 jar 中的类实现
- 当你只知道 `ClassName`（不含包名），希望自动在 jar 中探测并推断全限定名（FQN）
- 当你需要限定依赖版本来定位某个类实现时

## 前置条件
- 需要能执行 `class-finder` 命令

### 如果系统里没有 class-finder
- macOS / Linux：

```bash
curl -fsSL https://github.com/Groos-dev/class-finder/releases/latest/download/install.sh | sh
```

- Windows（PowerShell）：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -Command "$s=irm https://github.com/Groos-dev/class-finder/releases/latest/download/install.ps1; & ([scriptblock]::Create($s))"
```

可选参数：
- 安装目录（macOS/Linux）：`INSTALL_DIR="$HOME/.local/bin" ... | sh`
- 指定版本（macOS/Linux）：`VERSION=v0.0.1-beta.4 ... | sh`
- 指定版本（Windows）：`powershell ... -Command \"...\"` 并追加 `-Version 'v0.0.1-beta.4'`

## 使用规范（推荐默认）
1. 优先使用全限定名（FQN）查询，并输出源码：

```bash
class-finder org.springframework.stereotype.Component --code-only
```

2. 如果只知道类名（ClassName），直接查（会在 jar 内按 `*/ClassName.class` 探测并推断 FQN）：

```bash
class-finder Component --code-only
```

3. 想要摘要而不是源码：

```bash
class-finder org.springframework.stereotype.Component --format text
```

4. 想限定版本：

```bash
class-finder org.springframework.stereotype.Component --version 6.2.8 --code-only
```

5. 想缩小扫描范围（加速、减少误命中），把 `--m2` 指到某个 groupId 目录：

```bash
class-finder --m2 ~/.m2/repository/org/springframework Component --format text
```

## 常见失败与排查
- 找不到类：确认该依赖已下载到本地 Maven 仓库；必要时用 `--m2` 缩小范围。
- CFR 下载失败：用 `--cfr /path/to/cfr.jar` 指定本地 CFR。
- 同名类冲突：使用 FQN 或 `--version` 精确定位。
