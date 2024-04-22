use glam::{Vec2, Vec3};

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct MarkerVertex {
    pub position: Vec3,
    pub alpha: f32,
    pub texture_coordinates: Vec2,
    pub fade_near_far: Vec2,
    pub color: [u8; 4],
}

#[derive(Debug)]
pub struct MarkerObject {
    /// The six vertices that make up the marker quad
    pub vertices: [MarkerVertex; 6],
    /// The (managed) texture id from egui data
    pub texture: u64,
    /// The distance from camera
    /// As markers have transparency, we need to render them from far -> near order
    /// So, we will sort them using this distance just before rendering
    pub distance: f32,
}