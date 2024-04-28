pub type ComponentDataExchange = Vec<u8>;

pub fn default_data_exchange() -> ComponentDataExchange {
    ComponentDataExchange::default()
}

pub fn to_data<T>(value: T) -> ComponentDataExchange
where
    T: Serialize,
{
    bincode::serialize(&value).unwrap()
}

pub fn from_data<'a, T>(value: &'a ComponentDataExchange) -> T
where
    T: Deserialize<'a>,
{
    bincode::deserialize(&value).unwrap()
}
