use scraper::selectable::Selectable;
use std::collections::HashSet;
use std::fs;

use chrono::{DateTime, Datelike, Duration, Utc};

use crate::cli::Cli;
use crate::model::{Building, Cache, Course, EventType, Room, TimetableEvent, Weekday};

const MAIN_URL: &str = "https://apps.uclan.ac.uk/MvcRoomTimetable/";

/// Scrape the timetable of every known room.
pub fn scrape_all_events(cli: &Cli) -> Vec<TimetableEvent> {
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

/// Read the cached buildings and rooms, scraping and caching them when needed.
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

    let server_millis = parse_server_millis(&html_content);

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

        let Some(day_name) = day_name.as_deref().and_then(Weekday::parse) else {
            continue;
        };
        let date = date_for_weekday(server_millis, day_name);

        for event in events {
            if let Some(event) =
                parse_timetable_event(event, room_url, building_url, day_name, &date)
            {
                timetable_events.push(event);
            }
        }
    }

    timetable_events
}

/// Extract the server-side epoch time (in milliseconds) from the room page.
fn parse_server_millis(html: &str) -> Option<i64> {
    let marker = "var serverMillis = ";
    let start = html.find(marker)? + marker.len();
    let rest = &html[start..];
    let end = rest.find(';')?;
    rest[..end].trim().parse().ok()
}

/// Compute the calendar date (YYYY-MM-DD) of a weekday shown on a room page.
fn date_for_weekday(server_millis: Option<i64>, day: Weekday) -> String {
    let Some(millis) = server_millis else {
        return String::new();
    };
    let Some(today) = DateTime::<Utc>::from_timestamp_millis(millis) else {
        return String::new();
    };
    let today = today.date_naive();
    let today_index = today.weekday().num_days_from_monday() as i64;
    let target_index = day as i64;
    let offset = (target_index - today_index).rem_euclid(7);
    (today + Duration::days(offset))
        .format("%Y-%m-%d")
        .to_string()
}

fn parse_timetable_event(
    event: scraper::ElementRef,
    room_url: &str,
    building_url: &str,
    day: Weekday,
    date: &str,
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
        date: date.to_string(),
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
        eprintln!(
            "Scraping building: {:?} - {:?}",
            building_url, building_name
        );
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_server_millis_from_script_tag() {
        let html = r#"<script>var serverMillis = 1791473128246;</script>"#;
        assert_eq!(parse_server_millis(html), Some(1791473128246));
    }

    #[test]
    fn computes_next_five_days_including_today() {
        // 2026-10-08 15:25:28 UTC is a Thursday.
        let millis = Some(1791473128246);
        assert_eq!(date_for_weekday(millis, Weekday::Thu), "2026-10-08");
        assert_eq!(date_for_weekday(millis, Weekday::Fri), "2026-10-09");
        assert_eq!(date_for_weekday(millis, Weekday::Sat), "2026-10-10");
        assert_eq!(date_for_weekday(millis, Weekday::Sun), "2026-10-11");
        assert_eq!(date_for_weekday(millis, Weekday::Mon), "2026-10-12");
    }
}
