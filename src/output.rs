use std::collections::HashSet;
use std::fs;
use std::io::{self, Write};
use std::path::Path;

use crate::cli::{OutputFormat, WriteMode};
use crate::courses::{CourseMap, merge_courses_file};
use crate::fail;
use crate::filter::sort_events;
use crate::model::{Course, EventType, TimetableEvent, Weekday};

/// Write events as JSON, CSV, or plain text, either to a file or to stdout.
///
/// When `write_mode` is [`WriteMode::Merge`] and `output` names an existing
/// file, the existing events are merged with the new ones before writing.
pub fn write_events_output(
    format: &OutputFormat,
    output: Option<&Path>,
    events: &[TimetableEvent],
    write_mode: WriteMode,
) {
    let merged = match (write_mode, output) {
        (WriteMode::Merge, Some(path)) if path.exists() => {
            merge_events_file(path, format, events)
        }
        _ => events.to_vec(),
    };

    let mut writer = open_writer(output);

    if let Err(err) = match format {
        OutputFormat::Json => write_json(&mut writer, &merged),
        OutputFormat::Csv => write_csv(&mut writer, &merged),
        OutputFormat::Text => write_text(&mut writer, &merged),
    } {
        fail(format!("Failed to write output: {err}"));
    }
}

/// Write courses as JSON, CSV, or plain text.
///
/// When `write_mode` is [`WriteMode::Merge`] and `output` names an existing
/// file, the existing courses are merged with the new ones before writing.
pub fn write_courses_output(
    format: &OutputFormat,
    output: Option<&Path>,
    courses: &CourseMap,
    write_mode: WriteMode,
) {
    let merged = match (write_mode, output) {
        (WriteMode::Merge, Some(path)) if path.exists() => merge_courses_file(path, format, courses),
        _ => courses.clone(),
    };

    let mut writer = open_writer(output);

    if let Err(err) = match format {
        OutputFormat::Json => serde_json::to_writer_pretty(&mut writer, &merged)
            .map_err(io::Error::other),
        OutputFormat::Csv => write_courses_csv(&mut writer, &merged),
        OutputFormat::Text => write_courses_text(&mut writer, &merged),
    } {
        fail(format!("Failed to write output: {err}"));
    }
}

fn open_writer(output: Option<&Path>) -> Box<dyn Write> {
    match output {
        Some(path) => Box::new(fs::File::create(path).unwrap_or_else(|err| {
            fail(format!("Failed to create {}: {err}", path.display()))
        })),
        None => Box::new(io::stdout()),
    }
}

/// Merge the events already stored in `path` into `events`, if that file can be read.
fn merge_events_file(
    path: &Path,
    format: &OutputFormat,
    events: &[TimetableEvent],
) -> Vec<TimetableEvent> {
    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(err) => {
            eprintln!("Failed to read {} ({err}), starting fresh", path.display());
            return events.to_vec();
        }
    };

    let existing = match format {
        OutputFormat::Json => read_events_json(&content).unwrap_or_else(|err| {
            eprintln!("Failed to parse {} ({err}), starting fresh", path.display());
            Vec::new()
        }),
        OutputFormat::Csv => read_events_csv(&content).unwrap_or_else(|| {
            eprintln!("Failed to parse {} as CSV, starting fresh", path.display());
            Vec::new()
        }),
        OutputFormat::Text => return merge_events_text(&content, events),
    };

    merge_events(existing, events)
}

/// Append `new_events` to `existing`, dropping exact duplicates, and sort by date and start time.
fn merge_events(existing: Vec<TimetableEvent>, new_events: &[TimetableEvent]) -> Vec<TimetableEvent> {
    let mut seen = HashSet::new();
    let mut merged = Vec::new();
    for event in existing.into_iter().chain(new_events.iter().cloned()) {
        if seen.insert(event.clone()) {
            merged.push(event);
        }
    }
    sort_events(&mut merged);
    merged
}

fn read_events_json(content: &str) -> Result<Vec<TimetableEvent>, serde_json::Error> {
    serde_json::from_str(content)
}

fn read_events_csv(content: &str) -> Option<Vec<TimetableEvent>> {
    let mut reader = csv::Reader::from_reader(content.as_bytes());
    let mut events = Vec::new();
    for record in reader.records() {
        let record = record.ok()?;
        let room_url = record.get(0)?.to_string();
        let building_url = record.get(1)?.to_string();
        let day = Weekday::parse(record.get(2)?)?;
        let date = record.get(3)?.to_string();
        let start_time = record.get(4)?.to_string();
        let end_time = record.get(5)?.to_string();
        let duration_slots = record.get(6)?.parse().ok()?;
        let event_type = EventType::parse(record.get(7)?);
        let courses = parse_courses_csv(record.get(8)?);
        let speakers = parse_speakers_csv(record.get(9)?);
        events.push(TimetableEvent {
            room_url,
            building_url,
            day,
            date,
            start_time,
            end_time,
            duration_slots,
            event_type,
            courses,
            speakers,
        });
    }
    Some(events)
}

fn merge_events_text(content: &str, events: &[TimetableEvent]) -> Vec<TimetableEvent> {
    let mut seen = HashSet::new();
    let mut merged = Vec::new();

    for line in content.lines() {
        if seen.insert(line.to_string())
            && let Some(event) = parse_event_text(line)
        {
            merged.push(event);
        }
    }

    for event in events {
        let line = event.to_string();
        if seen.insert(line.clone()) {
            if let Some(parsed) = parse_event_text(&line) {
                merged.push(parsed);
            } else {
                merged.push(event.clone());
            }
        }
    }

    sort_events(&mut merged);
    merged
}

fn parse_event_text(line: &str) -> Option<TimetableEvent> {
    let room_url = take_field(line, "Room: ", " - Building: ")?.to_string();
    let building_url = take_field(line, " - Building: ", " - Day: ")?.to_string();
    let day = Weekday::parse(take_field(line, " - Day: ", " - Date: ")?)?;
    let date = take_field(line, " - Date: ", " - Start: ")?.to_string();
    let start_time = take_field(line, " - Start: ", " - End: ")?.to_string();
    let end_time = take_field(line, " - End: ", " - Took: ")?.to_string();
    let duration_slots = take_field(line, " - Took: ", " - Type: ")?.parse().ok()?;
    let event_type = EventType::parse(take_field(line, " - Type: ", " - Courses: ")?);
    let courses = parse_courses_text(take_field(line, " - Courses: ", " - Speaker: ")?);
    let speakers = parse_speakers_text(take_field(line, " - Speaker: ", "")?);

    Some(TimetableEvent {
        room_url,
        building_url,
        day,
        date,
        start_time,
        end_time,
        duration_slots,
        event_type,
        courses,
        speakers,
    })
}

/// Return the text between `prefix` and `suffix`, or the rest of `line` when `suffix` is empty.
fn take_field<'a>(line: &'a str, prefix: &str, suffix: &str) -> Option<&'a str> {
    let start = line.find(prefix)? + prefix.len();
    let rest = &line[start..];
    let end = if suffix.is_empty() {
        rest.len()
    } else {
        rest.find(suffix)?
    };
    Some(&rest[..end])
}

fn parse_courses_csv(value: &str) -> Vec<Course> {
    value
        .split("; ")
        .filter(|item| !item.trim().is_empty())
        .map(|item| {
            item.split_once(" - ")
                .map(|(code, title)| Course {
                    code: code.trim().to_string(),
                    title: title.trim().to_string(),
                })
                .unwrap_or_else(|| Course {
                    code: item.trim().to_string(),
                    title: String::new(),
                })
        })
        .collect()
}

fn parse_courses_text(value: &str) -> Vec<Course> {
    value
        .split(", ")
        .filter(|item| !item.trim().is_empty())
        .map(|item| {
            item.split_once(" -> ")
                .map(|(code, title)| Course {
                    code: code.trim().to_string(),
                    title: title.trim().to_string(),
                })
                .unwrap_or_else(|| Course {
                    code: item.trim().to_string(),
                    title: String::new(),
                })
        })
        .collect()
}

fn parse_speakers_csv(value: &str) -> Vec<String> {
    value
        .split("; ")
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(String::from)
        .collect()
}

fn parse_speakers_text(value: &str) -> Vec<String> {
    value
        .split(", ")
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(String::from)
        .collect()
}

fn write_json(writer: &mut dyn Write, events: &[TimetableEvent]) -> io::Result<()> {
    serde_json::to_writer_pretty(writer, events).map_err(io::Error::other)
}

fn write_csv(writer: &mut dyn Write, events: &[TimetableEvent]) -> io::Result<()> {
    let mut csv_writer = csv::Writer::from_writer(writer);
    csv_writer.write_record([
        "room_url",
        "building_url",
        "day",
        "date",
        "start_time",
        "end_time",
        "duration_slots",
        "event_type",
        "courses",
        "speakers",
    ])?;

    for event in events {
        let courses = event
            .courses
            .iter()
            .map(|course| format!("{} - {}", course.code, course.title))
            .collect::<Vec<_>>()
            .join("; ");
        let record = [
            event.room_url.clone(),
            event.building_url.clone(),
            event.day.to_string(),
            event.date.clone(),
            event.start_time.clone(),
            event.end_time.clone(),
            event.duration_slots.to_string(),
            event.event_type.to_string(),
            courses,
            event.speakers.join("; "),
        ];
        csv_writer.write_record(&record)?;
    }

    csv_writer.flush()?;
    Ok(())
}

fn write_text(writer: &mut dyn Write, events: &[TimetableEvent]) -> io::Result<()> {
    for event in events {
        writeln!(writer, "{event}")?;
    }
    Ok(())
}

fn write_courses_csv(writer: &mut dyn Write, courses: &CourseMap) -> io::Result<()> {
    let mut csv_writer = csv::Writer::from_writer(writer);
    csv_writer.write_record(["code", "title"])?;
    for (code, titles) in courses {
        for title in titles {
            csv_writer.write_record([code.as_str(), title.as_str()])?;
        }
    }
    csv_writer.flush()?;
    Ok(())
}

fn write_courses_text(writer: &mut dyn Write, courses: &CourseMap) -> io::Result<()> {
    for (code, titles) in courses {
        for title in titles {
            writeln!(writer, "{code}  ---  {title}")?;
        }
    }
    Ok(())
}
