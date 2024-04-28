mutually_exclusive_features::exactly_one_of!("messages_any", "messages_bincode");

use std::str::FromStr;

use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

/*
each manager must have
1. a main thread struct
2. an off thread struct
3. commands that they send/receive
4. a public api for other managers to access

*/

pub mod serde_glam;
pub mod task;

/// This newtype is used to represents relative paths in marker packs
/// 1. It won't start with `/` or `C:` like roots, because its a relative path
/// 2. It can be empty to represent current directory
/// 3. No expansion of special characters like  `.` or `..` stuff.
/// 4. It is always lowercase to avoid platform specific quirks.
/// 5. It will use `/` as the path separator.
/// 6. It doesn't mean that the path is valid. It may contain many of the utf-8 characters which are not valid path names on linux/windows
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RelativePath(SmolStr);

impl Serialize for RelativePath {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.0.as_str())
    }
}
impl<'de> Deserialize<'de> for RelativePath {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let r = s.parse().unwrap();
        Ok(r)
    }
}

#[allow(unused)]
impl RelativePath {
    pub fn normalize(path: &str) -> String {
        let normalized_slash = path.replace('\\', "/");
        let trimmed_path = normalized_slash.trim_start_matches('/');
        trimmed_path.to_lowercase()
    }

    pub fn join_str(&self, path: &str) -> Self {
        let normalized_path = RelativePath::normalize(path);
        if normalized_path.is_empty() {
            return Self(self.0.clone());
        }
        if self.0.is_empty() {
            // no need to push `/` if we are empty, as that would make it an absolute path
            return Self(normalized_path.into());
        }

        let mut new = self.0.to_string();
        if !self.0.ends_with('/') {
            new.push('/');
        }
        new.push_str(&normalized_path);
        Self(new.into())
    }

    pub fn ends_with(&self, ext: &str) -> bool {
        self.0.ends_with(ext)
    }
    pub fn is_png(&self) -> bool {
        self.ends_with(".png")
    }
    pub fn is_tbin(&self) -> bool {
        self.ends_with(".trl")
    }
    pub fn is_xml(&self) -> bool {
        self.ends_with(".xml")
    }
    pub fn is_dir(&self) -> bool {
        self.ends_with("/")
    }
    pub fn parent(&self) -> Option<&str> {
        let path = self.0.trim_end_matches('/');
        if path.is_empty() {
            return None;
        }
        path.rfind('/').map(|index| &path[..=index])
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for RelativePath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl From<RelativePath> for String {
    fn from(val: RelativePath) -> String {
        val.0.into()
    }
}
impl FromStr for RelativePath {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let path = RelativePath::normalize(s);
        if path.is_empty() {
            return Ok(Self::default());
        }
        Ok(Self(path.into()))
    }
}
