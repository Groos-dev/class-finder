# AGENTS.md

Agent guide for `/Users/groos/repo/rust/class-finder`.
Follow repository conventions over generic Rust habits.

## 0) Priority And Scope

- Instruction priority:
  1. `AGENTS.md`
  2. `CLAUDE.md`
  3. `CONTRIBUTING.md`
  4. `README.md` / `README.en.md`
- If rules conflict, follow higher priority.
- Rule files check (as requested):
  - `.cursorrules`: not present
  - `.cursor/rules/`: not present
  - `.github/copilot-instructions.md`: not present

## 1) Project Snapshot

- `class-finder` is a Rust CLI for locating Java classes in local Maven repo and returning CFR-decompiled source.
- Core modules under `src/`:
  - `cache`, `registry`, `scan`, `probe`, `catalog`, `cfr`, `parse`
  - `buffer`, `warmup`, `hotspot`, `incremental`, `cli`, `config`, `main`
- Core characteristics:
  - persistent cache via `LMDB` (`heed`)
  - class-to-jar registry + indexing
  - background buffered writes
  - warmup/hotspot optimization

## 2) Build / Lint / Test Commands

Use these commands by default.

### Build
```bash
cargo build --release
```

### Test (all)
```bash
cargo test
```

### Test (single case)
```bash
cargo test phase2_three_layer_flow_works
```

Optional when debugging test output:
```bash
cargo test phase2_three_layer_flow_works -- --nocapture
```

### Format
```bash
cargo fmt --all
```

### Lint
```bash
cargo clippy --all-targets --all-features -- -D warnings
```

### Clean
```bash
cargo clean
```

## 3) Agent Workflow

- Default change flow:
  1. Implement focused change.
  2. `cargo fmt --all`.
  3. `cargo clippy --all-targets --all-features -- -D warnings`.
  4. Run tests (targeted first if isolated; full suite for broader impact).
- For risky/cross-module edits, run full `cargo test` before handoff.

## 4) Code Style Conventions

Conventions are inferred from `src/` and `tests/`.

### 4.1 Imports

- Prefer explicit `use`; avoid wildcard imports.
- Keep imports grouped logically (external crates / std / local crate).
- Let `rustfmt` decide final order.
- References: `src/main.rs`, `src/cache.rs`, `src/cli.rs`.

### 4.2 Naming

- Types/enums: `PascalCase` (`PersistentCache`, `OutputFormat`).
- Functions/vars/modules: `snake_case` (`normalize_class_name`, `warmup_targets`).
- Constants: `UPPER_SNAKE_CASE` (`CLASSES_TABLE`, `JAR_MTIME_TABLE`).
- CLI subcommands are concise verbs/nouns: `find`, `load`, `warmup`, `index`, `stats`, `clear`.

### 4.3 Types And Data Shapes

- Prefer concrete structs for command output/internal deps.
  - Example: `FindResult`, `LoadResult`, `WarmupResult`, `FindDeps` in `src/main.rs`.
- Prefer typed enums (`ValueEnum`) instead of stringly flags for CLI options.
- Keep serialization explicit with `#[derive(Serialize)]` / `#[derive(Deserialize)]`.

### 4.4 Error Handling

- Standard result type is `anyhow::Result<T>` across modules.
- Add context at IO/process boundaries with `.context(...)` / `.with_context(...)`.
- Preserve current user-facing tone: many runtime error messages are Chinese.
- Do not silently swallow errors unless existing code intentionally degrades.
- Avoid `unwrap()` / `expect()` in production paths.
  - Existing unwraps are mostly in tests or controlled helper code.

### 4.5 Control Flow And Output

- Keep top-level command dispatch explicit in `main` via `Commands` match arms.
- Prefer early return for empty/no-op paths (e.g., empty batch writes).
- Keep CLI output structured JSON by default unless user requests another format.

### 4.6 Concurrency And Performance

- Reuse existing patterns before adding new abstractions:
  - `WriteBuffer` + background flusher
  - `Arc` + atomics + `std::sync::mpsc`
  - `rayon` parallel iterators for scan/filter work
- Preserve graceful shutdown behavior:
  - `shutdown_and_flush` for buffer
  - `shutdown_and_drain` for warmer
- Avoid adding blocking work inside hot loops without clear justification.

### 4.7 Module Boundaries

- Keep responsibilities aligned with existing architecture:
  - scanning in `scan`
  - probing in `probe`
  - persistence in `cache` / `registry`
  - warmup policy in `warmup` / `hotspot`
  - CLI definition in `cli`
  - orchestration in `main`
- Prefer extending nearest module over creating a new module.

## 5) Testing Conventions

- Main integration flow is in `tests/phase2_integration.rs`.
- Tests frequently use temp directories, fake jars, and mocked `java` binaries.
- Keep tests deterministic:
  - controlled temp paths
  - explicit env setup
  - stable assertions over JSON fields
- Prefer behavior-based test names (e.g., `phase2_three_layer_flow_works`).

## 6) Security And Ops Hygiene

- Avoid logging sensitive local paths/env details unless explicitly required.
- Prefer explicit path flags (`--m2`, `--cfr`, `--db`) when adding behavior.
- For external process calls (`java`, `curl`), include clear error context.

## 7) Change Scope Rules

- Keep changes minimal and task-focused.
- Do not refactor unrelated areas during bugfixes.
- Do not casually change JSON output fields: downstream tools may depend on them.
- If CLI surface changes, also update:
  - `src/cli.rs`
  - command handling in `src/main.rs`
  - docs in `README.md` and `README.en.md`
  - tests

## 8) Pre-Handoff Checklist

- `cargo fmt --all`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test`
- If behavior/CLI changed, docs and tests are updated accordingly.

## 9) Agent Notes

- Read nearby code before editing to match established patterns.
- Maintain existing language style in touched files.
- Keep commits atomic when user asks for commit.
- Do not add placeholder TODO behavior in production code unless explicitly requested.
