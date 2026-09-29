use scraper::selectable::Selectable;
use std::{collections::HashSet, fmt, fs};

const MAIN_URL: &str = "https://apps.uclan.ac.uk/MvcRoomTimetable/";

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Course {
    code: String,
    title: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
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

fn main() {
    eprintln!("Starting web scraping...");
    let buildings = scrape_all_buildings();
    eprintln!("Scraped {} buildings", buildings.len());
    let mut rooms = Vec::new();

    for (building_url, building_name) in buildings {
        if let Some(building_url) = &building_url
            && building_name.is_some()
        {
            let mut building_rooms = scrap_building_for_rooms(building_url);
            rooms.append(&mut building_rooms);
        } else {
            eprintln!(
                "No building url found for: {:?} - {:?}",
                building_url, building_name
            );
        }
    }

    eprintln!("Scraped {} rooms", rooms.len());
    let total_rooms = rooms.len();
    let mut all_events = HashSet::new();
    for (index, (building_url, room_url, room_name)) in rooms.into_iter().enumerate() {
        if index % 100 == 0 {
            eprintln!("Scraped {} rooms out of {}", index, total_rooms);
        }

        if let Some(room_url) = &room_url
            && let Some(room_name) = &room_name
        {
            let events = scrap_room_for_timetable(&building_url, room_url, room_name);
            for event in events {
                all_events.insert(event);
            }
        } else {
            eprintln!("No room url found for: {:?} - {:?}", room_url, room_name);
        }
    }

    let mut all_events = all_events.into_iter().collect::<Vec<_>>();

    all_events.sort_by(|a, b| {
        a.day
            .cmp(&b.day)
            .then_with(|| a.start_time.cmp(&b.start_time))
    });

    let course_ids = all_events
        .iter()
        .flat_map(|event| {
            event
                .courses
                .iter()
                .map(|course| (course.code.clone(), course.title.clone()))
        })
        .collect::<HashSet<_>>();

    let mut course_ids = course_ids.into_iter().collect::<Vec<_>>();

    course_ids.sort();

    println!("\n\n\n\n");

    for course in course_ids {
        println!("{}  ---  {}", course.0, course.1);
    }

    println!("\n\n\n\n");

    for event in &all_events {
        println!("{}", event);
    }

    let blacklist = fs::read_to_string("course_blacklist.txt").unwrap();
    let blacklist = blacklist
        .split('\n')
        .map(|f| f.split_once("  ---  ").unwrap().0.to_string())
        .collect::<HashSet<_>>();

    let filtered_events = all_events
        .into_iter()
        .filter(|event| match &event.event_type {
            EventType::Teaching { category, delivery } => {
                category.contains("Lecture")
                    && event
                        .courses
                        .iter()
                        .all(|course| !blacklist.contains(&course.code))
            }
            _ => false,
        })
        .collect::<Vec<_>>();

    println!("\n\n\n\n");

    for event in &filtered_events {
        println!("{}", event);
    }
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

fn scrape_all_buildings() -> Vec<(Option<String>, Option<String>)> {
    let response = reqwest::blocking::get(MAIN_URL);
    let html_content = response.unwrap().text().unwrap();
    let document = scraper::Html::parse_document(&html_content);
    let buildings_table_selector = scraper::Selector::parse("table > tbody").unwrap();
    let table_row_selector = scraper::Selector::parse("tr:not(:first-child)").unwrap();

    let buildings_table_body = document.select(&buildings_table_selector).next().unwrap();

    let rows = buildings_table_body
        .select(&table_row_selector)
        .collect::<Vec<_>>();

    let mut buildings: Vec<(Option<String>, Option<String>)> = Vec::new();

    for row in rows {
        let building_url = row
            .select(&scraper::Selector::parse("a:not(:empty)").unwrap())
            .next()
            .map(|f| f.value().attr("href").unwrap());
        let building_name = row
            .select(&scraper::Selector::parse("td:last-child").unwrap())
            .next()
            .map(|f| f.text().map(|f| f.trim()).collect::<Vec<_>>().join(""));
        println!(
            "Scraping building: {:?} - {:?}",
            building_url, building_name
        );
        buildings.push((building_url.map(|f| f.to_string()), building_name));
    }

    buildings
}

fn scrap_building_for_rooms(building_url: &str) -> Vec<(String, Option<String>, Option<String>)> {
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
            .map(|f| f.value().attr("href").unwrap().replace("#", ""));
        let room_name = row
            .select(&scraper::Selector::parse("td:nth-child(2)").unwrap())
            .next()
            .map(|f| f.text().map(|f| f.trim()).collect::<Vec<_>>().join(""));
        println!("Scraping room: {:?} - {:?}", room_url, room_name);
        rooms.push((building_url.to_string(), room_url, room_name));
    }

    rooms
}
