use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use crate::cli::OutputFormat;
use crate::model::TimetableEvent;

/// Course code mapped to every title seen for that code.
pub type CourseMap = BTreeMap<String, BTreeSet<String>>;

/// Collect every course code and title mentioned by the given events.
pub fn collect_courses(events: &[TimetableEvent]) -> CourseMap {
    let mut courses: CourseMap = BTreeMap::new();
    for event in events {
        for course in &event.courses {
            courses
                .entry(course.code.clone())
                .or_default()
                .insert(course.title.clone());
        }
    }
    courses
}

/// Merge the courses already stored in `path` into `courses`, if that file can be read.
pub fn merge_courses_file(
    path: &Path,
    format: &OutputFormat,
    courses: &CourseMap,
) -> CourseMap {
    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(err) => {
            eprintln!("Failed to read {} ({err}), starting fresh", path.display());
            return courses.clone();
        }
    };

    match format {
        OutputFormat::Json => {
            let mut merged = serde_json::from_str::<CourseMap>(&content).unwrap_or_else(|err| {
                eprintln!("Failed to parse {} ({err}), starting fresh", path.display());
                BTreeMap::new()
            });
            merge_courses(&mut merged, courses);
            merged
        }
        OutputFormat::Csv => {
            let existing = read_courses_csv(&content).unwrap_or_else(|| {
                eprintln!("Failed to parse {} as CSV, starting fresh", path.display());
                BTreeMap::new()
            });
            let mut merged = courses.clone();
            merge_courses(&mut merged, &existing);
            merged
        }
        OutputFormat::Text => {
            let mut merged = courses.clone();
            for line in content.lines() {
                if let Some((code, title)) = line.split_once("  ---  ") {
                    merged
                        .entry(code.trim().to_string())
                        .or_default()
                        .insert(title.trim().to_string());
                }
            }
            merged
        }
    }
}

/// Add every course of `source` to `target`, keeping all known titles.
fn merge_courses(target: &mut CourseMap, source: &CourseMap) {
    for (code, titles) in source {
        target.entry(code.clone()).or_default().extend(titles.iter().cloned());
    }
}

/// Parse a CSV of `code,title` rows. Returns `None` when the CSV is malformed.
fn read_courses_csv(content: &str) -> Option<CourseMap> {
    let mut reader = csv::Reader::from_reader(content.as_bytes());
    let mut courses = BTreeMap::new();
    for record in reader.records() {
        let record = record.ok()?;
        let code = record.get(0)?.to_string();
        let title = record.get(1)?.to_string();
        courses
            .entry(code)
            .or_insert_with(BTreeSet::new)
            .insert(title);
    }
    Some(courses)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Course;
    use std::path::PathBuf;

    fn event(code: &str, title: &str) -> TimetableEvent {
        TimetableEvent {
            room_url: "EIC317".to_string(),
            building_url: "EIC".to_string(),
            day: crate::model::Weekday::Mon,
            date: "2026-10-05".to_string(),
            start_time: "09:00".to_string(),
            end_time: "10:00".to_string(),
            duration_slots: 2,
            event_type: crate::model::EventType::NonTeaching,
            courses: vec![Course {
                code: code.to_string(),
                title: title.to_string(),
            }],
            speakers: vec![],
        }
    }

    fn temp_path(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join("uclan_timetable_courses_tests");
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(name)
    }

    #[test]
    fn collect_courses_groups_every_title_per_code() {
        let events = vec![
            event("AA100", "Title A"),
            event("BB200", "Title B"),
            event("AA100", "Title A2"),
        ];
        let courses = collect_courses(&events);
        assert_eq!(courses.len(), 2);
        let titles: Vec<&str> = courses["AA100"].iter().map(String::as_str).collect();
        assert_eq!(titles, vec!["Title A", "Title A2"]);
        assert_eq!(courses["BB200"].iter().collect::<Vec<_>>(), vec!["Title B"]);
    }

    #[test]
    fn collect_courses_ignores_events_without_courses() {
        assert!(collect_courses(&[]).is_empty());
        let mut no_course = event("AA100", "Title A");
        no_course.courses.clear();
        assert!(collect_courses(&[no_course]).is_empty());
    }

    #[test]
    fn read_courses_csv_parses_code_title_rows() {
        let map = read_courses_csv("code,title\nAA100,Title A\nBB200,Title B\n").unwrap();
        assert_eq!(map.len(), 2);
        assert_eq!(map["AA100"].iter().collect::<Vec<_>>(), vec!["Title A"]);
        assert_eq!(map["BB200"].iter().collect::<Vec<_>>(), vec!["Title B"]);
    }

    #[test]
    fn read_courses_csv_returns_none_for_rows_with_missing_columns() {
        // The first line is consumed as the header, so the malformed row is
        // the one with a single column below it.
        assert!(read_courses_csv("code,title\nonly-one-column\n").is_none());
    }

    #[test]
    fn read_courses_csv_returns_empty_map_for_empty_input() {
        let map = read_courses_csv("").unwrap();
        assert!(map.is_empty());
    }

    #[test]
    fn merge_courses_file_merges_json() {
        let path = temp_path("courses.json");
        std::fs::write(&path, r#"{"AA100": ["Existing Title"]}"#).unwrap();

        let new = collect_courses(&[event("AA100", "New Title"), event("BB200", "Other")]);
        let merged = merge_courses_file(&path, &OutputFormat::Json, &new);

        assert_eq!(
            merged["AA100"].iter().collect::<Vec<_>>(),
            vec!["Existing Title", "New Title"]
        );
        assert_eq!(merged["BB200"].iter().collect::<Vec<_>>(), vec!["Other"]);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn merge_courses_file_merges_csv() {
        let path = temp_path("courses.csv");
        std::fs::write(&path, "code,title\nAA100,Existing Title\n").unwrap();

        let new = collect_courses(&[event("AA100", "New Title"), event("BB200", "Other")]);
        let merged = merge_courses_file(&path, &OutputFormat::Csv, &new);

        assert_eq!(
            merged["AA100"].iter().collect::<Vec<_>>(),
            vec!["Existing Title", "New Title"]
        );
        assert_eq!(merged["BB200"].iter().collect::<Vec<_>>(), vec!["Other"]);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn merge_courses_file_merges_text() {
        let path = temp_path("courses.txt");
        std::fs::write(&path, "AA100  ---  Existing Title\n").unwrap();

        let new = collect_courses(&[event("AA100", "New Title"), event("BB200", "Other")]);
        let merged = merge_courses_file(&path, &OutputFormat::Text, &new);

        assert_eq!(
            merged["AA100"].iter().collect::<Vec<_>>(),
            vec!["Existing Title", "New Title"]
        );
        assert_eq!(merged["BB200"].iter().collect::<Vec<_>>(), vec!["Other"]);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn merge_courses_file_starts_fresh_when_file_is_missing() {
        let path = temp_path("does-not-exist.json");
        let new = collect_courses(&[event("AA100", "Title A")]);
        let merged = merge_courses_file(&path, &OutputFormat::Json, &new);
        assert_eq!(merged["AA100"].iter().collect::<Vec<_>>(), vec!["Title A"]);
    }
}
