mutually_exclusive_features::exactly_one_of!("messages_any", "messages_bincode");
pub enum MessageToApplicationBack {
    SaveUIConfiguration(String),
}
