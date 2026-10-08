use std::collections::HashSet;
use std::fs;
use std::path::Path;

use crate::cli::FilterArgs;
use crate::fail;
use crate::model::TimetableEvent;

/// Order events by weekday and start time.
pub fn sort_events(events: &mut [TimetableEvent]) {
    events.sort_by(|a, b| {
        a.date
            .cmp(&b.date)
            .then_with(|| a.start_time.cmp(&b.start_time))
    });
}

/// Keep only events whose building and room identifiers contain the given needles.
pub fn filter_events_by_location(
    events: Vec<TimetableEvent>,
    buildings: &[String],
    rooms: &[String],
) -> Vec<TimetableEvent> {
    events
        .into_iter()
        .filter(|event| {
            let building_matches = buildings.is_empty()
                || buildings.iter().any(|needle| {
                    event
                        .building_url
                        .to_lowercase()
                        .contains(&needle.to_lowercase())
                });
            let room_matches = rooms.is_empty()
                || rooms.iter().any(|needle| {
                    event
                        .room_url
                        .to_lowercase()
                        .contains(&needle.to_lowercase())
                });
            building_matches && room_matches
        })
        .collect()
}

/// Keep only events allowed by the weekday, whitelist, and blacklist settings.
pub fn apply_filters(events: Vec<TimetableEvent>, args: &FilterArgs) -> Vec<TimetableEvent> {
    let blacklist = args
        .blacklist
        .as_deref()
        .map(load_course_codes)
        .unwrap_or_default();
    let whitelist = args.whitelist.as_deref().map(load_course_codes);

    events
        .into_iter()
        .filter(|event| {
            if !args.weekday.is_empty() && !args.weekday.contains(&event.day) {
                return false;
            }
            if let Some(whitelist) = &whitelist
                && !event
                    .courses
                    .iter()
                    .any(|course| whitelist.contains(&course.code))
            {
                return false;
            }
            if !blacklist.is_empty()
                && event
                    .courses
                    .iter()
                    .any(|course| blacklist.contains(&course.code))
            {
                return false;
            }
            true
        })
        .collect()
}

/// Read course codes from a file, ignoring the optional "  ---  Title" suffix.
fn load_course_codes(path: &Path) -> HashSet<String> {
    let content = fs::read_to_string(path)
        .unwrap_or_else(|err| fail(format!("Failed to read {}: {err}", path.display())));
    content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            line.split_once("  ---  ")
                .map(|(code, _)| code)
                .unwrap_or(line)
                .trim()
                .to_string()
        })
        .collect()
}
