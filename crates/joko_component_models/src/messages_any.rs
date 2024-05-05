use std::{any::TypeId, sync::Arc};

use serde::{Deserialize, Serialize};

#[derive(Clone)]
pub struct ComponentMessage {
    data: Arc<Box<dyn std::any::Any + Send + Sync + 'static>>,
}

#[derive(Clone)]
pub struct ComponentResult {
    //TODO: remove  + Send + Sync
    data: Arc<Box<dyn std::any::Any + Send + Sync + 'static>>,
}

pub fn default_component_result() -> ComponentResult {
    ComponentResult {
        data: Arc::new(Box::new(0)),
    }
}

pub fn to_data<T>(value: T) -> ComponentMessage
where
    T: Serialize + Clone + Send + Sync + 'static,
{
    ComponentMessage {
        data: Arc::new(Box::new(value)),
    }
}
pub fn to_broadcast<T>(value: T) -> ComponentResult
where
    T: Serialize + Clone + Send + Sync + 'static, //TODO: remove  + Send + Sync
{
    ComponentResult {
        data: Arc::new(Box::new(value)),
    }
}

pub fn from_data<'a, T>(value: &'a ComponentMessage) -> T
where
    T: Deserialize<'a> + Clone + Send + Sync + 'static,
{
    if let Some(d) = value.data.downcast_ref::<T>() {
        d.to_owned()
    } else {
        panic!(
            "Bad routing of elements, expected {:?} {:?}",
            TypeId::of::<ComponentMessage>(),
            TypeId::of::<T>()
        );
    }
}

pub fn from_broadcast<'a, T>(value: &'a ComponentResult) -> T
where
    T: Deserialize<'a> + Clone + Send + Sync + 'static, //TODO: remove  + Send + Sync
{
    if let Some(d) = value.data.downcast_ref::<T>() {
        d.to_owned()
    } else {
        panic!(
            "Bad routing of elements, expected {:?} {:?}",
            TypeId::of::<ComponentResult>(),
            TypeId::of::<T>()
        );
    }
}
