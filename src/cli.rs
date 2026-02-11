use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Debug, Clone, Parser)]
#[command(name = "class-finder")]
#[command(about = "Find Java classes in local Maven repository and return decompiled source")]
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
    Warmup {
        #[arg(value_name = "JAR")]
        jar_path: Option<PathBuf>,

        #[arg(long)]
        hot: bool,

        #[arg(long, value_name = "GROUP")]
        group: Option<String>,

        #[arg(long, value_name = "N", default_value_t = 20)]
        top: usize,

        #[arg(long, value_name = "N")]
        limit: Option<usize>,
    },
    Index {
        #[arg(long, value_name = "DIR")]
        path: Option<PathBuf>,
    },
    Stats,
    Clear,
}

#[derive(Debug, Copy, Clone, ValueEnum)]
pub enum OutputFormat {
    Json,
    Text,
    Code,
    Structure,
}
