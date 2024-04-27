use serde::{
    de::{SeqAccess, Visitor},
    Deserialize, Serialize,
};

#[repr(C)]
#[derive(Copy, Clone, Debug, Default, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vec3(pub glam::Vec3);

impl From<Vec3> for glam::Vec3 {
    fn from(src: Vec3) -> glam::Vec3 {
        src.0
    }
}

struct Vec3Deserializer;
impl<'de> Visitor<'de> for Vec3Deserializer {
    type Value = Vec3;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("Vec3Deserializer key value sequence.")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let _n: Option<u32> = seq.next_element()?;
        let x: f32 = seq.next_element()?.unwrap();
        let y: f32 = seq.next_element()?.unwrap();
        let z: f32 = seq.next_element()?.unwrap();
        let res = Vec3(glam::Vec3 { x, y, z });
        Ok(res)
    }
}

impl Serialize for Vec3 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeSeq;
        let mut seq = serializer.serialize_seq(Some(2))?;
        seq.serialize_element(&self.0.x)?;
        seq.serialize_element(&self.0.y)?;
        seq.serialize_element(&self.0.z)?;
        seq.end()
    }
}

impl<'de> Deserialize<'de> for Vec3 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_seq(Vec3Deserializer)
    }
}
