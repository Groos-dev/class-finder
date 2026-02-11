//! # class-finder
//!
//! A high-performance Java class finder and decompiler for Maven repositories.
//!
//! ## Architecture
//!
//! - **cache**: Persistent storage using redb for decompiled sources and metadata
//! - **registry**: Class-to-JAR mapping index for fast lookups
//! - **scan**: JAR file discovery in Maven repository structure
//! - **probe**: JAR inspection utilities for class existence checks
//! - **catalog**: JAR indexing to extract class lists
//! - **cfr**: CFR decompiler integration
//! - **parse**: Decompiled output parsing and class extraction
//! - **buffer**: Write buffering for batch database operations
//! - **warmup**: Background preloading of frequently accessed JARs
//! - **hotspot**: Access tracking and warmup prioritization
//! - **incremental**: Incremental indexing based on file modification times
//! - **structure**: Java class structure extraction using tree-sitter AST parsing

pub mod buffer;
pub mod cache;
pub mod catalog;
pub mod cfr;
pub mod cli;
pub mod config;
pub mod hotspot;
pub mod incremental;
pub mod parse;
pub mod probe;
pub mod registry;
pub mod scan;
pub mod structure;
pub mod warmup;
