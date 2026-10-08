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
        .split("; ")
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
        .split("; ")
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn base_event(date: &str, start: &str) -> TimetableEvent {
        TimetableEvent {
            room_url: "EIC317".to_string(),
            building_url: "EIC".to_string(),
            day: Weekday::Mon,
            date: date.to_string(),
            start_time: start.to_string(),
            end_time: "10:00".to_string(),
            duration_slots: 2,
            event_type: EventType::Teaching {
                category: "Lecture".to_string(),
                delivery: Some("Seminar".to_string()),
            },
            courses: vec![Course {
                code: "CSC100".to_string(),
                title: "Computing".to_string(),
            }],
            speakers: vec!["Alice".to_string()],
        }
    }

    fn non_teaching_event(date: &str, start: &str) -> TimetableEvent {
        TimetableEvent {
            event_type: EventType::NonTeaching,
            courses: vec![],
            speakers: vec![],
            ..base_event(date, start)
        }
    }

    #[test]
    fn take_field_extracts_text_between_prefix_and_suffix() {
        assert_eq!(
            take_field("Room: EIC317 - Building: EIC", "Room: ", " - Building: "),
            Some("EIC317")
        );
    }

    #[test]
    fn take_field_returns_rest_of_line_when_suffix_is_empty() {
        assert_eq!(
            take_field(" - Speaker: Alice, Bob", " - Speaker: ", ""),
            Some("Alice, Bob")
        );
    }

    #[test]
    fn take_field_returns_none_when_prefix_or_suffix_is_missing() {
        assert_eq!(take_field("no prefix here", "Room: ", " - "), None);
        assert_eq!(take_field("Room: EIC317", "Room: ", " - Missing: "), None);
    }

    #[test]
    fn parse_courses_csv_splits_on_semicolon_separator() {
        let courses = parse_courses_csv("CSC100 - Computing; CSC101 - Programming");
        assert_eq!(courses.len(), 2);
        assert_eq!(courses[0].code, "CSC100");
        assert_eq!(courses[0].title, "Computing");
        assert_eq!(courses[1].code, "CSC101");
        assert_eq!(courses[1].title, "Programming");
    }

    #[test]
    fn parse_courses_text_splits_on_semicolon_separator() {
        let courses = parse_courses_text("CSC100 -> Computing; CSC101 -> Programming");
        assert_eq!(courses.len(), 2);
        assert_eq!(courses[0].code, "CSC100");
        assert_eq!(courses[1].title, "Programming");
    }

    #[test]
    fn parse_speakers_csv_and_text_split_on_their_separators() {
        assert_eq!(
            parse_speakers_csv("Alice; Bob"),
            vec!["Alice".to_string(), "Bob".to_string()]
        );
        assert_eq!(
            parse_speakers_text("Alice; Bob"),
            vec!["Alice".to_string(), "Bob".to_string()]
        );
    }

    #[test]
    fn parse_event_text_reconstructs_the_event() {
        let original = base_event("2026-10-12", "09:00");
        let parsed = parse_event_text(&original.to_string()).unwrap();
        assert_eq!(parsed, original);
    }

    #[test]
    fn text_round_trip_preserves_speaker_names_containing_commas() {
        // Speakers are joined with "; " in the text output, so a single
        // speaker whose name contains ", " is preserved on a round trip.
        let original = TimetableEvent {
            speakers: vec!["Smith, John".to_string()],
            ..base_event("2026-10-12", "09:00")
        };
        let parsed = parse_event_text(&original.to_string()).unwrap();
        assert_eq!(parsed.speakers, vec!["Smith, John".to_string()]);
    }

    #[test]
    fn write_json_emits_a_json_array() {
        let mut out = Vec::new();
        write_json(&mut out, &[base_event("2026-10-12", "09:00")]).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("\"room_url\": \"EIC317\""));
        assert!(text.contains("\"event_type\": {"));
    }

    #[test]
    fn json_round_trips_events() {
        let events = vec![
            base_event("2026-10-12", "09:00"),
            non_teaching_event("2026-10-11", "10:00"),
        ];
        let mut out = Vec::new();
        write_json(&mut out, &events).unwrap();
        let parsed = read_events_json(&String::from_utf8(out).unwrap()).unwrap();
        assert_eq!(parsed, events);
    }

    #[test]
    fn write_csv_emits_header_and_one_row_per_event() {
        let mut out = Vec::new();
        write_csv(&mut out, &[base_event("2026-10-12", "09:00")]).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.starts_with(
            "room_url,building_url,day,date,start_time,end_time,duration_slots,event_type,courses,speakers\n"
        ));
        assert!(text.contains(
            "EIC317,EIC,Mon,2026-10-12,09:00,10:00,2,Lecture (Seminar),CSC100 - Computing,Alice"
        ));
    }

    #[test]
    fn csv_round_trips_events() {
        let events = vec![
            base_event("2026-10-12", "09:00"),
            non_teaching_event("2026-10-11", "10:00"),
        ];
        let mut out = Vec::new();
        write_csv(&mut out, &events).unwrap();
        let parsed = read_events_csv(&String::from_utf8(out).unwrap()).unwrap();
        assert_eq!(parsed, events);
    }

    #[test]
    fn write_text_emits_one_line_per_event() {
        let mut out = Vec::new();
        write_text(&mut out, &[base_event("2026-10-12", "09:00")]).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.starts_with("Room: EIC317 - Building: EIC - Day: Mon"));
        assert!(text.ends_with("- Speaker: Alice\n"));
    }

    #[test]
    fn merge_events_drops_exact_duplicates_and_sorts_by_date() {
        let earlier = non_teaching_event("2026-10-11", "10:00");
        let later = base_event("2026-10-12", "09:00");
        let merged = merge_events(
            vec![later.clone(), earlier.clone()],
            std::slice::from_ref(&later),
        );
        assert_eq!(merged, vec![earlier, later]);
    }

    #[test]
    fn merge_events_text_round_trips_and_sorts_events() {
        let later = base_event("2026-10-12", "09:00");
        let earlier = non_teaching_event("2026-10-11", "10:00");
        let merged = merge_events_text("", &[later.clone(), earlier.clone()]);
        assert_eq!(merged, vec![earlier, later]);
    }

    #[test]
    fn write_courses_csv_emits_code_title_rows() {
        let mut courses = CourseMap::new();
        courses.insert(
            "CSC100".to_string(),
            BTreeSet::from(["Computing".to_string()]),
        );
        let mut out = Vec::new();
        write_courses_csv(&mut out, &courses).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.starts_with("code,title\n"));
        assert!(text.contains("CSC100,Computing"));
    }

    #[test]
    fn write_courses_text_emits_code_title_lines() {
        let mut courses = CourseMap::new();
        courses.insert(
            "CSC100".to_string(),
            BTreeSet::from(["Computing".to_string()]),
        );
        let mut out = Vec::new();
        write_courses_text(&mut out, &courses).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("CSC100  ---  Computing"));
    }
}
