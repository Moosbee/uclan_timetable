mod cli;
mod courses;
mod filter;
mod model;
mod output;
mod scrape;

use clap::Parser;
use std::fs;

use crate::cli::{Cli, Command, DataSource};
use crate::model::TimetableEvent;

fn main() {
    let cli = Cli::parse();

    match &cli.command {
        Command::Events(args) => {
            let events = load_events(&args.source.source, &cli);
            let events = filter::filter_events_by_location(events, &args.building, &args.room);
            output::write_events_output(&args.output.format, args.output.output.as_deref(), &events);
        }
        Command::Courses(args) => {
            let events = load_events(&args.source.source, &cli);
            let courses = courses::collect_courses(&events);
            output::write_courses_output(
                &args.output.format,
                args.output.output.as_deref(),
                &courses,
            );
        }
        Command::Filter(args) => {
            let events = load_events(&args.source.source, &cli);
            let events = filter::apply_filters(events, args);
            output::write_events_output(&args.output.format, args.output.output.as_deref(), &events);
        }
    }
}

/// Print an error message and exit with a non-zero status.
pub(crate) fn fail(message: String) -> ! {
    eprintln!("{message}");
    std::process::exit(1);
}

/// Read the timetable from a file or by scraping, sorted by weekday and start time.
fn load_events(source: &DataSource, cli: &Cli) -> Vec<TimetableEvent> {
    let mut events = match source {
        DataSource::Scrape => scrape::scrape_all_events(cli),
        DataSource::File(path) => {
            let content = fs::read_to_string(path)
                .unwrap_or_else(|err| fail(format!("Failed to read {}: {err}", path.display())));
            serde_json::from_str(&content).unwrap_or_else(|err| {
                fail(format!(
                    "Failed to parse {} as a timetable JSON file: {err}",
                    path.display()
                ))
            })
        }
    };
    filter::sort_events(&mut events);
    events
}
