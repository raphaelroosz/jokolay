use indexmap::IndexMap;
use uuid::Uuid;

/// This is the activation data per pack
#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct ActivationData {
    /// this is for markers which are global and only activate once regardless of account
    pub global: IndexMap<Uuid, ActivationType>,
    /// this is the activation data per character
    /// for markers which trigger once per character
    pub character: IndexMap<String, IndexMap<Uuid, ActivationType>>,
}
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum ActivationType {
    /// clean these up when the map is changed
    ReappearOnMapChange,
    /// clean these up when the timestamp is reached
    TimeStamp(time::OffsetDateTime),
    Instance(std::net::IpAddr),
}
