use std::sync::Arc;

use crate::marker::MarkerVertex;

#[derive(Debug, Clone)]
pub struct TrailObject {
    pub vertices: Arc<[MarkerVertex]>,
    pub texture: u64,
}
