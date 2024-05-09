use std::str::FromStr;

use crate::prelude::*;

/// Filter which professions the marker should be active for. if its null, its available for all professions
#[bitflags]
#[repr(u16)]
#[derive(Debug, Clone, Copy)]
pub enum Profession {
    Elementalist = 1 << 0,
    Engineer = 1 << 1,
    Guardian = 1 << 2,
    Mesmer = 1 << 3,
    Necromancer = 1 << 4,
    Ranger = 1 << 5,
    Revenant = 1 << 6,
    Thief = 1 << 7,
    Warrior = 1 << 8,
}

impl FromStr for Profession {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "guardian" => Profession::Guardian,
            "warrior" => Profession::Warrior,
            "engineer" => Profession::Engineer,
            "ranger" => Profession::Ranger,
            "thief" => Profession::Thief,
            "elementalist" => Profession::Elementalist,
            "mesmer" => Profession::Mesmer,
            "necromancer" => Profession::Necromancer,
            "revenant" => Profession::Revenant,
            _ => return Err("invalid profession"),
        })
    }
}

impl AsRef<str> for Profession {
    fn as_ref(&self) -> &str {
        match self {
            Profession::Guardian => "guardian",
            Profession::Warrior => "warrior",
            Profession::Engineer => "engineer",
            Profession::Ranger => "ranger",
            Profession::Thief => "thief",
            Profession::Elementalist => "elementalist",
            Profession::Mesmer => "mesmer",
            Profession::Necromancer => "necromancer",
            Profession::Revenant => "revenant",
        }
    }
}

impl std::fmt::Display for Profession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_ref())
    }
}
