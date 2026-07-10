//! Command-line frontend for `shrinkpub-core`: shrink EPUBs from the
//! terminal or from scripts, no GUI required.
//!
//! Exit codes: 0 = every file shrunk, 1 = at least one file failed,
//! 2 = bad arguments (clap's default).

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use shrinkpub_core::{shrink_epub, Quality, ShrinkReport};

#[derive(Parser)]
#[command(
    name = "shrinkpub",
    version,
    about = "Shrink EPUB files by recompressing the images inside them",
    after_help = "Each input gets a \"<name> (shrunk).epub\" sibling; originals are never touched."
)]
struct Args {
    /// EPUB files to shrink.
    #[arg(required = true, value_name = "EPUB")]
    files: Vec<PathBuf>,

    /// Quality tier: veryhigh, high, medium, low, verylow, terrible or atrocious.
    #[arg(short, long, default_value = "medium", value_parser = parse_quality)]
    quality: Quality,

    /// Print per-entry progress while shrinking.
    #[arg(short, long)]
    verbose: bool,
}

fn parse_quality(raw: &str) -> Result<Quality, String> {
    Quality::from_id(&raw.to_lowercase()).ok_or_else(|| {
        let tiers: Vec<&str> = Quality::ALL.iter().map(|q| q.id()).collect();
        format!(
            "unknown quality tier (expected one of: {})",
            tiers.join(", ")
        )
    })
}

fn main() -> ExitCode {
    let args = Args::parse();

    let mut failures = 0;
    for file in &args.files {
        let result = shrink_epub(file, args.quality, |progress| {
            if args.verbose {
                eprintln!(
                    "  [{}/{}] {}",
                    progress.index + 1,
                    progress.total,
                    progress.entry_name
                );
            }
        });
        match result {
            Ok(report) => println!("{}", summarize(file, &report)),
            Err(error) => {
                eprintln!("error: {error}");
                failures += 1;
            }
        }
    }

    if failures > 0 {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn summarize(input: &std::path::Path, report: &ShrinkReport) -> String {
    format!(
        "{} -> {} ({} -> {}, saved {}, {} images recompressed, {} kept as-is)",
        input.display(),
        report.output_path.display(),
        format_bytes(report.input_bytes),
        format_bytes(report.output_bytes),
        format_saving(report.input_bytes, report.output_bytes),
        report.images_recompressed,
        report.images_kept,
    )
}

/// Human byte size, binary units, one decimal ("4.2 MiB").
fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// Saved percentage, or a note when nothing was gained.
fn format_saving(input: u64, output: u64) -> String {
    if output >= input || input == 0 {
        return "nothing".to_string();
    }
    let percent = (input - output) as f64 / input as f64 * 100.0;
    format!("{percent:.0}%")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_all_tiers_case_insensitively() {
        assert_eq!(parse_quality("medium"), Ok(Quality::Medium));
        assert_eq!(parse_quality("VeryHigh"), Ok(Quality::VeryHigh));
        assert_eq!(parse_quality("ATROCIOUS"), Ok(Quality::Atrocious));
        assert!(parse_quality("ultra").is_err());
    }

    #[test]
    fn formats_bytes_humanely() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(999), "999 B");
        assert_eq!(format_bytes(4 * 1024 * 1024 + 200 * 1024), "4.2 MiB");
    }

    #[test]
    fn formats_savings() {
        assert_eq!(format_saving(100, 40), "60%");
        assert_eq!(format_saving(100, 100), "nothing");
        assert_eq!(format_saving(100, 120), "nothing");
        assert_eq!(format_saving(0, 0), "nothing");
    }

    #[test]
    fn cli_definition_is_valid() {
        use clap::CommandFactory;
        Args::command().debug_assert();
    }
}
