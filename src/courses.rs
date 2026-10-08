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
