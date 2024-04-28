use std::sync::Arc;

use serde::{Deserialize, Serialize};

pub type ComponentDataExchange = Arc<Box<dyn std::any::Any + Send + Sync + 'static>>;

pub fn default_data_exchange() -> ComponentDataExchange {
    Arc::new(Box::new(0))
}

pub fn to_data<'a, T>(value: T) -> ComponentDataExchange
where
    T: Serialize + Clone + Send + Sync + 'static,
{
    Arc::new(Box::new(T::from(value)))
}

pub fn from_data<'a, T>(value: ComponentDataExchange) -> T
where
    T: Deserialize<'a> + Clone + Send + Sync + 'static,
{
    use downcast_rs::Downcast;

    let a = value.as_any();
    let d = a.downcast_ref::<T>();
    let res = d.unwrap().to_owned();
    res
}
