use serde::ser::SerializeMap;
use serde::{Deserialize, Serialize, Serializer};
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TimetableEvent {
    pub room_url: String,
    pub building_url: String,
    pub day: Weekday,
    #[serde(default)]
    pub date: String,
    pub start_time: String,
    pub end_time: String,
    pub duration_slots: i32,
    pub event_type: EventType,
    pub courses: Vec<Course>,
    pub speakers: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Course {
    pub code: String,
    pub title: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Weekday {
    Mon = 0,
    Tue = 1,
    Wed = 2,
    Thu = 3,
    Fri = 4,
    Sat = 5,
    Sun = 6,
}

impl Weekday {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "Mon" => Some(Self::Mon),
            "Tue" => Some(Self::Tue),
            "Wed" => Some(Self::Wed),
            "Thu" => Some(Self::Thu),
            "Fri" => Some(Self::Fri),
            "Sat" => Some(Self::Sat),
            "Sun" => Some(Self::Sun),
            _ => None,
        }
    }
}

impl FromStr for Weekday {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value).ok_or_else(|| {
            format!("unknown weekday `{value}` (expected Mon, Tue, Wed, Thu, Fri, Sat, or Sun)")
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
            Self::Sat => "Sat",
            Self::Sun => "Sun",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum EventType {
    Teaching {
        category: String,
        delivery: Option<String>,
    },
    NonTeaching,
    Other(String),
}

impl EventType {
    pub fn parse(value: &str) -> Self {
        if value == "Non-Teaching" {
            return Self::NonTeaching;
        }

        // `Other` values are displayed as `Other(<value>)` so they survive a
        // text/CSV round trip without being mistaken for a teaching category.
        if let Some(inner) = value
            .strip_prefix("Other(")
            .and_then(|v| v.strip_suffix(')'))
        {
            return Self::Other(inner.to_string());
        }

        if let Some((category, delivery)) = value.split_once(" (")
            && let Some(delivery) = delivery.strip_suffix(')')
        {
            return Self::Teaching {
                category: category.to_string(),
                delivery: Some(delivery.to_string()),
            };
        }

        if value.is_empty() {
            // An empty type (e.g. from a malformed timetable row) is kept as
            // an unknown type rather than being treated as non-teaching.
            Self::Other(String::new())
        } else {
            Self::Teaching {
                category: value.to_string(),
                delivery: None,
            }
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
            Self::Other(value) => write!(formatter, "Other({value})"),
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
                if map.get("type").and_then(serde_json::Value::as_str) == Some("non-teaching") {
                    return Ok(Self::NonTeaching);
                }
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
            "Room: {} - Building: {} - Day: {} - Date: {} - Start: {} - End: {} - Took: {} - Type: {} - Courses: {} - Speaker: {}",
            self.room_url,
            self.building_url,
            self.day,
            self.date,
            self.start_time,
            self.end_time,
            self.duration_slots,
            self.event_type,
            self.courses
                .iter()
                .map(|course| format!("{} -> {}", course.code, course.title))
                .collect::<Vec<_>>()
                .join("; "),
            self.speakers.join("; ")
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Building {
    pub url: Option<String>,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Room {
    pub building_url: String,
    pub room_url: Option<String>,
    pub room_name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Cache {
    pub buildings: Vec<Building>,
    pub rooms: Vec<Room>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event() -> TimetableEvent {
        TimetableEvent {
            room_url: "EIC317".to_string(),
            building_url: "EIC".to_string(),
            day: Weekday::Thu,
            date: "2026-10-08".to_string(),
            start_time: "09:00".to_string(),
            end_time: "10:00".to_string(),
            duration_slots: 2,
            event_type: EventType::Teaching {
                category: "Lecture".to_string(),
                delivery: Some("Seminar".to_string()),
            },
            courses: vec![
                Course {
                    code: "CSC100".to_string(),
                    title: "Computing".to_string(),
                },
                Course {
                    code: "CSC101".to_string(),
                    title: "Programming".to_string(),
                },
            ],
            speakers: vec!["Smith, John".to_string(), "Doe, Jane".to_string()],
        }
    }

    #[test]
    fn weekday_parse_accepts_every_three_letter_name() {
        assert_eq!(Weekday::parse("Mon"), Some(Weekday::Mon));
        assert_eq!(Weekday::parse("Tue"), Some(Weekday::Tue));
        assert_eq!(Weekday::parse("Wed"), Some(Weekday::Wed));
        assert_eq!(Weekday::parse("Thu"), Some(Weekday::Thu));
        assert_eq!(Weekday::parse("Fri"), Some(Weekday::Fri));
        assert_eq!(Weekday::parse("Sat"), Some(Weekday::Sat));
        assert_eq!(Weekday::parse("Sun"), Some(Weekday::Sun));
    }

    #[test]
    fn weekday_parse_rejects_unknown_names() {
        assert_eq!(Weekday::parse("Monday"), None);
        assert_eq!(Weekday::parse("mon"), None);
        assert_eq!(Weekday::parse(""), None);
    }

    #[test]
    fn weekday_from_str_parses_and_reports_errors() {
        use std::str::FromStr;
        assert_eq!(Weekday::from_str("Mon").unwrap(), Weekday::Mon);
        let err = Weekday::from_str("Funday").unwrap_err();
        assert!(err.contains("Funday"), "unexpected error: {err}");
    }

    #[test]
    fn weekday_displays_three_letter_name() {
        assert_eq!(Weekday::Sun.to_string(), "Sun");
        assert_eq!(Weekday::Wed.to_string(), "Wed");
    }

    #[test]
    fn weekday_serializes_to_name_and_deserializes_back() {
        let json = serde_json::to_value(Weekday::Thu).unwrap();
        assert_eq!(json, serde_json::json!("Thu"));
        let day: Weekday = serde_json::from_value(json).unwrap();
        assert_eq!(day, Weekday::Thu);
    }

    #[test]
    fn event_type_parse_handles_non_teaching() {
        assert_eq!(EventType::parse("Non-Teaching"), EventType::NonTeaching);
    }

    #[test]
    fn event_type_parse_extracts_category_and_delivery() {
        assert_eq!(
            EventType::parse("Lecture (Seminar)"),
            EventType::Teaching {
                category: "Lecture".to_string(),
                delivery: Some("Seminar".to_string()),
            }
        );
    }

    #[test]
    fn event_type_parse_treats_plain_text_as_teaching_category() {
        assert_eq!(
            EventType::parse("Workshop"),
            EventType::Teaching {
                category: "Workshop".to_string(),
                delivery: None,
            }
        );
    }

    #[test]
    fn event_type_parse_returns_other_for_empty_string() {
        // An empty event type is classified as an unknown type (Other) rather
        // than non-teaching; it round-trips via the `Other(...)` display form.
        assert_eq!(EventType::parse(""), EventType::Other(String::new()));
    }

    #[test]
    fn event_type_display_round_trips_teaching_and_non_teaching() {
        let teaching = EventType::Teaching {
            category: "Lecture".to_string(),
            delivery: Some("Seminar".to_string()),
        };
        assert_eq!(EventType::parse(&teaching.to_string()), teaching);
        assert_eq!(
            EventType::parse(&EventType::NonTeaching.to_string()),
            EventType::NonTeaching
        );
    }

    #[test]
    fn event_type_other_round_trips_through_display() {
        // `Other` values are written as `Other(<value>)` so that parsing the
        // Display form reconstructs the same variant instead of a Teaching.
        let original = EventType::Other("Workshop".to_string());
        assert_eq!(EventType::parse(&original.to_string()), original);

        let empty = EventType::Other(String::new());
        assert_eq!(EventType::parse(&empty.to_string()), empty);
    }

    #[test]
    fn event_type_serializes_teaching_as_object() {
        let value = serde_json::to_value(EventType::Teaching {
            category: "Lecture".to_string(),
            delivery: Some("Seminar".to_string()),
        })
        .unwrap();
        assert_eq!(
            value,
            serde_json::json!({"type": "teaching", "category": "Lecture", "delivery": "Seminar"})
        );
    }

    #[test]
    fn event_type_serializes_non_teaching_as_string() {
        assert_eq!(
            serde_json::to_value(EventType::NonTeaching).unwrap(),
            serde_json::json!("non-teaching")
        );
    }

    #[test]
    fn event_type_serializes_other_as_raw_string() {
        assert_eq!(
            serde_json::to_value(EventType::Other("Banana".to_string())).unwrap(),
            serde_json::json!("Banana")
        );
    }

    #[test]
    fn event_type_deserializes_string_and_object_forms() {
        let non_teaching: EventType =
            serde_json::from_value(serde_json::json!("non-teaching")).unwrap();
        assert_eq!(non_teaching, EventType::NonTeaching);

        // An object carrying a non-teaching type also maps to NonTeaching.
        let non_teaching_obj: EventType =
            serde_json::from_value(serde_json::json!({"type": "non-teaching"})).unwrap();
        assert_eq!(non_teaching_obj, EventType::NonTeaching);

        let teaching: EventType = serde_json::from_value(serde_json::json!({
            "type": "teaching",
            "category": "Lecture",
            "delivery": "Seminar",
        }))
        .unwrap();
        assert_eq!(
            teaching,
            EventType::Teaching {
                category: "Lecture".to_string(),
                delivery: Some("Seminar".to_string()),
            }
        );

        // The delivery key may be absent entirely.
        let no_delivery: EventType =
            serde_json::from_value(serde_json::json!({"category": "Lab"})).unwrap();
        assert_eq!(
            no_delivery,
            EventType::Teaching {
                category: "Lab".to_string(),
                delivery: None,
            }
        );

        // Any other string becomes an Other event type.
        let other: EventType = serde_json::from_value(serde_json::json!("Workshop")).unwrap();
        assert_eq!(other, EventType::Other("Workshop".to_string()));
    }

    #[test]
    fn event_type_rejects_non_string_non_object_json() {
        let result: Result<EventType, _> = serde_json::from_value(serde_json::json!(42));
        assert!(result.is_err());
    }

    #[test]
    fn timetable_event_displays_all_fields() {
        let text = event().to_string();
        assert!(text.starts_with("Room: EIC317 - Building: EIC - Day: Thu - Date: 2026-10-08"));
        assert!(text.contains(" - Start: 09:00 - End: 10:00 - Took: 2 - Type: Lecture (Seminar)"));
        assert!(text.contains(" - Courses: CSC100 -> Computing; CSC101 -> Programming"));
        assert!(text.contains(" - Speaker: Smith, John; Doe, Jane"));
    }

    #[test]
    fn timetable_event_json_round_trips() {
        let original = event();
        let json = serde_json::to_string(&original).unwrap();
        let parsed: TimetableEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, original);
    }

    #[test]
    fn timetable_event_date_defaults_to_empty_when_missing() {
        let value = serde_json::json!({
            "room_url": "EIC317",
            "building_url": "EIC",
            "day": "Thu",
            "start_time": "09:00",
            "end_time": "10:00",
            "duration_slots": 2,
            "event_type": "non-teaching",
            "courses": [],
            "speakers": [],
        });
        let parsed: TimetableEvent = serde_json::from_value(value).unwrap();
        assert_eq!(parsed.date, "");
    }
}
