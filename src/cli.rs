use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "oxirescue", about = "Disaster recovery tool for OxiCloud")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Extract all blobs to a directory (works without database)
    Dump {
        /// Path to the .blobs directory
        #[arg(long)]
        blobs: PathBuf,
        /// Output directory for recovered files
        #[arg(long)]
        output: PathBuf,
        /// Group files by MIME type
        #[arg(long, default_value_t = false)]
        classify: bool,
        /// Force copy instead of hard-link
        #[arg(long, default_value_t = false)]
        copy: bool,
        /// Re-hash every blob and report corrupted files
        #[arg(long, default_value_t = false)]
        verify: bool,
        /// Show what would be extracted without doing it
        #[arg(long, default_value_t = false)]
        dry_run: bool,
        /// Skip blobs smaller than this (e.g. "1KB", "1MB")
        #[arg(long)]
        min_size: Option<String>,
    },
    /// Export PostgreSQL metadata to a portable SQLite file
    ExportMetadata {
        /// PostgreSQL connection string
        #[arg(long)]
        db: String,
        /// Output SQLite file path
        #[arg(long)]
        output: PathBuf,
    },
    /// Mount the OxiCloud filesystem as read-only FUSE
    Mount {
        /// PostgreSQL connection string (live mode)
        #[arg(long)]
        db: Option<String>,
        /// Path to exported SQLite metadata (offline mode)
        #[arg(long)]
        meta: Option<PathBuf>,
        /// Path to the .blobs directory
        #[arg(long)]
        blobs: PathBuf,
        /// Directory to mount on
        mountpoint: PathBuf,
    },
    /// Launch interactive TUI
    Tui {
        /// PostgreSQL connection string (live mode)
        #[arg(long)]
        db: Option<String>,
        /// Path to exported SQLite metadata (offline mode)
        #[arg(long)]
        meta: Option<PathBuf>,
        /// Path to the .blobs directory
        #[arg(long)]
        blobs: PathBuf,
    },
}
