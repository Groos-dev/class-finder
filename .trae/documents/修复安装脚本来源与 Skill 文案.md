## 问题原因定位
- 你执行的是 `curl ...raw.githubusercontent.com/.../main/install.sh | sh`，这会拿到 **main 分支** 的安装脚本。
- 我最近的安装增强（安装 Skill、预装 CFR、SHA256 校验）是在 `release/0.0.1-beta` 分支上发布的，并随 tag `v0.0.1-beta.3` 打包进 Release。
- 所以你看到的输出只包含“下载二进制+拷贝到 ~/.local/bin”，没有“校验/预装 CFR/安装 Skill”的日志。

## 目标
- 用户用一条命令安装时：
  - 安装 class-finder 二进制
  - 预下载 cfr.jar
  - 安装 find-class Skill 到 `~/.claude/skill/find-class/SKILL.md`
- 同时修正 Skill 的“何时使用”描述为你给的语义。

## 变更方案
### 1) 调整 README 的一键安装命令（关键）
- **稳定版**：改为从 Release 资产下载安装脚本（永远对应最新稳定版）
  - `https://github.com/Groos-dev/class-finder/releases/latest/download/install.sh`
  - `https://github.com/Groos-dev/class-finder/releases/latest/download/install.ps1`
- **指定版本/测试版**：改为从对应 tag 的 Release 资产下载安装脚本（保证脚本与该版本一致）
  - `https://github.com/Groos-dev/class-finder/releases/download/v0.0.1-beta.3/install.sh`
  - `https://github.com/Groos-dev/class-finder/releases/download/v0.0.1-beta.3/install.ps1`
- 这样用户不会再因为 main 分支脚本落后而“没装 Skill”。

### 2) 同步 main 分支的安装脚本能力
- 把 `release/0.0.1-beta` 上的安装增强同步到 main（至少保证 raw main 也能安装 Skill/CFR）。

### 3) 修正 Skill 文案与触发条件
- 更新 `.claude/skill/find-class/SKILL.md`：
  - “何时使用”改成你给的表述：
    - 当无法在工作目录查到某个类的实现时，通过该 Skill 查询 jar 中的类实现
  - 同时把示例聚焦在“查 jar 源码/反编译结果”这一件事上。

### 4) 验证与发布
- 本地验证：
  - 运行 `install.sh`（从 tag 的 Release 资产下载）后确认：
    - `~/.claude/skill/find-class/SKILL.md` 存在
    - macOS：`~/Library/Application Support/class-finder/tools/cfr.jar` 存在
  - `class-finder --help` 可运行
- 发布：打新 tag（如 `v0.0.1-beta.4`）让 Release 带上最新 installer + Skill 文案。

确认后我会按以上 4 步修改 README、同步 main、更新 SKILL.md，并发布 `v0.0.1-beta.4` 供你重新安装测试。