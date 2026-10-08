use std::fs;
use std::io::{self, Write};
use std::path::Path;

use crate::cli::OutputFormat;
use crate::courses::{CourseMap, merge_courses_file};
use crate::fail;
use crate::model::TimetableEvent;

/// Write events as JSON, CSV, or plain text, either to a file or to stdout.
pub fn write_events_output(format: &OutputFormat, output: Option<&Path>, events: &[TimetableEvent]) {
    let mut writer = open_writer(output);

    if let Err(err) = match format {
        OutputFormat::Json => write_json(&mut writer, events),
        OutputFormat::Csv => write_csv(&mut writer, events),
        OutputFormat::Text => write_text(&mut writer, events),
    } {
        fail(format!("Failed to write output: {err}"));
    }
}

/// Write courses as JSON, CSV, or plain text, merging any existing output file first.
pub fn write_courses_output(
    format: &OutputFormat,
    output: Option<&Path>,
    courses: &CourseMap,
) {
    let merged = match output {
        Some(path) if path.exists() => merge_courses_file(path, format, courses),
        _ => courses.clone(),
    };

    let mut writer = open_writer(output);

    if let Err(err) = match format {
        OutputFormat::Json => serde_json::to_writer_pretty(&mut writer, &merged)
            .map_err(|err| io::Error::new(io::ErrorKind::Other, err)),
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

fn write_json(writer: &mut dyn Write, events: &[TimetableEvent]) -> io::Result<()> {
    serde_json::to_writer_pretty(writer, events)
        .map_err(|err| io::Error::new(io::ErrorKind::Other, err))
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
