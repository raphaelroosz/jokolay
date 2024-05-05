use serde::{Deserialize, Serialize};

#[derive(Clone)]
pub struct ComponentMessage {
    data: Vec<u8>,
}
#[derive(Clone, Default)]
pub struct ComponentResult {
    data: Vec<u8>,
}

pub fn default_component_result() -> ComponentResult {
    ComponentResult::default()
}

pub fn to_data<T>(value: T) -> ComponentMessage
where
    T: Serialize,
{
    ComponentMessage {
        data: bincode::serialize(&value).unwrap(),
    }
}
pub fn to_broadcast<T>(value: T) -> ComponentResult
where
    T: Serialize,
{
    ComponentResult {
        data: bincode::serialize(&value).unwrap(),
    }
}

pub fn from_data<'a, T>(value: &'a ComponentMessage) -> T
where
    T: Deserialize<'a>,
{
    bincode::deserialize(&value.data).unwrap()
}

pub fn from_broadcast<'a, T>(value: &'a ComponentResult) -> T
where
    T: Deserialize<'a>,
{
    bincode::deserialize(&value.data).unwrap()
}
