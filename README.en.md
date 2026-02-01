# class-finder

English | [简体中文](README.md)

Find Java classes in your local Maven repository (`~/.m2/repository`) and return decompiled source code.

Automatically manages the decompiler (CFR) and cache (redb) at runtime - users don't need to worry about where they're stored.

## Installation

### One-Click Install (Recommended)

- Linux / macOS:

```bash
curl -fsSL https://github.com/Groos-dev/class-finder/releases/latest/download/install.sh | sh
```

- Windows (PowerShell):

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -Command "$s=irm https://github.com/Groos-dev/class-finder/releases/latest/download/install.ps1; & ([scriptblock]::Create($s))"
```

The installation script automatically:
- Downloads the platform-specific release binary and verifies integrity with `SHA256SUMS`
- Pre-downloads CFR to the default data directory (avoiding download on first run):
  - macOS: `~/Library/Application Support/class-finder/tools/cfr.jar`
  - Linux: `~/.local/share/class-finder/tools/cfr.jar` (or `$XDG_DATA_HOME/class-finder/tools/cfr.jar`)
  - Windows: `%LOCALAPPDATA%\\class-finder\\tools\\cfr.jar`
- Installs the `find-class` Skill to:
  - macOS/Linux: `~/.claude/skills/find-class/SKILL.md`
  - Windows: `%USERPROFILE%\\.claude\\skills\\find-class\\SKILL.md`

#### Specify Version and Install Directory

- Linux / macOS (specify version):

```bash
VERSION=v0.0.1-beta.4 curl -fsSL https://github.com/Groos-dev/class-finder/releases/latest/download/install.sh | sh
```

- Linux / macOS (install to custom directory):

```bash
INSTALL_DIR="$HOME/.local/bin" curl -fsSL https://github.com/Groos-dev/class-finder/releases/latest/download/install.sh | sh
```

- Linux / macOS (install prerelease: auto-select latest beta/rc/alpha):

```bash
ALLOW_PRERELEASE=1 curl -fsSL https://github.com/Groos-dev/class-finder/releases/latest/download/install.sh | sh
```

- Windows (specify version):

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -Command "$s=irm https://github.com/Groos-dev/class-finder/releases/latest/download/install.ps1; & ([scriptblock]::Create($s)) -Version 'v0.0.1-beta.4'"
```

- Windows (install prerelease: auto-select latest beta/rc/alpha):

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -Command "$s=irm https://github.com/Groos-dev/class-finder/releases/latest/download/install.ps1; & ([scriptblock]::Create($s)) -AllowPrerelease"
```

#### Additional Parameters

- Install/update Skill only (specify ref): `SKILL_REF=main ... | sh` / `-SkillRef main`
- Override CFR download URL: `CFR_URL=... ... | sh` / `-CfrUrl ...`

### Build from Source

This is a Rust CLI project:

```bash
cargo build --release
```

The binary will be generated at:

```bash
target/release/class-finder
```

## Quick Start

### 1) Search by Fully Qualified Name (Recommended)

```bash
class-finder org.springframework.stereotype.Component --code-only
```

Equivalent syntax:

```bash
class-finder find org.springframework.stereotype.Component --code-only
```

### 2) Search by Simple Class Name

```bash
class-finder Component --code-only
```

Note: When the input doesn't contain `.`, it will probe JARs using the `*/Component.class` pattern and infer the fully qualified name (automatically excludes `$` inner classes).

### 3) Output Formats

- Default JSON output (convenient for AI / jq processing):

```bash
class-finder org.springframework.stereotype.Component
```

- Source code only (convenient for grep):

```bash
class-finder org.springframework.stereotype.Component --code-only
```

- Plain text summary:

```bash
class-finder org.springframework.stereotype.Component --format text
```

### 4) Specify Version (parsed from Maven path)

```bash
class-finder org.springframework.stereotype.Component --version 6.2.8 --code-only
```

## Cache

- View cache statistics:

```bash
class-finder stats
```

- Clear cache:

```bash
class-finder clear
```

The first query will be slower (needs to scan JARs and decompile), but subsequent queries will be significantly faster when hitting the local cache (use `class-finder stats` to view cache path and statistics).

## FAQ

### Class Not Found

- Confirm the dependency containing the class has been downloaded to `~/.m2/repository`
- For simple class name queries: if there are many classes with the same name, it may prioritize the fully qualified name that appears most frequently

### CFR Download Failed

If automatic download fails on first run, you can temporarily use:

```bash
class-finder --cfr /path/to/cfr.jar org.springframework.stereotype.Component
```

## Development and Testing

```bash
cargo test
```

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md)

## License

MIT, see [LICENSE](LICENSE)
