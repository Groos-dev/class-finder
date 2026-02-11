# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**class-finder** is a high-performance Rust CLI tool that finds Java classes in local Maven repositories (`~/.m2/repository`) and returns decompiled source code using CFR. It features persistent caching with LMDB (via heed), class-to-JAR indexing, background warmup of frequently accessed JARs, and hotspot tracking for performance optimization.

## Build & Development Commands

### Build
```bash
cargo build --release
```
Binary output: `target/release/class-finder`

### Run Tests
```bash
cargo test
```

### Run Specific Test
```bash
cargo test phase2_three_layer_flow_works
```

### Lint & Format
```bash
cargo clippy
cargo fmt
```

### Clean Build Artifacts
```bash
cargo clean
```

## Architecture Overview

The codebase is organized into modular layers with clear separation of concerns:

### Core Data Layer
- **cache.rs**: Persistent storage using LMDB (via heed) with ACID guarantees. Manages 6 tables:
  - `CLASSES_TABLE`: Decompiled class sources (key: `"ClassName::jar_path"`)
  - `JARS_TABLE`: JAR load status tracking
  - `CLASS_REGISTRY_TABLE`: Class-to-JAR mappings for fast lookups
  - `ARTIFACT_MANIFEST_TABLE`: Cataloged JAR tracking
  - `JAR_HOTSPOT_TABLE`: Access frequency tracking
  - `JAR_MTIME_TABLE`: File modification times for incremental indexing

- **registry.rs**: `ClassRegistry` provides class-to-artifact lookups. Queries `CLASS_REGISTRY_TABLE` to find which JARs contain a given fully-qualified class name.

### JAR Discovery & Inspection
- **scan.rs**: Parallel JAR discovery using `ignore` crate's `WalkBuilder`. Converts Maven package names to filesystem paths (e.g., `org.springframework` → `org/springframework`).

- **probe.rs**: JAR inspection utilities. `jar_contains_class()` checks if a specific class exists in a JAR without full decompilation.

- **catalog.rs**: Extracts complete class lists from JARs. Used during indexing to populate the registry.

### Decompilation & Parsing
- **cfr.rs**: CFR decompiler integration. Handles downloading CFR if missing, executing decompilation, and managing the decompiler lifecycle.

- **parse.rs**: Parses CFR output to extract individual class sources. Handles multi-class decompilation results and separates inner classes.

### Performance Optimization
- **buffer.rs**: `WriteBuffer` batches database writes with configurable batch size (default 100) and flush interval (default 50ms). Uses a background thread to avoid blocking main thread on I/O.

- **warmup.rs**: `Warmer` maintains a priority queue of warmup tasks executed by a thread pool. Two modes:
  - `TopLevelOnly`: Fast, decompiles only top-level classes
  - `AllClasses`: Thorough, includes inner classes
  - Coordinates with `HotspotTracker` to identify high-frequency JARs

- **hotspot.rs**: `HotspotTracker` records class access patterns and identifies which JARs should be preloaded. Tracks access frequency and marks JARs as "warmed" after preloading.

### CLI & Configuration
- **cli.rs**: Command definitions using clap derive macros. Supports: `find`, `load`, `warmup`, `index`, `stats`, `clear`.

- **config.rs**: Path resolution for Maven repo, CFR binary, and database. Respects `--m2`, `--cfr`, `--db` flags and environment variables.

- **main.rs**: Entry point orchestrating all components. Implements implicit `find` command (e.g., `class-finder ClassName` → `class-finder find ClassName`).

### Incremental Indexing
- **incremental.rs**: Tracks file modification times to avoid re-indexing unchanged JARs.

## Key Data Flow Patterns

### Find Command Flow
1. **Normalize input**: Strip `import` keyword, semicolons, whitespace
2. **Resolve class location**:
   - If FQN (contains `.`): Use registry lookup first, fall back to scan
   - If simple name: Scan all JARs, find all matches, select most common FQN
3. **Decompile**: Call CFR for each matched JAR version
4. **Cache write**: Enqueue to `WriteBuffer` (batched, async)
5. **Hotspot tracking**: Record access for warmup prioritization
6. **Output**: JSON (default), text, or code-only format

### Index Command Flow
1. Scan all JARs in Maven repo
2. For each uncataloged JAR: Extract class list via `catalog()`
3. Update `CLASS_REGISTRY_TABLE` with class→JAR mappings
4. Mark JAR as cataloged in `ARTIFACT_MANIFEST_TABLE`

### Warmup Command Flow
1. Identify target JARs (via `--hot`, `--group`, or explicit path)
2. For each JAR: Call `load_jar()` to decompile all classes
3. Batch writes to cache via `WriteBuffer`
4. Mark JARs as warmed in hotspot tracker

## Important Implementation Details

### Class Name Resolution
- **FQN queries** (e.g., `org.springframework.stereotype.Component`): Direct registry lookup, then scan if not found
- **Simple name queries** (e.g., `Component`): Scan all JARs, find all matches, select FQN with highest occurrence count
- **Version filtering**: Applied after finding all matches; sorts by version

### Cache Key Format
```
"{fully_qualified_class_name}::{jar_path}"
```
Example: `"org.springframework.stereotype.Component::/Users/user/.m2/repository/org/springframework/spring-context/6.2.8/spring-context-6.2.8.jar"`

### Write Buffering Strategy
- Batches writes to reduce transaction overhead
- Background thread flushes every 50ms or when batch reaches 100 entries
- Pending count tracked via atomic counter for monitoring

### Hotspot Tracking
- Records access to each JAR
- Tracks which JARs have been "warmed" (fully decompiled)
- Prioritizes high-frequency JARs for background preloading
- Prevents re-warming already-processed JARs

## Testing

Integration tests in `tests/phase2_integration.rs` use:
- Temporary directories for isolation
- Fake JAR files created with `zip` crate
- Mock CFR binary (shell script) for deterministic output
- JSON output validation

Run tests with: `cargo test`

## Common Development Tasks

### Adding a New Command
1. Add variant to `Commands` enum in `cli.rs`
2. Add match arm in `main.rs` to handle the command
3. Implement command logic (typically in `main.rs` or new module)
4. Add integration test in `tests/phase2_integration.rs`

### Modifying Cache Schema
1. Add new `TableDefinition` constant in `cache.rs`
2. Initialize table in `PersistentCache::open()`
3. Add accessor methods to `PersistentCache`
4. Update tests to verify schema changes

### Optimizing Performance
- Profile with `cargo flamegraph` or `perf`
- Check `WriteBuffer` batch size and flush interval in `buffer.rs`
- Verify `Warmer` thread pool size in `warmup.rs` (default: 2 concurrent)
- Consider incremental indexing via `incremental.rs` for large repos

## Dependencies

Key crates:
- **heed**: LMDB bindings used for persistent cache
- **clap**: CLI argument parsing with derive macros
- **rayon**: Data parallelism for JAR scanning and filtering
- **zip**: JAR file reading
- **serde/serde_json**: Serialization for cache and output
- **sha2**: Content hashing for decompiled sources
- **dirs**: Platform-specific directory resolution
- **ignore**: Efficient parallel directory walking

## Error Handling

- Uses `anyhow::Result<T>` throughout for ergonomic error propagation
- Context added via `.with_context()` for user-friendly error messages
- Chinese error messages in user-facing output
- Errors in warmup/load operations don't halt entire operation (continue on individual failures)

## Configuration Paths

Default locations (platform-specific via `dirs` crate):
- **Database**: `~/.local/share/class-finder/db.lmdb` (Linux), `~/Library/Application Support/class-finder/db.lmdb` (macOS)
- **CFR**: `~/.local/share/class-finder/tools/cfr.jar` (Linux), `~/Library/Application Support/class-finder/tools/cfr.jar` (macOS)
- **Maven repo**: `~/.m2/repository` (standard Maven location)

Override with `--db`, `--cfr`, `--m2` flags or environment variables.
