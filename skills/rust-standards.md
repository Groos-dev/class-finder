# Rust Project Standards Skill

**Purpose**: Enforce consistent Rust project architecture, naming conventions, and code organization standards.

**Trigger**: Writing/modifying Rust code in Tauri projects or CLI tools.

---

## 📁 Project Structure Standards

### Tauri Backend Structure

```
src-tauri/src/
├── main.rs           (~100 lines) - Only Tauri command registration
├── lib.rs            - Module declarations
├── app.rs            - Application orchestration layer
├── config.rs          - Centralized configuration
├── error.rs           - Custom error types
├── models/            - Data models and DTOs
│   ├── mod.rs
│   └── ...
├── commands/           - Tauri IPC commands
│   ├── mod.rs
│   └── ...
├── services/           - Business logic layer
│   ├── mod.rs
│   └── ...
├── repositories/       - Data access layer
│   ├── mod.rs
│   └── ...
│   ├── db/
│   │   ├── mod.rs
│   │   ├── connection.rs
│   │   └── schema.rs
│   └── transactions/
│       ├── mod.rs
│       └── ...
└── utils/              - Shared utilities
    ├── mod.rs
    └── ...
```

### CLI Tool Structure

```
src/
├── main.rs           (~100 lines) - CLI entry point, command dispatch
├── lib.rs            - Module declarations
├── cli.rs            - CLI argument parsing
├── config.rs                   - Centralized configuration
├── error.rs              - Custom error types
├── models/               - Data models and result types
│   ├── mod.rs
│   └── ...
├── commands/             - Command handlers
│   ├── mod.rs
│   └── ...
├── services/             - Business logic
│   ├── mod.rs
│   └── ...
├── repositories/         - Data access
│   ├── mod.rs
│   └── ...
└── utils/                - Shared utilities
    ├── mod.rs
    └── ...
```

---

## 📝 Naming Conventions

### Module Names (snake_case)

**Rules**:
- Use full words, avoid abbreviations
- Use descriptive, precise names
- Group related modules in subdirectories

| Avoid | Use | Reason |
|--------|-----|--------|
| `cfr.rs` | `decompiler.rs` or `cfr_decompiler.rs` | More descriptive |
| `scan.rs` | `discovery.rs` or `jar_discovery.rs` | "scan" is too generic |
| `probe.rs` | `jar_inspector.rs` | More descriptive |
| `parse.rs` | `parser.rs` | Standard naming |

**Module Content Naming**:
```rust
// File: src/services/jar_discovery.rs
pub mod jar_discovery {
    // Module-level types
    pub struct JarDiscovery { ... }

    // Module-level functions - no namespace prefix
    pub fn discover_jars(path: &Path) -> Result<Vec<JarInfo>> { ... }
    pub fn infer_search_scope(cli: &Cli) -> Result<SearchScope> { ... }
}
```

### Struct Names (PascalCase)

**Rules**:
- Use descriptive, full names
- Avoid abbreviations unless widely known
- For specific implementations, include the type name

| Avoid | Use | Reason |
|--------|-----|--------|
| `struct Cfr` | `struct CfrDecompiler` | Clear what it is |
| `struct Db` | `struct Database` or `struct SQLiteDatabase` | More descriptive |
| `struct App` | `struct Application` | More explicit |

### Function Names (snake_case)

**Rules**:
- Use precise verbs
- Avoid redundant context from module name
- Use `get_`, `set_`, `is_`, `has_`, `find_`, `execute_` patterns

| Context | Avoid | Use | Reason |
|----------|--------|-----|--------|
| In `jar_discovery.rs` | `scan_jars` | `discover_jars` | More precise |
| In `jar_inspector.rs` | `jar_contains_class` | `jar_has_class` | Consistent verb |
| In `commands/find.rs` | `find_class` | `execute_find` | Command pattern |
| Path resolution | `class_finder_home` | `get_app_data_dir` | Descriptive |
| Path resolution | `resolve_m2_repo` | `resolve_maven_repository` | No abbreviation |

**Common Patterns**:
```rust
// Getter
pub fn get_config(&self) -> &Config { ... }

// Setter (prefer builder pattern over setters)
pub fn with_config(mut self, config: Config) -> Self { ... }

// Boolean check - use `is_` or `has_`
pub fn is_cataloged(&self, jar_key: &str) -> bool { ... }
pub fn has_class(&self, class_name: &str) -> bool { ... }

// Search/Find - returns Option/Result
pub fn find_by_id(&self, id: &str) -> Option<Item> { ... }
pub fn find_class(&self, name: &str) -> Result<ClassInfo> { ... }

// Execute/Run - for commands
pub fn execute_find(&self, args: FindArgs) -> Result<FindResult> { ... }
pub fn run(&self) -> Result<()> { ... }

// Create/Build - for constructors
pub fn create(args: CreateArgs) -> Result<Self> { ... }
pub fn new(config: Config) -> Result<Self> { ... }
```

### Variable Names (snake_case)

**Rules**:
- Use descriptive names, avoid single letters except in loops
- Avoid abbreviations unless widely known
- Be consistent with units when naming duration/size variables

| Avoid | Use | Reason |
|--------|-----|--------|
| `m2_repo` | `maven_repository` | No abbreviation |
| `db_path` | `database_path` | Consistency |
| `jar_key` | `jar_id` or `jar_path_key` | "key" is ambiguous |
| `batch_size` | `batch_capacity` | More precise |
| `flush_interval_ms` | `flush_duration_ms` | Clear units |

**Good Examples**:
```rust
// Descriptive variable names
let class_name = "com.example.MyClass";
let jar_path = PathBuf::from("/path/to/file.jar");
let database_path = PathBuf::from("~/.app/database.db");

// Capacity/count vs size
let batch_capacity = 100;
let item_count = items.len();

// Duration naming
let flush_duration = Duration::from_millis(100);
let timeout_seconds = 30;
```

### Constant Names (SCREAMING_SNAKE_CASE)

```rust
// Module-level constants
pub const DEFAULT_BATCH_CAPACITY: usize = 100;
pub const MAX_RETRY_ATTEMPTS: u32 = 3;
pub const DATABASE_VERSION: u32 = 1;
```

---

## 🏗️ Architecture Patterns

### Layered Architecture

```
┌─────────────────────────────────────┐
│  Commands/Entry Point Layer         │  - CLI commands, Tauri IPC
├─────────────────────────────────────┤
│  Application/Orchestration Layer    │  - app.rs, command coordination
├─────────────────────────────────────┤
│  Service Layer                     │  - Business logic, orchestration
├─────────────────────────────────────┤
│  Repository Layer                  │  - Data access, persistence
├─────────────────────────────────────┤
│  Database/Storage Layer           │  - Database, files, API calls
└─────────────────────────────────────┘
```

**Dependency Flow**: Commands → Services → Repositories → Database

### Repository Pattern

```rust
// src/repositories/class_repository.rs
use crate::db::connection::Database;
use crate::error::Result;

pub struct ClassRepository {
    database: Arc<Database>,
}

impl ClassRepository {
    pub fn new(database: Arc<Database>) -> Self {
        Self { database }
    }

    pub fn find_by_name(&self, class_name: &str) -> Result<Option<ClassInfo>> {
        let readable_txn = self.database.begin_read()?;
        let table = readable_txn.open_table(crate::db::schema::CLASSES_TABLE)?;
        Ok(table.get(&class_name)?.map(|value| ClassInfo::from_bytes(value)))
    }

    pub fn save(&self, class_info: &ClassInfo) -> Result<()> {
        let mut write_txn = self.database.begin_write()?;
        {
            let mut table = write_txn.open_table(crate::db::schema::CLASSES_TABLE)?;
            table.insert(&class_info.name, class_info.to_bytes())?;
        }
        write_txn.commit()?;
        Ok(())
    }
}
```

### Service Pattern

```rust
// src/services/class_finder_service.rs
use crate::repositories::{ClassRepository, JarRepository};
use crate::error::Result;

pub struct ClassFinderService {
    class_repository: Arc<ClassRepository>,
    jar_repository: Arc<JarRepository>,
}

impl ClassFinderService {
    pub fn new(
        class_repository: Arc<ClassRepository>,
        jar_repository: Arc<JarRepository>,
    ) -> Self {
        Self {
            class_repository,
            jar_repository,
        }
    }

    pub async fn find_class(&self, class_name: &str) -> Result<ClassLocation> {
        // Business logic here
        let class_info = self.class_repository
            .find_by_name(class_name)?
            .ok_or_else(|| anyhow!("Class not found: {}", class_name))?;

        let jar_info = self.jar_repository
            .find_by_id(&class_info.jar_id)?
            .ok_or_else(|| anyhow!("JAR not found: {}", class_info.jar_id))?;

        Ok(ClassLocation {
            class: class_info,
            jar: jar_info,
        })
    }
}
```

---

## 🛡️ Error Handling Standards

### Custom Error Types

```rust
// src/error.rs
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("Database error: {0}")]
    Database(#[from] redb::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Class not found: {0}")]
    ClassNotFound(String),

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("Decompilation failed: {0}")]
    DecompilationFailed(String),
}

pub type Result<T> = std::result::Result<T, AppError>;
```

### Error Context Usage

```rust
// Consistent error context
use anyhow::Context;

fn read_jar(path: &Path) -> Result<Vec<u8>> {
    std::fs::read(path)
        .with_context(|| format!("Failed to read JAR file: {}", path.display()))
}
```

**Rules**:
- Use `.context()` or `.with_context()` from anyhow
- Provide meaningful error messages in English
- Include relevant context (file paths, identifiers) in
- Don't use Chinese error messages in code

---

## 🔧 Configuration Standards

### Centralized Configuration

```rust
// src/config.rs
use std::path::PathBuf;

#[deriveConfig(Debug, Clone)]
pub struct AppConfig {
    pub maven_repository: PathBuf,
    pub database_path: PathBuf,
    pub decompiler: DecompilerConfig,
    pub performance: PerformanceConfig,
}

#[derive(Debug, Clone)]
pub struct DecompilerConfig {
    pub decompiler_jar_path: PathBuf,
    pub java_executable: PathBuf,
}

#[derive(Debug, Clone)]
pub struct PerformanceConfig {
    pub batch_capacity: usize,
    pub flush_duration: Duration,
    pub parallel_jobs: usize,
}

impl AppConfig {
    pub fn from_cli(cli: &Cli) -> Result<Self> {
        Ok(Self {
            maven_repository: cli.maven_repo.clone(),
            database_path: get_app_data_dir()?.join("database.db"),
            decompiler: DecompilerConfig {
                decompiler_jar_path: resolve_decompiler_jar()?,
                java_executable: resolve_java_executable()?,
            },
            performance: PerformanceConfig {
                batch_capacity: cli.batch_size.unwrap_or(100),
                flush_duration: Duration::from_millis(cli.flush_ms.unwrap_or(1000)),
                parallel_jobs: cli.jobs.unwrap_or(num_cpus::get()),
            },
        })
    }
}
```

---

## 🗄️ Database Standards

### Schema Definition

```rust
// src/db/schema.rs
use redb::TableDefinition;

pub mod schema {
    pub const CLASSES: TableDefinition<&str, &str> =
        TableDefinition::new("classes");
    pub const JARS: TableDefinition<&str, &str> =
        TableDefinition::new("jars");
    pub const ARTIFACTS: TableDefinition<&str, &str> =
        TableDefinition::new("artifacts");
}
```

### Database Connection

```rust
// src/db/connection.rs
use redb::Database;
use std::sync::Arc;

pub struct DatabaseConnection {
    database: Arc<Database>,
}

impl DatabaseConnection {
    pub fn open(path: &Path) -> Result<Self> {
        let database = Database::create(path)
            .context("Failed to open database")?;
        Ok(Self {
            database: Arc::new(database),
        })
    }

    pub fn database(&self) -> Arc<Database> {
        Arc::clone(&self.database)
    }
}
```

---

## ✅ Testing Standards

### Test File Organization

```
src/
├── services/
│   └── class_finder.rs
└── services/
    └── class_finder_tests.rs   # Unit tests in same module
tests/
├── integration/
│   └── find_command_test.rs   # Integration tests
└── common/
    └── test_utils.rs           # Shared test helpers
```

### Test Naming

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_class_returns_result_when_class_exists() {
        // Arrange
        let class_name = "com.example.TestClass";
        // ...

        // Act
        let result = find_class(class_name);

        // Assert
        assert!(result.is_ok());
    }

    #[test]
    fn test_find_class_returns_error_when_class_not_found() {
        // ...
    }
}
```

---

## 📏 Code Quality Checklist

Before marking code as complete:

- [ ] File names follow `snake_case` convention
- [ ] Struct names follow `PascalCase` convention
- [ ] Function names are descriptive and precise
- [ ] No abbreviations except widely known ones
- [ ] Error messages are in English
- [ ] Consistent error handling with `anyhow::Context`
- [ ] Configuration is centralized in `config.rs`
- [ ] Database operations use repository pattern
- [ ] main.rs is under 150 lines
- [ ] Each module has clear single responsibility
- [ ] No duplicate functionality across modules
- [ ] Tests follow naming conventions
