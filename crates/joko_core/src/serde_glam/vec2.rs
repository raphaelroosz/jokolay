use serde::{
    de::{SeqAccess, Visitor},
    Deserialize, Serialize,
};

#[repr(C)]
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct Vec2(pub glam::Vec2);
#[repr(C)]
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct IVec2(pub glam::IVec2);
#[repr(C)]
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct UVec2(pub glam::UVec2);

impl From<Vec2> for glam::Vec2 {
    fn from(src: Vec2) -> glam::Vec2 {
        src.0
    }
}
impl From<IVec2> for glam::IVec2 {
    fn from(src: IVec2) -> glam::IVec2 {
        src.0
    }
}
impl From<UVec2> for glam::UVec2 {
    fn from(src: UVec2) -> glam::UVec2 {
        src.0
    }
}

unsafe impl bytemuck::Pod for Vec2 {}
unsafe impl bytemuck::Zeroable for Vec2 {
    fn zeroed() -> Self {
        Self::default()
    }
}

struct Vec2Deserializer;
impl<'de> Visitor<'de> for Vec2Deserializer {
    type Value = Vec2;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("Vec2Deserializer key value sequence.")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let _n: Option<u32> = seq.next_element()?;
        let x: f32 = seq.next_element()?.unwrap();
        let y: f32 = seq.next_element()?.unwrap();
        let res = Vec2(glam::Vec2 { x, y });
        Ok(res)
    }
}

impl Serialize for Vec2 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeSeq;
        let mut seq = serializer.serialize_seq(Some(2))?;
        seq.serialize_element(&self.0.x)?;
        seq.serialize_element(&self.0.y)?;
        seq.end()
    }
}

impl<'de> Deserialize<'de> for Vec2 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_seq(Vec2Deserializer)
    }
}

struct IVec2Deserializer;
impl<'de> Visitor<'de> for IVec2Deserializer {
    type Value = IVec2;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("IVec2Deserializer key value sequence.")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let _n: Option<u32> = seq.next_element()?;
        let x: i32 = seq.next_element()?.unwrap();
        let y: i32 = seq.next_element()?.unwrap();
        let res = IVec2(glam::IVec2 { x, y });
        Ok(res)
    }
}
impl Serialize for IVec2 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeSeq;
        let mut seq = serializer.serialize_seq(Some(2))?;
        seq.serialize_element(&self.0.x)?;
        seq.serialize_element(&self.0.y)?;
        seq.end()
    }
}

impl<'de> Deserialize<'de> for IVec2 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_seq(IVec2Deserializer)
    }
}

struct UVec2Deserializer;
impl<'de> Visitor<'de> for UVec2Deserializer {
    type Value = UVec2;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("UVec2Deserializer key value sequence.")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let _n: Option<u32> = seq.next_element()?;
        let x: u32 = seq.next_element()?.unwrap();
        let y: u32 = seq.next_element()?.unwrap();
        let res = UVec2(glam::UVec2 { x, y });
        Ok(res)
    }
}

impl Serialize for UVec2 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeSeq;
        let mut seq = serializer.serialize_seq(Some(2))?;
        seq.serialize_element(&self.0.x)?;
        seq.serialize_element(&self.0.y)?;
        seq.end()
    }
}

impl<'de> Deserialize<'de> for UVec2 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_seq(UVec2Deserializer)
    }
}
