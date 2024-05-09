use std::str::FromStr;

use crate::prelude::*;

#[bitflags]
#[repr(u8)]
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum Race {
    Unknown = 1 << 1,
    Asura = 1 << 2,
    Charr = 1 << 3,
    Human = 1 << 4,
    Norn = 1 << 5,
    Sylvari = 1 << 6,
}

impl Race {
    #[allow(dead_code)]
    fn from_link_id(race_id: u32) -> Race {
        match race_id {
            0 => Race::Asura,
            1 => Race::Charr,
            2 => Race::Human,
            3 => Race::Norn,
            4 => Race::Sylvari,
            _ => Race::Unknown,
        }
    }
}

impl FromStr for Race {
    type Err = &'static str;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Ok(match s {
            "Asura" => Self::Asura,
            "Charr" => Self::Charr,
            "Human" => Self::Human,
            "Norn" => Self::Norn,
            "Sylvari" => Self::Sylvari,
            _ => Self::Unknown,
        })
    }
}

impl AsRef<str> for Race {
    fn as_ref(&self) -> &'static str {
        match self {
            Self::Asura => "Asura",
            Self::Charr => "Charr",
            Self::Human => "Human",
            Self::Norn => "Norn",
            Self::Sylvari => "Sylvari",
            Self::Unknown => "Unknown",
        }
    }
}

impl std::fmt::Display for Race {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_ref())
    }
}
