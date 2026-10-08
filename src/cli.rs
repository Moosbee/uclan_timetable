use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;
use std::str::FromStr;

use crate::model::Weekday;

#[derive(Parser)]
#[command(
    name = "uclan_timetable",
    version,
    about = "Scrape or process the UCLan room timetable.",
    long_about = "Scrape or process the UCLan room timetable.\n\
        \n\
        Timetable data is scraped from the UCLan timetable website\n\
        <https://apps.uclan.ac.uk/MvcRoomTimetable/>. Scraping always fetches\n\
        the next five days including today: if today is Thursday, the scraper\n\
        fetches Thursday, Friday, Saturday, Sunday, and Monday.\n\
        \n\
        Each subcommand reads events from a --source (the website by default,\n\
        or a timetable JSON file written by a previous `events` run) and\n\
        writes the result to stdout or a file in JSON, CSV, or plain text.",
    after_help = "Examples:\n  \
        uclan_timetable events\n  \
        uclan_timetable events --building EIC --room EIC317 --format csv\n  \
        uclan_timetable courses --output courses.json\n  \
        uclan_timetable filter --whitelist mine.txt --weekday Mon --weekday Tue"
)]
pub struct Cli {
    /// Path to the buildings and rooms cache file (created and read when scraping).
    #[arg(long, global = true, default_value = "cache.json")]
    pub cache: PathBuf,

    /// Re-scrape buildings and rooms even if the cache file already exists.
    #[arg(long, global = true)]
    pub refresh: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// List every timetable event for the next five days.
    ///
    /// Events can be restricted to particular buildings and/or rooms.
    ///
    /// Scraping always fetches the next five days including today. For example,
    /// if today is Thursday, the scraper fetches Thursday (today), Friday
    /// (tomorrow), Saturday, Sunday, and Monday (next week).
    #[command(
        after_help = "Output schema:\n\
            \n\
            JSON: an array of event objects. Each object has the string fields\n\
            room_url (e.g. EIC317), building_url (e.g. EIC), day (Mon..Sun),\n\
            date (YYYY-MM-DD), start_time (HH:MM), and end_time (HH:MM); the\n\
            integer field duration_slots (event length in 15-minute slots); an\n\
            event_type that is either the string \"non-teaching\" or an object\n\
            { type, category, delivery }; a courses array of { code, title }\n\
            objects; and a speakers array of strings.\n\
            \n\
            CSV: one row per event with the columns room_url, building_url,\n\
            day, date, start_time, end_time, duration_slots, event_type,\n\
            courses (each course as \"code - title\", multiple joined with\n\
            \"; \"), and speakers (joined with \"; \").\n\
            \n\
            Text: one line per event with the fields Room, Building, Day, Date,\n\
            Start, End, Took (slots), Type, Courses (\"code -> title\"), and\n\
            Speaker."
    )]
    Events(EventsArgs),

    /// List every course code and its title(s).
    ///
    /// One course code may have several titles, so each distinct title seen is
    /// listed.
    #[command(
        after_help = "Output schema:\n\
            \n\
            JSON: an object mapping each course code to an array of its titles.\n\
            \n\
            CSV: one row per (code, title) pair with the columns code and title.\n\
            \n\
            Text: one line per (code, title) pair in the form\n\
            \"CODE  ---  TITLE\"."
    )]
    Courses(CoursesArgs),

    /// Filter the timetable by a blacklist/whitelist and/or by weekday.
    ///
    /// Filtering keeps only events that pass every supplied filter: events on
    /// one of the requested weekdays (if any), with at least one whitelisted
    /// course (if a whitelist is given), and with no blacklisted courses (if a
    /// blacklist is given).
    #[command(
        after_help = "Output schema: identical to the `events` command (see\n\
            `uclan_timetable events --help`)."
    )]
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
    ///
    /// "scrape" fetches from the UCLan timetable website; any other value is
    /// treated as the path to a timetable JSON file produced by a previous
    /// `events` run.
    #[arg(long, value_name = "SOURCE", default_value = "scrape", value_parser = clap::value_parser!(DataSource))]
    pub source: DataSource,
}

#[derive(Args, Clone)]
pub struct OutputArgs {
    /// Output format: json, csv, or text.
    #[arg(short, long, value_enum, default_value = "json")]
    pub format: OutputFormat,

    /// Write output to this file instead of stdout.
    ///
    /// By default, an existing file is merged with the newly collected data.
    /// Use `--write-mode overwrite` to replace the file instead.
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Whether to merge with an existing output file or overwrite it.
    ///
    /// `merge` keeps the entries already in the file and adds the new ones;
    /// `overwrite` replaces the file's contents. Applies to `events`,
    /// `courses`, and `filter`.
    #[arg(long, value_enum, default_value = "merge")]
    pub write_mode: WriteMode,
}

#[derive(Args, Clone)]
pub struct EventsArgs {
    #[command(flatten)]
    pub source: SourceArgs,

    #[command(flatten)]
    pub output: OutputArgs,

    /// Restrict to buildings whose identifier (e.g. EIC) contains this text.
    ///
    /// Matching is case-insensitive and may be repeated; a building is kept if
    /// it matches any of the given values.
    #[arg(short, long)]
    pub building: Vec<String>,

    /// Restrict to rooms whose identifier (e.g. EIC317) contains this text.
    ///
    /// Matching is case-insensitive and may be repeated; a room is kept if it
    /// matches any of the given values.
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

    /// File of course codes to exclude.
    ///
    /// One code per line; an optional "  ---  Title" suffix on a line is
    /// ignored. Any event mentioning a blacklisted course is removed.
    #[arg(long)]
    pub blacklist: Option<PathBuf>,

    /// File of course codes to include.
    ///
    /// One code per line; an optional "  ---  Title" suffix on a line is
    /// ignored. When given, only events mentioning at least one whitelisted
    /// course are kept.
    #[arg(long)]
    pub whitelist: Option<PathBuf>,

    /// Only include events on this weekday (Mon..Sun). Repeatable.
    #[arg(short, long, value_name = "WEEKDAY", value_parser = clap::value_parser!(Weekday))]
    pub weekday: Vec<Weekday>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum OutputFormat {
    Json,
    Csv,
    Text,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum WriteMode {
    /// Merge the new data with the entries already in the output file.
    Merge,
    /// Replace the output file's contents with the new data.
    Overwrite,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_source_treats_scrape_keyword_as_scrape() {
        assert!(matches!(
            "scrape".parse::<DataSource>().unwrap(),
            DataSource::Scrape
        ));
    }

    #[test]
    fn data_source_treats_any_other_value_as_a_file() {
        let DataSource::File(path) = "timetable.json".parse::<DataSource>().unwrap() else {
            panic!("expected a file data source");
        };
        assert_eq!(path, PathBuf::from("timetable.json"));
    }
}
