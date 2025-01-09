use clap::Parser;
use std::path::PathBuf;
use tracing::Level;

#[derive(Debug, Parser)]
pub struct Commands {
    #[arg(long, short)]
    pub input: PathBuf,
    #[arg(long, short)]
    pub output: PathBuf,
    #[arg(long, short)]
    pub shaders: Option<bool>,
    #[arg(long, short)]
    pub threads: Option<u8>,
    #[arg(long, short)]
    pub filter: Option<String>,
    #[arg(long, short, default_value_t = Level::INFO)]
    pub debug: Level,
}
