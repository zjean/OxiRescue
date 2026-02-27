use clap::Parser;

mod cli;

fn main() -> anyhow::Result<()> {
    let args = cli::Cli::parse();
    match args.command {
        cli::Command::Dump {
            blobs,
            output,
            classify,
            copy,
            verify,
            dry_run,
            min_size,
        } => {
            let min_bytes = min_size.map(|s| parse_size(&s)).transpose()?;
            let stats = oxirescue::dump::recover::dump_blobs(
                &blobs, &output, classify, copy, verify, dry_run, min_bytes,
            )?;
            println!(
                "Recovered: {} blobs ({} bytes)",
                stats.total_blobs, stats.total_bytes
            );
            if stats.skipped > 0 {
                println!("Skipped: {}", stats.skipped);
            }
            if stats.corrupted > 0 {
                println!("Corrupted: {}", stats.corrupted);
            }
            for (cat, (count, bytes)) in &stats.by_category {
                println!("  {cat}: {count} files ({bytes} bytes)");
            }
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

fn parse_size(s: &str) -> anyhow::Result<u64> {
    let s = s.trim().to_uppercase();
    if let Some(num) = s.strip_suffix("GB") {
        Ok(num.trim().parse::<u64>()? * 1024 * 1024 * 1024)
    } else if let Some(num) = s.strip_suffix("MB") {
        Ok(num.trim().parse::<u64>()? * 1024 * 1024)
    } else if let Some(num) = s.strip_suffix("KB") {
        Ok(num.trim().parse::<u64>()? * 1024)
    } else {
        Ok(s.parse::<u64>()?)
    }
}
