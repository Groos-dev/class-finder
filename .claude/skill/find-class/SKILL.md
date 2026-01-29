---
name: "find-class"
description: "用 class-finder 在本地 Maven 仓库定位 Java 类所在 jar 并输出反编译源码。用户要查类在哪个 jar、要源码、或只知道 ClassName 时调用。"
---

# find-class

## 何时使用
- 用户问“这个 Java 类在哪个 jar 里？”
- 用户要某个类的反编译源码（给我代码/实现细节）
- 用户只知道 `ClassName`（不含包名），希望自动推断 FQN
- 用户想限定某个版本（例如 Spring 6.2.8 的某个类）

## 前置条件
- 需要能执行 `class-finder` 命令

### 如果系统里没有 class-finder
- macOS / Linux：

```bash
curl -fsSL https://raw.githubusercontent.com/Groos-dev/class-finder/main/install.sh | sh
```

- Windows（PowerShell）：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -Command "irm https://raw.githubusercontent.com/Groos-dev/class-finder/main/install.ps1 | iex"
```

可选参数：
- 安装目录（macOS/Linux）：`INSTALL_DIR="$HOME/.local/bin" ... | sh`
- 指定版本（macOS/Linux）：`VERSION=v0.0.1-beta.2 ... | sh`
- 指定版本（Windows）：`powershell ... \"& { irm ... | iex }\" -Version v0.0.1-beta.2`

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
