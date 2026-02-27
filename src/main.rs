use clap::Parser;

mod cli;

fn main() -> anyhow::Result<()> {
    let args = cli::Cli::parse();
    match args.command {
        cli::Command::Dump { blobs, output, classify, .. } => {
            println!("dump: blobs={blobs:?} output={output:?} classify={classify}");
        }
        cli::Command::ExportMetadata { db, output } => {
            println!("export-metadata: db={db} output={output:?}");
        }
        cli::Command::Mount { mountpoint, .. } => {
            println!("mount: mountpoint={mountpoint:?}");
        }
        cli::Command::Tui { blobs, .. } => {
            println!("tui: blobs={blobs:?}");
        }
    }
    Ok(())
}
