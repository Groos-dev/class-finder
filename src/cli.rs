use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Debug, Clone, Parser)]
#[command(name = "class-finder")]
#[command(about = "在本地 Maven 仓库中查找 Java 类并返回反编译源码")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    #[arg(long, value_name = "PATH")]
    pub m2: Option<PathBuf>,

    #[arg(long, value_name = "FILE")]
    pub cfr: Option<PathBuf>,

    #[arg(long, value_name = "FILE")]
    pub db: Option<PathBuf>,
}

#[derive(Debug, Clone, Subcommand)]
pub enum Commands {
    Find {
        class_name: String,

        #[arg(short = 'f', long, value_enum, default_value_t = OutputFormat::Json)]
        format: OutputFormat,

        #[arg(long)]
        code_only: bool,

        #[arg(short = 'v', long, value_name = "VER")]
        version: Option<String>,

        #[arg(short = 'o', long, value_name = "FILE")]
        output: Option<PathBuf>,
    },
    Load {
        jar_path: PathBuf,
    },
    Stats,
    Clear,
}

#[derive(Debug, Copy, Clone, ValueEnum)]
pub enum OutputFormat {
    Json,
    Text,
    Code,
}
