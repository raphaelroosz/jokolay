use joko_package_models::attributes::CommonAttributes;
use jokoapi::end_point::mounts::Mount;
use ordered_hash_map::OrderedHashMap;

use egui::TextureHandle;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::INCHES_PER_METER;
use joko_core::{
    serde_glam::{Vec2, Vec3},
    RelativePath,
};
use joko_render_models::{
    marker::{MarkerObject, MarkerVertex},
    trail::TrailObject,
};
use joko_link::MumbleLink;

/*
- activation data with uuids and track the latest timestamp that will be activated
- category activation data -> track and changes to propagate to markers of this map
- current active markers, which will keep track of their original marker, so as to propagate any changes easily
*/
#[derive(Clone)]
pub struct ActiveTrail {
    pub trail_object: TrailObject,
    pub texture_handle: TextureHandle,
}
/// This is an active marker.
/// It stores all the info that we need to scan every frame
#[derive(Clone)]
pub(crate) struct ActiveMarker {
    /// texture id from managed textures
    pub texture_id: u64,
    /// owned texture handle to keep it alive
    pub _texture: TextureHandle,
    /// position
    pub pos: Vec3,
    /// billboard must not be bigger than this size in pixels
    pub max_pixel_size: f32,
    /// billboard must not be smaller than this size in pixels
    pub min_pixel_size: f32,
    pub common_attributes: CommonAttributes,
}

pub const BILLBOARD_MAX_VISIBILITY_DISTANCE_IN_GAME: f32 = 20000.0; // in game metric, for GW2, inches

impl ActiveMarker {
    pub fn get_vertices_and_texture(&self, link: &MumbleLink, z_near: f32) -> Option<MarkerObject> {
        let Self {
            texture_id,
            pos,
            common_attributes: attrs,
            _texture,
            max_pixel_size,
            min_pixel_size,
            ..
        } = self;
        // let width = *width;
        // let height = *height;
        let texture_id = *texture_id;
        let pos = *pos;
        // filters
        if let Some(mounts) = attrs.get_mount() {
            if let Some(current) = Mount::try_from_mumble_link(link.mount) {
                if !mounts.contains(current) {
                    return None;
                }
            } else {
                return None;
            }
        }
        let height_offset = attrs.get_height_offset().copied().unwrap_or(1.5); // default taco height offset
        let fade_near = attrs.get_fade_near().copied().unwrap_or(-1.0) / INCHES_PER_METER;
        let fade_far = attrs
            .get_fade_far()
            .copied()
            .unwrap_or(BILLBOARD_MAX_VISIBILITY_DISTANCE_IN_GAME)
            / INCHES_PER_METER;
        let icon_size = attrs.get_icon_size().copied().unwrap_or(1.0);
        let player_distance = pos.0.distance(link.player_pos.0);
        let camera_distance = pos.0.distance(link.cam_pos.0);
        let fade_near_far = Vec2(glam::Vec2::new(fade_near, fade_far));

        let alpha = attrs.get_alpha().copied().unwrap_or(1.0);
        let color = attrs.get_color().copied().unwrap_or_default();
        /*
           1. we need to filter the markers
               1. statically - mapid, character, map_type, race, profession
               2. dynamically - achievement, behavior, mount, fade_far, cull
               3. force hide/show by user discretion
           2. for active markers (not forcibly shown), we must do the dynamic checks every frame like behavior
           3. store the state for these markers activation data, and temporary data like bounce
        */
        /*
        skip if:
        alpha is 0.0
        achievement id/bit is done (maybe this should be at map filter level?)
        behavior (activation)
        cull
        distance > fade_far
        visibility (ingame/map/minimap)
        mount
        specialization
        */
        if fade_far > 0.0 && player_distance > fade_far {
            return None;
        }
        // markers are 1 meter in width/height by default
        let mut pos = pos.0;
        pos.y += height_offset;
        let direction_to_marker = link.cam_pos.0 - pos;
        let direction_to_side = direction_to_marker.normalize().cross(glam::Vec3::Y);

        let far_offset = {
            let dpi = if link.dpi_scaling <= 0 {
                96.0
            } else {
                link.dpi as f32
            } / 96.0;
            let gw2_width = link.client_size.0.as_vec2().x / dpi;

            // offset (half width i.e. distance from center of the marker to the side of the marker)
            const SIDE_OFFSET_FAR: f32 = 1.0;
            // the size of the projected on to the near plane
            let near_offset = SIDE_OFFSET_FAR * icon_size * (z_near / camera_distance);
            // convert the near_plane width offset into pixels by multiplying the near_ffset with gw2 window width
            let near_offset_in_pixels = near_offset * gw2_width;

            // we will clamp the texture width between min and max widths, and make sure that it is less than gw2 window width
            let near_offset_in_pixels = near_offset_in_pixels
                .clamp(*min_pixel_size, *max_pixel_size)
                .min(gw2_width / 2.0);

            let near_offset_of_marker = near_offset_in_pixels / gw2_width;
            near_offset_of_marker * camera_distance / z_near
        };
        // let pixel_ratio = width as f32 * (distance / z_near);// (near width / far width) = near_z / far_z;
        // we want to map 100 pixels to one meter in game
        // we are supposed to half the width/height too, as offset from the center will be half of the whole billboard
        // But, i will ignore that as that makes markers too small
        let x_offset = far_offset;
        let y_offset = x_offset; // seems all markers are squares
        let bottom_left = MarkerVertex {
            position: Vec3(pos - (direction_to_side * x_offset) - (glam::Vec3::Y * y_offset)),
            texture_coordinates: Vec2(glam::vec2(0.0, 1.0)),
            alpha,
            color,
            fade_near_far,
        };

        let top_left = MarkerVertex {
            position: Vec3(pos - (direction_to_side * x_offset) + (glam::Vec3::Y * y_offset)),
            texture_coordinates: Vec2(glam::vec2(0.0, 0.0)),
            alpha,
            color,
            fade_near_far,
        };
        let top_right = MarkerVertex {
            position: Vec3(pos + (direction_to_side * x_offset) + (glam::Vec3::Y * y_offset)),
            texture_coordinates: Vec2(glam::vec2(1.0, 0.0)),
            alpha,
            color,
            fade_near_far,
        };
        let bottom_right = MarkerVertex {
            position: Vec3(pos + (direction_to_side * x_offset) - (glam::Vec3::Y * y_offset)),
            texture_coordinates: Vec2(glam::vec2(1.0, 1.0)),
            alpha,
            color,
            fade_near_far,
        };
        let vertices = [
            top_left,
            bottom_left,
            bottom_right,
            bottom_right,
            top_right,
            top_left,
        ];
        Some(MarkerObject {
            vertices,
            texture: texture_id,
            distance: player_distance,
        })
    }
}

impl ActiveTrail {
    pub fn get_vertices_and_texture(
        attrs: &CommonAttributes,
        positions: &[Vec3],
        texture: TextureHandle,
    ) -> Option<Self> {
        // can't have a trail without atleast two nodes
        if positions.len() < 2 {
            return None;
        }
        let alpha = attrs.get_alpha().copied().unwrap_or(1.0);
        let fade_near = attrs.get_fade_near().copied().unwrap_or(-1.0) / INCHES_PER_METER;
        let fade_far = attrs
            .get_fade_far()
            .copied()
            .unwrap_or(BILLBOARD_MAX_VISIBILITY_DISTANCE_IN_GAME)
            / INCHES_PER_METER;
        let fade_near_far = Vec2(glam::Vec2::new(fade_near, fade_far));
        let color = attrs.get_color().copied().unwrap_or([0u8; 4]);
        // default taco width
        let horizontal_offset = 20.0 / INCHES_PER_METER;
        // scale it trail scale
        let horizontal_offset = horizontal_offset * attrs.get_trail_scale().copied().unwrap_or(1.0);
        let height = horizontal_offset * 2.0;

        let mut vertices = vec![];
        // trail mesh is split by separating different parts with a [0, 0, 0]
        // we will call each separate trail mesh as a "strip" of trail.
        // each strip should *almost* act as an independent trail, but they all are drawn at the same time with the same parameters.
        for strip in positions.split(|&v| v.0 == glam::Vec3::ZERO) {
            let mut y_offset = 1.0;
            for two_positions in strip.windows(2) {
                let first = two_positions[0].0;
                let second = two_positions[1].0;
                // right side of the vector from first to second
                let right_side = (second - first)
                    .normalize()
                    .cross(glam::Vec3::Y)
                    .normalize();

                let new_offset = (-1.0 * (first.distance(second) / height)) + y_offset;
                let first_left = MarkerVertex {
                    position: Vec3(first - (right_side * horizontal_offset)),
                    texture_coordinates: Vec2(glam::vec2(0.0, y_offset)),
                    alpha,
                    color,
                    fade_near_far,
                };
                let first_right = MarkerVertex {
                    position: Vec3(first + (right_side * horizontal_offset)),
                    texture_coordinates: Vec2(glam::vec2(1.0, y_offset)),
                    alpha,
                    color,
                    fade_near_far,
                };
                let second_left = MarkerVertex {
                    position: Vec3(second - (right_side * horizontal_offset)),
                    texture_coordinates: Vec2(glam::vec2(0.0, new_offset)),
                    alpha,
                    color,
                    fade_near_far,
                };
                let second_right = MarkerVertex {
                    position: Vec3(second + (right_side * horizontal_offset)),
                    texture_coordinates: Vec2(glam::vec2(1.0, new_offset)),
                    alpha,
                    color,
                    fade_near_far,
                };
                y_offset = if new_offset.is_sign_positive() {
                    new_offset
                } else {
                    1.0 - new_offset.fract().abs()
                };
                vertices.extend([
                    second_left,
                    first_left,
                    first_right,
                    first_right,
                    second_right,
                    second_left,
                ]);
            }
        }

        Some(ActiveTrail {
            trail_object: TrailObject {
                vertices: vertices.into(),
                texture: match texture.id() {
                    egui::TextureId::Managed(i) => i,
                    egui::TextureId::User(_) => todo!(),
                },
            },
            texture_handle: texture,
        })
    }
}

#[derive(Default, Clone)]
pub(crate) struct CurrentMapData {
    /// the map to which the current map data belongs to
    //pub map_id: u32,
    //pub active_elements: HashSet<Uuid>,
    /// The textures that are being used by the markers, so must be kept alive by this hashmap
    pub active_textures: OrderedHashMap<RelativePath, TextureHandle>,
    /// The key is the index of the marker in the map markers
    /// Their position in the map markers serves as their "id" as uuids can be duplicates.
    pub active_markers: IndexMap<Uuid, ActiveMarker>,
    pub wip_markers: IndexMap<Uuid, ActiveMarker>,
    /// The key is the position/index of this trail in the map trails. same as markers
    pub active_trails: IndexMap<Uuid, ActiveTrail>,
    pub wip_trails: IndexMap<Uuid, ActiveTrail>,
}
