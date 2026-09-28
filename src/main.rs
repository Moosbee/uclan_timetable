use scraper::selectable::Selectable;

const MAIN_URL: &str = "https://apps.uclan.ac.uk/MvcRoomTimetable/";

fn main() {
    eprintln!("Starting web scraping...");
    let buildings = scrape_all_buildings();
    eprintln!("Scraped {} buildings", buildings.len());
    let mut rooms = Vec::new();

    for (building_url, building_name) in buildings {
        if let Some(building_url) = &building_url
            && let Some(building_name) = &building_name
        {
            let mut building_rooms = scrap_building_for_rooms(building_url, building_name);
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
    for (index, (building_url, room_url, room_name)) in rooms.into_iter().enumerate() {
        if index % 100 == 0 {
            eprintln!("Scraped {} rooms out of {}", index, total_rooms);
        }

        if let Some(room_url) = &room_url
            && let Some(room_name) = &room_name
        {
            let timetable = scrap_room_for_timetable(&building_url, room_url, room_name);
        } else {
            eprintln!("No room url found for: {:?} - {:?}", room_url, room_name);
        }
    }
}

fn scrap_room_for_timetable(building_url: &str, room_url: &str, room_name: &str) {
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
            parse_timetable_event(event, &room_url, &building_url, day_name.as_ref().unwrap());
        }
    }
}

fn parse_timetable_event(
    event: scraper::ElementRef,
    room_url: &str,
    building_url: &str,
    week_day: &str, // is only "Mon", "Tue", "Wed", "Thu" or "Fri"
) {
    let span: i32 = event
        .value()
        .attr("colspan")
        .map(|f| f.parse().ok())
        .flatten()
        .unwrap(); // each span is 15min
    let text = event
        .inner_html()
        .replace("<br></b>", "</b><br>")
        .split("<br>")
        .map(|f| f.to_string())
        .collect::<Vec<_>>();
    if !text[0].starts_with("<b>") {
        eprintln!("Skipping event: {:?}", text);
        return;
    }
    let (start_time, end_time) = text[0]
        .replace("<b>", "")
        .replace("</b>", "")
        .split_once(&" - ")
        .map(|f| (f.0.to_string(), f.1.to_string()))
        .unwrap(); // the first element is the time
    let event_type = &text[text.len() - 1]; // the last element is the event type
    let courses_and_speaker = &text[1..text.len() - 1];
    let speaker_split = courses_and_speaker
        .iter()
        .position(|f| f.starts_with("<b>"));
    let courses = speaker_split
        .map(|speaker_split| &courses_and_speaker[0..speaker_split])
        .unwrap_or_else(|| courses_and_speaker)
        .into_iter()
        .map(|f| f.split_once(" - ").unwrap())
        .collect::<Vec<_>>();
    let speaker = speaker_split
        .map(|speaker_split| &courses_and_speaker[speaker_split..])
        .unwrap_or_else(|| &[])
        .into_iter()
        .map(|f| f.replace("<b>", "").replace("</b>", ""))
        .collect::<Vec<_>>()
        .join(", ");

    println!(
        "Room: {} - Building: {} - Day: {} - Start: {} - End: {} - Took: {} - Type: {} - Courses: {} - Speaker: {}",
        room_url,
        building_url,
        week_day,
        start_time,
        end_time,
        span,
        event_type,
        courses
            .iter()
            .map(|f| format!("{} -> {}", f.0, f.1))
            .collect::<Vec<_>>()
            .join(", "),
        speaker
    );
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

fn scrap_building_for_rooms(
    building_url: &str,
    building_name: &str,
) -> Vec<(String, Option<String>, Option<String>)> {
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
