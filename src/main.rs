use clap::{Args, Parser, Subcommand, ValueEnum};
use scraper::selectable::Selectable;
use serde::ser::SerializeMap;
use serde::{Deserialize, Serialize, Serializer};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fmt;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::str::FromStr;

const MAIN_URL: &str = "https://apps.uclan.ac.uk/MvcRoomTimetable/";

#[derive(Parser)]
#[command(
    name = "uclan_timetable",
    version,
    about = "Scrape or process the UCLan room timetable"
)]
struct Cli {
    /// Path to the buildings and rooms cache file (used when scraping).
    #[arg(long, global = true, default_value = "cache.json")]
    cache: PathBuf,

    /// Re-scrape buildings and rooms even if the cache exists.
    #[arg(long, global = true)]
    refresh: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Generate a list of all timetable events, optionally restricted to certain buildings or rooms.
    Events(EventsArgs),

    /// Generate a list of all courses and their titles (one course may have several titles).
    Courses(CoursesArgs),

    /// Filter the timetable by a black/whitelist and/or by weekday.
    Filter(FilterArgs),
}

/// Where timetable data should come from.
#[derive(Debug, Clone)]
enum DataSource {
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
struct SourceArgs {
    /// Where to read timetable data from: "scrape" or a path to a timetable JSON file.
    #[arg(long, default_value = "scrape", value_parser = clap::value_parser!(DataSource))]
    source: DataSource,
}

#[derive(Args, Clone)]
struct OutputArgs {
    /// Output format.
    #[arg(short, long, value_enum, default_value = "json")]
    format: OutputFormat,

    /// Write output to this file instead of stdout. Existing course files are updated.
    #[arg(short, long)]
    output: Option<PathBuf>,
}

#[derive(Args, Clone)]
struct EventsArgs {
    #[command(flatten)]
    source: SourceArgs,

    #[command(flatten)]
    output: OutputArgs,

    /// Restrict to buildings whose identifier (e.g. EIC) matches this text. Repeatable.
    #[arg(short, long)]
    building: Vec<String>,

    /// Restrict to rooms whose identifier (e.g. EIC317) matches this text. Repeatable.
    #[arg(short, long)]
    room: Vec<String>,
}

#[derive(Args, Clone)]
struct CoursesArgs {
    #[command(flatten)]
    source: SourceArgs,

    #[command(flatten)]
    output: OutputArgs,
}

#[derive(Args, Clone)]
struct FilterArgs {
    #[command(flatten)]
    source: SourceArgs,

    #[command(flatten)]
    output: OutputArgs,

    /// File of course codes (with optional "  ---  Title" suffix) to exclude.
    #[arg(long)]
    blacklist: Option<PathBuf>,

    /// File of course codes (with optional "  ---  Title" suffix) to include.
    #[arg(long)]
    whitelist: Option<PathBuf>,

    /// Only include events on these weekdays. Repeatable.
    #[arg(short, long, value_parser = clap::value_parser!(Weekday))]
    weekday: Vec<Weekday>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum OutputFormat {
    Json,
    Csv,
    Text,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
struct TimetableEvent {
    room_url: String,
    building_url: String,
    day: Weekday,
    start_time: String,
    end_time: String,
    duration_slots: i32,
    event_type: EventType,
    courses: Vec<Course>,
    speakers: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
struct Course {
    code: String,
    title: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
enum Weekday {
    Mon = 0,
    Tue = 1,
    Wed = 2,
    Thu = 3,
    Fri = 4,
}

impl Weekday {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "Mon" => Some(Self::Mon),
            "Tue" => Some(Self::Tue),
            "Wed" => Some(Self::Wed),
            "Thu" => Some(Self::Thu),
            "Fri" => Some(Self::Fri),
            _ => None,
        }
    }
}

impl FromStr for Weekday {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value).ok_or_else(|| {
            format!("unknown weekday `{value}` (expected Mon, Tue, Wed, Thu, or Fri)")
        })
    }
}

impl fmt::Display for Weekday {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Mon => "Mon",
            Self::Tue => "Tue",
            Self::Wed => "Wed",
            Self::Thu => "Thu",
            Self::Fri => "Fri",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum EventType {
    Teaching {
        category: String,
        delivery: Option<String>,
    },
    NonTeaching,
    Other(String),
}

impl EventType {
    fn parse(value: &str) -> Self {
        if value == "Non-Teaching" {
            return Self::NonTeaching;
        }

        if let Some((category, delivery)) = value.split_once(" (")
            && let Some(delivery) = delivery.strip_suffix(')')
        {
            return Self::Teaching {
                category: category.to_string(),
                delivery: Some(delivery.to_string()),
            };
        }

        if !value.is_empty() {
            Self::Teaching {
                category: value.to_string(),
                delivery: None,
            }
        } else {
            Self::Other(value.to_string())
        }
    }
}

impl fmt::Display for EventType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Teaching {
                category,
                delivery: Some(delivery),
            } => {
                write!(formatter, "{} ({})", category, delivery)
            }
            Self::Teaching {
                category,
                delivery: None,
            } => formatter.write_str(category),
            Self::NonTeaching => formatter.write_str("Non-Teaching"),
            Self::Other(value) => formatter.write_str(value),
        }
    }
}

impl Serialize for EventType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Teaching { category, delivery } => {
                let mut map = serializer.serialize_map(Some(3))?;
                map.serialize_entry("type", "teaching")?;
                map.serialize_entry("category", category)?;
                map.serialize_entry("delivery", delivery)?;
                map.end()
            }
            Self::NonTeaching => serializer.serialize_str("non-teaching"),
            Self::Other(value) => serializer.serialize_str(value),
        }
    }
}

impl<'de> Deserialize<'de> for EventType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        match value {
            serde_json::Value::String(value) => {
                if value == "non-teaching" {
                    Ok(Self::NonTeaching)
                } else {
                    Ok(Self::Other(value))
                }
            }
            serde_json::Value::Object(map) => {
                let category = map
                    .get("category")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let delivery = map
                    .get("delivery")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string);
                Ok(Self::Teaching { category, delivery })
            }
            _ => Err(serde::de::Error::custom(
                "event_type must be a string or an object",
            )),
        }
    }
}

impl fmt::Display for TimetableEvent {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "Room: {} - Building: {} - Day: {} - Start: {} - End: {} - Took: {} - Type: {} - Courses: {} - Speaker: {}",
            self.room_url,
            self.building_url,
            self.day,
            self.start_time,
            self.end_time,
            self.duration_slots,
            self.event_type,
            self.courses
                .iter()
                .map(|course| format!("{} -> {}", course.code, course.title))
                .collect::<Vec<_>>()
                .join(", "),
            self.speakers.join(", ")
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Building {
    url: Option<String>,
    name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Room {
    building_url: String,
    room_url: Option<String>,
    room_name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Cache {
    buildings: Vec<Building>,
    rooms: Vec<Room>,
}

fn main() {
    let cli = Cli::parse();

    match &cli.command {
        Command::Events(args) => {
            let events = load_events(&args.source.source, &cli);
            let events = filter_events_by_location(events, &args.building, &args.room);
            write_events_output(&args.output.format, args.output.output.as_deref(), &events);
        }
        Command::Courses(args) => {
            let events = load_events(&args.source.source, &cli);
            let courses = collect_courses(&events);
            write_courses_output(&args.output.format, args.output.output.as_deref(), &courses);
        }
        Command::Filter(args) => {
            let events = load_events(&args.source.source, &cli);
            let events = apply_filters(events, args);
            write_events_output(&args.output.format, args.output.output.as_deref(), &events);
        }
    }
}

fn fail(message: String) -> ! {
    eprintln!("{message}");
    std::process::exit(1);
}

fn load_events(source: &DataSource, cli: &Cli) -> Vec<TimetableEvent> {
    let mut events = match source {
        DataSource::Scrape => scrape_all_events(cli),
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
    sort_events(&mut events);
    events
}

fn sort_events(events: &mut [TimetableEvent]) {
    events.sort_by(|a, b| a.day.cmp(&b.day).then_with(|| a.start_time.cmp(&b.start_time)));
}

fn filter_events_by_location(
    events: Vec<TimetableEvent>,
    buildings: &[String],
    rooms: &[String],
) -> Vec<TimetableEvent> {
    events
        .into_iter()
        .filter(|event| {
            let building_matches = buildings.is_empty()
                || buildings.iter().any(|needle| {
                    event.building_url.to_lowercase().contains(&needle.to_lowercase())
                });
            let room_matches = rooms.is_empty()
                || rooms
                    .iter()
                    .any(|needle| event.room_url.to_lowercase().contains(&needle.to_lowercase()));
            building_matches && room_matches
        })
        .collect()
}

fn apply_filters(events: Vec<TimetableEvent>, args: &FilterArgs) -> Vec<TimetableEvent> {
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
                && !event.courses.iter().any(|course| whitelist.contains(&course.code))
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

fn collect_courses(events: &[TimetableEvent]) -> BTreeMap<String, BTreeSet<String>> {
    let mut courses: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
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

fn write_events_output(format: &OutputFormat, output: Option<&Path>, events: &[TimetableEvent]) {
    let mut writer: Box<dyn Write> = match output {
        Some(path) => Box::new(fs::File::create(path).unwrap_or_else(|err| {
            fail(format!("Failed to create {}: {err}", path.display()))
        })),
        None => Box::new(io::stdout()),
    };

    if let Err(err) = match format {
        OutputFormat::Json => write_json(&mut writer, events),
        OutputFormat::Csv => write_csv(&mut writer, events),
        OutputFormat::Text => write_text(&mut writer, events),
    } {
        fail(format!("Failed to write output: {err}"));
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

fn write_courses_output(
    format: &OutputFormat,
    output: Option<&Path>,
    courses: &BTreeMap<String, BTreeSet<String>>,
) {
    let merged = match output {
        Some(path) if path.exists() => merge_courses_file(path, format, courses),
        _ => courses.clone(),
    };

    let mut writer: Box<dyn Write> = match output {
        Some(path) => Box::new(fs::File::create(path).unwrap_or_else(|err| {
            fail(format!("Failed to create {}: {err}", path.display()))
        })),
        None => Box::new(io::stdout()),
    };

    if let Err(err) = match format {
        OutputFormat::Json => serde_json::to_writer_pretty(&mut writer, &merged)
            .map_err(|err| io::Error::new(io::ErrorKind::Other, err)),
        OutputFormat::Csv => write_courses_csv(&mut writer, &merged),
        OutputFormat::Text => write_courses_text(&mut writer, &merged),
    } {
        fail(format!("Failed to write output: {err}"));
    }
}

fn merge_courses_file(
    path: &Path,
    format: &OutputFormat,
    courses: &BTreeMap<String, BTreeSet<String>>,
) -> BTreeMap<String, BTreeSet<String>> {
    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(err) => {
            eprintln!("Failed to read {} ({err}), starting fresh", path.display());
            return courses.clone();
        }
    };

    match format {
        OutputFormat::Json => {
            let mut merged = serde_json::from_str::<BTreeMap<String, BTreeSet<String>>>(&content)
                .unwrap_or_else(|err| {
                    eprintln!(
                        "Failed to parse {} ({err}), starting fresh",
                        path.display()
                    );
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

fn merge_courses(
    target: &mut BTreeMap<String, BTreeSet<String>>,
    source: &BTreeMap<String, BTreeSet<String>>,
) {
    for (code, titles) in source {
        target.entry(code.clone()).or_default().extend(titles.iter().cloned());
    }
}

fn read_courses_csv(content: &str) -> Option<BTreeMap<String, BTreeSet<String>>> {
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

fn write_courses_csv(
    writer: &mut dyn Write,
    courses: &BTreeMap<String, BTreeSet<String>>,
) -> io::Result<()> {
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

fn write_courses_text(
    writer: &mut dyn Write,
    courses: &BTreeMap<String, BTreeSet<String>>,
) -> io::Result<()> {
    for (code, titles) in courses {
        for title in titles {
            writeln!(writer, "{code}  ---  {title}")?;
        }
    }
    Ok(())
}

fn scrape_all_events(cli: &Cli) -> Vec<TimetableEvent> {
    let rooms = load_or_scrape_rooms(cli);

    eprintln!("Scraping timetables for {} rooms...", rooms.len());
    let total_rooms = rooms.len();
    let mut all_events = HashSet::new();

    for (index, room) in rooms.iter().enumerate() {
        if index % 100 == 0 {
            eprintln!("Scraped {} rooms out of {}", index, total_rooms);
        }

        let Some(room_url) = room.room_url.as_deref() else {
            eprintln!("No room url found for: {:?}", room);
            continue;
        };

        let room_name = room.room_name.as_deref().unwrap_or_default();
        let events = scrap_room_for_timetable(&room.building_url, room_url, room_name);
        all_events.extend(events);
    }

    all_events.into_iter().collect()
}

fn load_or_scrape_rooms(cli: &Cli) -> Vec<Room> {
    if !cli.refresh {
        if let Ok(content) = fs::read_to_string(&cli.cache) {
            if let Ok(cache) = serde_json::from_str::<Cache>(&content) {
                if !cache.buildings.is_empty() && !cache.rooms.is_empty() {
                    eprintln!(
                        "Loaded {} buildings and {} rooms from {}",
                        cache.buildings.len(),
                        cache.rooms.len(),
                        cli.cache.display()
                    );
                    return cache.rooms;
                }
            }
        }
    }

    eprintln!("Scraping buildings...");
    let buildings = scrape_all_buildings();
    eprintln!("Scraped {} buildings", buildings.len());

    let mut rooms = Vec::new();
    for building in &buildings {
        if let Some(building_url) = building.url.as_deref() {
            let mut building_rooms = scrap_building_for_rooms(building_url);
            rooms.append(&mut building_rooms);
        } else {
            eprintln!("No building url found for: {:?}", building);
        }
    }
    eprintln!("Scraped {} rooms", rooms.len());

    let cache = Cache { buildings, rooms };
    if let Ok(json) = serde_json::to_string_pretty(&cache) {
        if let Err(err) = fs::write(&cli.cache, json) {
            eprintln!("Failed to write cache to {}: {err}", cli.cache.display());
        } else {
            eprintln!("Wrote buildings and rooms cache to {}", cli.cache.display());
        }
    }

    cache.rooms
}

fn scrap_room_for_timetable(
    building_url: &str,
    room_url: &str,
    room_name: &str,
) -> Vec<TimetableEvent> {
    let url = format!("{}{}{}", MAIN_URL, building_url, room_url);
    let response = reqwest::blocking::get(url.clone());
    let html_content = response.unwrap().text().unwrap();
    let document = scraper::Html::parse_document(&html_content);
    let time_table_selector = scraper::Selector::parse("table.TimeTableTable > tbody").unwrap();

    let time_table_day_selector = scraper::Selector::parse("tr:not(:first-child)").unwrap();

    let time_table_event_selector = scraper::Selector::parse(".TimeTableEvent").unwrap();

    let time_table_body = document
        .select(&time_table_selector)
        .next()
        .expect(&format!(
            "Time Table should exist for Building {} Room {} with url {}",
            building_url, room_name, url
        ));

    let days = time_table_body
        .select(&time_table_day_selector)
        .collect::<Vec<_>>();

    let mut timetable_events = Vec::new();

    for day in days {
        let day_name = day
            .select(
                &scraper::Selector::parse(".TimeTableRowHeader,.TimeTableCurrentRowHeader")
                    .unwrap(),
            )
            .next()
            .map(|f| f.text().map(|f| f.trim()).collect::<Vec<_>>().join(""));
        let events = day.select(&time_table_event_selector).collect::<Vec<_>>();

        for event in events {
            let Some(day_name) = day_name.as_deref().and_then(Weekday::parse) else {
                continue;
            };

            if let Some(event) = parse_timetable_event(event, room_url, building_url, day_name) {
                timetable_events.push(event);
            }
        }
    }

    timetable_events
}

fn parse_timetable_event(
    event: scraper::ElementRef,
    room_url: &str,
    building_url: &str,
    day: Weekday,
) -> Option<TimetableEvent> {
    let duration_slots: i32 = event
        .value()
        .attr("colspan")
        .and_then(|value| value.parse().ok())?;
    let text = event
        .inner_html()
        .replace("<br></b>", "</b><br>")
        .split("<br>")
        .map(|f| f.to_string())
        .collect::<Vec<_>>();
    if !text[0].starts_with("<b>") {
        eprintln!("Skipping event: {:?}", text);
        return None;
    }
    let (start_time, end_time) = text[0]
        .replace("<b>", "")
        .replace("</b>", "")
        .split_once(" - ")
        .map(|(start, end)| (start.to_string(), end.to_string()))?;
    if text.len() < 2 {
        return None;
    }
    let event_type = EventType::parse(&text[text.len() - 1]);
    let courses_and_speaker = &text[1..text.len() - 1];
    let speaker_split = courses_and_speaker
        .iter()
        .position(|f| f.starts_with("<b>"));
    let courses = speaker_split
        .map(|speaker_split| &courses_and_speaker[0..speaker_split])
        .unwrap_or_else(|| courses_and_speaker)
        .into_iter()
        .filter_map(|course| course.split_once(" - "))
        .map(|(code, title)| Course {
            code: code.to_string(),
            title: title.to_string(),
        })
        .collect::<Vec<_>>();
    let speaker = speaker_split
        .map(|speaker_split| &courses_and_speaker[speaker_split..])
        .unwrap_or_else(|| &[])
        .into_iter()
        .map(|f| f.replace("<b>", "").replace("</b>", ""))
        .collect::<Vec<_>>();

    Some(TimetableEvent {
        room_url: room_url.to_string(),
        building_url: building_url.to_string(),
        day,
        start_time,
        end_time,
        duration_slots,
        event_type,
        courses,
        speakers: speaker,
    })
}

fn scrape_all_buildings() -> Vec<Building> {
    let response = reqwest::blocking::get(MAIN_URL);
    let html_content = response.unwrap().text().unwrap();
    let document = scraper::Html::parse_document(&html_content);
    let buildings_table_selector = scraper::Selector::parse("table > tbody").unwrap();
    let table_row_selector = scraper::Selector::parse("tr:not(:first-child)").unwrap();

    let buildings_table_body = document.select(&buildings_table_selector).next().unwrap();

    let rows = buildings_table_body
        .select(&table_row_selector)
        .collect::<Vec<_>>();

    let mut buildings = Vec::new();

    for row in rows {
        let building_url = row
            .select(&scraper::Selector::parse("a:not(:empty)").unwrap())
            .next()
            .map(|f| f.value().attr("href").unwrap().to_string());
        let building_name = row
            .select(&scraper::Selector::parse("td:last-child").unwrap())
            .next()
            .map(|f| f.text().map(|f| f.trim()).collect::<Vec<_>>().join(""));
        eprintln!("Scraping building: {:?} - {:?}", building_url, building_name);
        buildings.push(Building {
            url: building_url,
            name: building_name,
        });
    }

    buildings
}

fn scrap_building_for_rooms(building_url: &str) -> Vec<Room> {
    let response = reqwest::blocking::get(format!("{}/{}", MAIN_URL, building_url));
    let html_content = response.unwrap().text().unwrap();
    let document = scraper::Html::parse_document(&html_content);
    let rooms_table_selector = scraper::Selector::parse("table > tbody").unwrap();
    let room_row_selector = scraper::Selector::parse("tr:not(:first-child)").unwrap();

    let rooms_table_body = document.select(&rooms_table_selector).next().unwrap();

    let rows = rooms_table_body
        .select(&room_row_selector)
        .collect::<Vec<_>>();

    let mut rooms = Vec::new();

    for row in rows {
        let room_url = row
            .select(&scraper::Selector::parse("a:not(:empty)").unwrap())
            .next()
            .map(|f| f.value().attr("href").unwrap().replace('#', ""));
        let room_name = row
            .select(&scraper::Selector::parse("td:nth-child(2)").unwrap())
            .next()
            .map(|f| f.text().map(|f| f.trim()).collect::<Vec<_>>().join(""));
        eprintln!("Scraping room: {:?} - {:?}", room_url, room_name);
        rooms.push(Room {
            building_url: building_url.to_string(),
            room_url,
            room_name,
        });
    }

    rooms
}