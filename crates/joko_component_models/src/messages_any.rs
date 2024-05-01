use std::{any::TypeId, sync::Arc};

use serde::{Deserialize, Serialize};

pub type ComponentDataExchange = Arc<Box<dyn std::any::Any + Send + Sync + 'static>>;

pub fn default_data_exchange() -> ComponentDataExchange {
    Arc::new(Box::new(0))
}

pub fn to_data<T>(value: T) -> ComponentDataExchange
where
    T: Serialize + Clone + Send + Sync + 'static,
{
    Arc::new(Box::new(value))
}

pub fn from_data<'a, T>(value: ComponentDataExchange) -> T
where
    T: Deserialize<'a> + Clone + Send + Sync + 'static,
{
    if let Some(d) = value.downcast_ref::<T>() {
        d.to_owned()
    } else {
        panic!(
            "Bad routing of elements, expected {:?} {:?}",
            TypeId::of::<ComponentDataExchange>(),
            TypeId::of::<T>()
        );
    }
}
