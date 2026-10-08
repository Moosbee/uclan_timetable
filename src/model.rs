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
                .join(", "),
            self.speakers.join(", ")
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
