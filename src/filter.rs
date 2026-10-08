use std::collections::HashSet;
use std::fs;
use std::path::Path;

use crate::cli::FilterArgs;
use crate::fail;
use crate::model::TimetableEvent;

/// Order events by date and start time.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::{DataSource, OutputArgs, OutputFormat, SourceArgs, WriteMode};
    use crate::model::{Course, EventType, Weekday};
    use std::path::PathBuf;

    fn event(
        date: &str,
        start_time: &str,
        day: Weekday,
        courses: &[(&str, &str)],
    ) -> TimetableEvent {
        TimetableEvent {
            room_url: "EIC317".to_string(),
            building_url: "EIC".to_string(),
            day,
            date: date.to_string(),
            start_time: start_time.to_string(),
            end_time: "10:00".to_string(),
            duration_slots: 2,
            event_type: EventType::NonTeaching,
            courses: courses
                .iter()
                .map(|(code, title)| Course {
                    code: code.to_string(),
                    title: title.to_string(),
                })
                .collect(),
            speakers: vec![],
        }
    }

    fn filter_args() -> FilterArgs {
        FilterArgs {
            source: SourceArgs {
                source: DataSource::Scrape,
            },
            output: OutputArgs {
                format: OutputFormat::Json,
                output: None,
                write_mode: WriteMode::Overwrite,
            },
            blacklist: None,
            whitelist: None,
            weekday: vec![],
        }
    }

    fn temp_path(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join("uclan_timetable_filter_tests");
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(name)
    }

    #[test]
    fn sort_events_orders_by_date_then_start_time() {
        // sort_events orders chronologically by date first, then start time.
        // These two events are deliberately ordered so date order contradicts
        // weekday order: Sun sorts after Mon by weekday, but its date is
        // earlier here.
        let mut events = vec![
            event("2026-10-12", "09:00", Weekday::Mon, &[]),
            event("2026-10-11", "10:00", Weekday::Sun, &[]),
        ];
        sort_events(&mut events);
        assert_eq!(
            events.iter().map(|e| e.day).collect::<Vec<_>>(),
            vec![Weekday::Sun, Weekday::Mon]
        );
    }

    #[test]
    fn sort_events_uses_start_time_when_dates_match() {
        let mut events = vec![
            event("2026-10-12", "11:00", Weekday::Mon, &[]),
            event("2026-10-12", "09:00", Weekday::Mon, &[]),
        ];
        sort_events(&mut events);
        assert_eq!(
            events
                .iter()
                .map(|e| e.start_time.as_str())
                .collect::<Vec<_>>(),
            vec!["09:00", "11:00"]
        );
    }

    #[test]
    fn filter_by_location_keeps_everything_when_no_filters_given() {
        let events = vec![event("2026-10-12", "09:00", Weekday::Mon, &[])];
        let kept = filter_events_by_location(events, &[], &[]);
        assert_eq!(kept.len(), 1);
    }

    #[test]
    fn filter_by_location_matches_building_and_room_case_insensitively() {
        let eic = event("2026-10-12", "09:00", Weekday::Mon, &[]);
        let other = TimetableEvent {
            building_url: "HB".to_string(),
            ..eic.clone()
        };
        let kept = filter_events_by_location(
            vec![eic, other],
            &["eic".to_string()],
            &["eic317".to_string()],
        );
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].building_url, "EIC");
    }

    #[test]
    fn filter_by_location_requires_building_and_room_to_both_match() {
        // Matches the building needle but not the room needle.
        let matches_building_only = TimetableEvent {
            room_url: "XYZ123".to_string(),
            building_url: "EIC".to_string(),
            ..event("2026-10-12", "09:00", Weekday::Mon, &[])
        };
        // Matches the room needle but not the building needle.
        let matches_room_only = TimetableEvent {
            room_url: "EIC317".to_string(),
            building_url: "HB".to_string(),
            ..event("2026-10-12", "09:00", Weekday::Mon, &[])
        };
        let kept = filter_events_by_location(
            vec![matches_building_only, matches_room_only],
            &["EIC".to_string()],
            &["EIC317".to_string()],
        );
        assert!(kept.is_empty());
    }

    #[test]
    fn apply_filters_keeps_only_requested_weekdays() {
        let mut args = filter_args();
        args.weekday = vec![Weekday::Mon];
        let events = vec![
            event("2026-10-12", "09:00", Weekday::Mon, &[]),
            event("2026-10-13", "09:00", Weekday::Tue, &[]),
        ];
        let kept = apply_filters(events, &args);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].day, Weekday::Mon);
    }

    #[test]
    fn apply_filters_keeps_only_events_with_a_whitelisted_course() {
        let path = temp_path("whitelist-only.txt");
        std::fs::write(&path, "CSC100\n").unwrap();

        let mut args = filter_args();
        args.whitelist = Some(path.clone());

        let events = vec![
            event(
                "2026-10-12",
                "09:00",
                Weekday::Mon,
                &[("CSC100", "Computing")],
            ),
            event("2026-10-12", "10:00", Weekday::Mon, &[("BB200", "Other")]),
        ];
        let kept = apply_filters(events, &args);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].courses[0].code, "CSC100");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn apply_filters_drops_events_with_a_blacklisted_course() {
        let path = temp_path("blacklist-only.txt");
        std::fs::write(&path, "CSC100\n").unwrap();

        let mut args = filter_args();
        args.blacklist = Some(path.clone());

        let events = vec![
            event(
                "2026-10-12",
                "09:00",
                Weekday::Mon,
                &[("CSC100", "Computing")],
            ),
            event("2026-10-12", "10:00", Weekday::Mon, &[("BB200", "Other")]),
        ];
        let kept = apply_filters(events, &args);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].courses[0].code, "BB200");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn apply_filters_combines_weekday_whitelist_and_blacklist() {
        let whitelist = temp_path("combines-whitelist.txt");
        std::fs::write(&whitelist, "CSC100\n").unwrap();
        let blacklist = temp_path("combines-blacklist.txt");
        std::fs::write(&blacklist, "BB200\n").unwrap();

        let mut args = filter_args();
        args.weekday = vec![Weekday::Mon];
        args.whitelist = Some(whitelist.clone());
        args.blacklist = Some(blacklist.clone());

        let events = vec![
            // Whitelisted course, right day -> kept.
            event(
                "2026-10-12",
                "09:00",
                Weekday::Mon,
                &[("CSC100", "Computing")],
            ),
            // Wrong day -> dropped.
            event(
                "2026-10-13",
                "09:00",
                Weekday::Tue,
                &[("CSC100", "Computing")],
            ),
            // Blacklisted course -> dropped.
            event("2026-10-12", "10:00", Weekday::Mon, &[("BB200", "Other")]),
        ];
        let kept = apply_filters(events, &args);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].courses[0].code, "CSC100");
        let _ = std::fs::remove_file(&whitelist);
        let _ = std::fs::remove_file(&blacklist);
    }

    #[test]
    fn load_course_codes_reads_codes_and_ignores_title_suffix() {
        let path = temp_path("codes.txt");
        std::fs::write(&path, "CSC100\nCSC101  ---  Computing\n\n   CSC102  \n").unwrap();
        let codes = load_course_codes(&path);
        assert!(codes.contains("CSC100"));
        assert!(codes.contains("CSC101"));
        assert!(codes.contains("CSC102"));
        assert_eq!(codes.len(), 3);
        let _ = std::fs::remove_file(&path);
    }
}
