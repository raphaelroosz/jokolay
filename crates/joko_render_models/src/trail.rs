use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::marker::MarkerVertex;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrailObject {
    pub vertices: Arc<[MarkerVertex]>,
    pub texture: u64,
}
