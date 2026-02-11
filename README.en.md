# class-finder

English | [简体中文](README.md)

Find Java classes in your local Maven repository (`~/.m2/repository`) and return decompiled source code.

Automatically manages the decompiler (CFR) and cache (LMDB via heed) at runtime - users don't need to worry about where they're stored.

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

Equivalent syntax:

```bash
class-finder org.springframework.stereotype.Component --format code
```

- Plain text summary:

```bash
class-finder org.springframework.stereotype.Component --format text
```

- Write output to file (parent directory is created automatically):

```bash
class-finder org.springframework.stereotype.Component --code-only --output /tmp/Component.java
```

### 4) Specify Version (parsed from Maven path)

```bash
class-finder org.springframework.stereotype.Component --version 6.2.8 --code-only
```

### 5) Common Global Options

- `--m2 <PATH>`: Maven repository root path (default: `~/.m2/repository`)
- `--db <FILE>`: cache DB file path (default: `class-finder/db.lmdb` under local data directory)
- `--cfr <FILE>`: local `cfr.jar` path
- `CFR_JAR`: if `--cfr` is not provided, this env var can point to `cfr.jar`

Example:

```bash
class-finder --m2 /data/m2 --db /data/class-finder.lmdb --cfr /tools/cfr.jar find org.example.Foo
```

### 6) Implicit `find` Rule

If no explicit subcommand is provided (`find/load/warmup/index/stats/clear`), `class-finder` treats the first non-global argument as `find` input.

These two are equivalent:

```bash
class-finder --db /tmp/cf.lmdb org.springframework.stereotype.Component
class-finder --db /tmp/cf.lmdb find org.springframework.stereotype.Component
```

## Advanced Features

### Index Building

Build a class-to-JAR mapping index to accelerate subsequent queries:

```bash
class-finder index
```

Specify scan path:

```bash
class-finder index --path /path/to/maven/repo
```

### Manual JAR Loading

Manually load a specific JAR file, parse all classes and cache them:

```bash
class-finder load /path/to/your.jar
```

### Warmup System

Preload frequently used JARs and cache decompiled results in advance:

- Warmup most frequently accessed JARs:

```bash
class-finder warmup --hot
```

- Warmup all JARs from a specific Maven group:

```bash
class-finder warmup --group org.springframework
```

- Warmup top N hotspot JARs (requires `--hot`):

```bash
class-finder warmup --hot --top 10
```

- Limit warmup targets with `--limit`:

```bash
class-finder warmup --hot --top 50 --limit 10
```

- Warmup a specific JAR:

```bash
class-finder warmup /path/to/your.jar
```

Note: `warmup` requires one of the following:
- provide positional `JAR`
- or use `--hot`
- or use `--group <GROUP>`

## Cache Management

- View cache statistics:

```bash
class-finder stats
```

Output includes:
- Number of cached sources
- Number of indexed classes
- Number of loaded JARs
- Hotspot JAR statistics
- Warmup status

- Clear cache:

```bash
class-finder clear
```

### Concurrent Reads

- The storage backend is LMDB (via heed), and `index`, `load`, `warmup`, `find`, and `stats` all access the same main DB directly (default pathname `db.lmdb`).

The first query will be slower (needs to scan JARs and decompile), but subsequent queries will be significantly faster when hitting the local cache. Use `index` and `warmup` commands to build indexes and caches in advance for even faster queries.

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
