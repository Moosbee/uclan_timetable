use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;
use std::str::FromStr;

use crate::model::Weekday;

#[derive(Parser)]
#[command(
    name = "uclan_timetable",
    version,
    about = "Scrape or process the UCLan room timetable using the UCLan timetable website https://apps.uclan.ac.uk/MvcRoomTimetable/. Scraping fetches the next five days including today."
)]
pub struct Cli {
    /// Path to the buildings and rooms cache file (used when scraping).
    #[arg(long, global = true, default_value = "cache.json")]
    pub cache: PathBuf,

    /// Re-scrape buildings and rooms even if the cache exists.
    #[arg(long, global = true)]
    pub refresh: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Generate a list of all timetable events, optionally restricted to certain buildings or rooms.
    ///
    /// Scraping always fetches the next five days including today. For example,
    /// if today is Thursday, the scraper fetches Thursday (today), Friday
    /// (tomorrow), Saturday, Sunday, and Monday (next week).
    Events(EventsArgs),

    /// Generate a list of all courses and their titles (one course may have several titles).
    Courses(CoursesArgs),

    /// Filter the timetable by a black/whitelist and/or by weekday.
    Filter(FilterArgs),
}

/// Where timetable data should come from.
#[derive(Debug, Clone)]
pub enum DataSource {
    Scrape,
    File(PathBuf),
}

impl FromStr for DataSource {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value == "scrape" {
            Ok(Self::Scrape)
        } else {
            Ok(Self::File(PathBuf::from(value)))
        }
    }
}

#[derive(Args, Clone)]
pub struct SourceArgs {
    /// Where to read timetable data from: "scrape" or a path to a timetable JSON file.
    #[arg(long, default_value = "scrape", value_parser = clap::value_parser!(DataSource))]
    pub source: DataSource,
}

#[derive(Args, Clone)]
pub struct OutputArgs {
    /// Output format.
    #[arg(short, long, value_enum, default_value = "json")]
    pub format: OutputFormat,

    /// Write output to this file instead of stdout. Existing course files are updated.
    #[arg(short, long)]
    pub output: Option<PathBuf>,
}

#[derive(Args, Clone)]
pub struct EventsArgs {
    #[command(flatten)]
    pub source: SourceArgs,

    #[command(flatten)]
    pub output: OutputArgs,

    /// Restrict to buildings whose identifier (e.g. EIC) matches this text. Repeatable.
    #[arg(short, long)]
    pub building: Vec<String>,

    /// Restrict to rooms whose identifier (e.g. EIC317) matches this text. Repeatable.
    #[arg(short, long)]
    pub room: Vec<String>,
}

#[derive(Args, Clone)]
pub struct CoursesArgs {
    #[command(flatten)]
    pub source: SourceArgs,

    #[command(flatten)]
    pub output: OutputArgs,
}

#[derive(Args, Clone)]
pub struct FilterArgs {
    #[command(flatten)]
    pub source: SourceArgs,

    #[command(flatten)]
    pub output: OutputArgs,

    /// File of course codes (with optional "  ---  Title" suffix) to exclude.
    #[arg(long)]
    pub blacklist: Option<PathBuf>,

    /// File of course codes (with optional "  ---  Title" suffix) to include.
    #[arg(long)]
    pub whitelist: Option<PathBuf>,

    /// Only include events on these weekdays. Repeatable.
    #[arg(short, long, value_parser = clap::value_parser!(Weekday))]
    pub weekday: Vec<Weekday>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum OutputFormat {
    Json,
    Csv,
    Text,
}
