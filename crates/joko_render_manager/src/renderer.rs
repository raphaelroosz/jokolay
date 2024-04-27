use crate::billboard::BillBoardRenderer;
use crate::gl_error;
use egui_render_three_d::three_d;
use egui_render_three_d::three_d::context::COLOR_BUFFER_BIT;
use egui_render_three_d::three_d::context::DEPTH_BUFFER_BIT;
use egui_render_three_d::three_d::context::STENCIL_BUFFER_BIT;
use egui_render_three_d::three_d::Camera;
use egui_render_three_d::three_d::HasContext;
use egui_render_three_d::three_d::ScissorBox;
use egui_render_three_d::three_d::Viewport;
use egui_render_three_d::ThreeDBackend;
use egui_render_three_d::ThreeDConfig;
use egui_window_glfw_passthrough::GlfwBackend;
use glam::Mat4;
use joko_component_models::ComponentDataExchange;
use joko_component_models::JokolayComponent;
use joko_component_models::JokolayComponentDeps;
use joko_component_models::PeerComponentChannel;
use joko_link_models::MumbleLink;
use joko_link_models::UIState;
use joko_render_models::messages::UIToUIMessage;
use three_d::prelude::*;

use joko_render_models::{marker::MarkerObject, trail::TrailObject};

pub struct JokoRenderer {
    pub view_proj: Mat4,
    pub cam_pos: glam::Vec3,
    pub camera: Camera,
    pub viewport: Viewport,
    pub has_link: bool,
    pub is_map_open: bool,
    pub billboard_renderer: BillBoardRenderer,
    pub gl: egui_render_three_d::ThreeDBackend,
    channel_receiver: Option<tokio::sync::mpsc::Receiver<ComponentDataExchange>>,
}

impl JokoRenderer {
    pub fn new(glfw_backend: &mut GlfwBackend, _debug: bool) -> Self {
        let glfw = glfw_backend.glfw.clone();
        let backend = ThreeDBackend::new(
            ThreeDConfig {
                glow_config: Default::default(),
            },
            |s| glfw.get_proc_address_raw(s),
            //glfw_backend.window.raw_window_handle(),
            glfw_backend.framebuffer_size_physical,
        );
        let viewport = Viewport {
            x: 0,
            y: 0,
            width: glfw_backend.framebuffer_size_physical[0],
            height: glfw_backend.framebuffer_size_physical[1],
        };
        let gl = &backend.context;
        unsafe { gl_error!(gl) };
        let billboard_renderer = BillBoardRenderer::new(gl);
        unsafe { gl_error!(gl) };
        Self {
            viewport,
            view_proj: Default::default(),
            camera: Camera::new_perspective(
                viewport,
                [0.0, 0.0, 0.0].into(),
                [0.0, 0.0, 0.0].into(),
                Vector3::unit_y(),
                Deg(90.0),
                1.0,
                5000.0,
            ),
            has_link: false,
            is_map_open: false,
            gl: backend,
            billboard_renderer,
            cam_pos: Default::default(),
            channel_receiver: None,
        }
    }

    /*
        CRect GetMinimapRectangle()
    {
      int w = mumbleLink.miniMap.compassWidth;
      int h = mumbleLink.miniMap.compassHeight;

      CRect pos;
      CRect size = App->GetRoot()->GetClientRect();
      float scale = GetWindowTooSmallScale();

      pos.x1 = int( size.Width() - w * scale );
      pos.x2 = size.Width();


      if ( mumbleLink.isMinimapTopRight )
      {
        pos.y1 = 1;
        pos.y2 = int( h * scale + 1 );
      }
      else
      {
        int delta = 37;
        if ( mumbleLink.uiSize == 0 )
          delta = 33;
        if ( mumbleLink.uiSize == 2 )
          delta = 41;
        if ( mumbleLink.uiSize == 3 )
          delta = 45;

        pos.y1 = int( size.Height() - h * scale - delta * scale );
        pos.y2 = int( size.Height() - delta * scale );
      }

      return pos;
    }
     */
    pub fn get_z_near() -> f32 {
        1.0
    }
    pub fn get_z_far() -> f32 {
        1000.0
    }
    pub fn swap(&mut self) {
        self.billboard_renderer.swap();
    }
    /*
    //https://wiki.guildwars2.com/wiki/API:1/event_details#Coordinate_recalculation
    fn _scale_coords(continent_rect, map_rect, coords){
        continent_width = continent_rect[1].x - continent_rect[0].x;
        continent_height  = continent_rect[1].y - continent_rect[0].y;
        map_width = map_rect[1].x - map_rect[0].x;
        map_height = map_rect[1].y - map_rect[0].y;
        position_on_map_x = coords.x - map_rect[0].x;
        position_on_map_y = coords.y - map_rect[1].y;
        return [
          Math.round( continent_rect[0].x + ( 1 * position_on_map_x / map_width * continent_width ) ),
          Math.round( continent_rect[0].y + (-1 * position_on_map_y / map_height * continent_height ) )
        ];
      }
      */
    fn handle_u2u_message(&mut self, msg: UIToUIMessage) {
        match msg {
            UIToUIMessage::BulkMarkerObject(marker_objects) => {
                tracing::debug!(
                    "Handling of UIToUIMessage::BulkMarkerObject {}",
                    marker_objects.len()
                );
                self.extend_markers(marker_objects);
            }
            UIToUIMessage::BulkTrailObject(trail_objects) => {
                tracing::debug!(
                    "Handling of UIToUIMessage::BulkTrailObject {}",
                    trail_objects.len()
                );
                self.extend_trails(trail_objects);
            }
            UIToUIMessage::MarkerObject(mo) => {
                tracing::trace!("Handling of UIToUIMessage::MarkerObject");
                self.add_billboard(*mo);
            }
            UIToUIMessage::TrailObject(to) => {
                tracing::trace!("Handling of UIToUIMessage::TrailObject");
                self.add_trail(*to);
            }
            UIToUIMessage::RenderSwapChain => {
                tracing::debug!("Handling of UIToUIMessage::RenderSwapChain");
                self.swap();
            }
            #[allow(unreachable_patterns)]
            _ => {
                unimplemented!("Handling UIToUIMessage has not been implemented yet");
            }
        }
    }

    pub fn extend_markers(&mut self, marker_objects: Vec<MarkerObject>) {
        self.billboard_renderer.markers_wip.extend(marker_objects);
    }
    pub fn add_billboard(&mut self, marker_object: MarkerObject) {
        self.billboard_renderer.markers_wip.push(marker_object);
    }

    pub fn extend_trails(&mut self, trail_objects: Vec<TrailObject>) {
        self.billboard_renderer.trails_wip.extend(trail_objects);
    }
    pub fn add_trail(&mut self, trail_object: TrailObject) {
        self.billboard_renderer.trails_wip.push(trail_object);
    }

    pub fn prepare_frame(&mut self, latest_framebuffer_size_getter: impl FnMut() -> [u32; 2]) {
        self.gl.prepare_frame(latest_framebuffer_size_getter);
        unsafe {
            let gl = self.gl.context.clone();
            gl_error!(gl);
            // self.gl.context.set_viewport(self.viewport);
            self.gl.context.set_scissor(ScissorBox::new_at_origo(
                self.viewport.width,
                self.viewport.height,
            ));
            self.gl.context.clear_color(0.0, 0.0, 0.0, 0.0);
            self.gl
                .context
                .clear(COLOR_BUFFER_BIT | DEPTH_BUFFER_BIT | STENCIL_BUFFER_BIT);
            gl_error!(gl);
        }
    }

    pub fn render_egui(
        &mut self,
        meshes: Vec<egui::ClippedPrimitive>,
        textures_delta: egui::TexturesDelta,
        logical_screen_size: [f32; 2],
        latest_time: f64,
    ) {
        if self.has_link && !self.is_map_open {
            self.billboard_renderer
                .prepare_render_data(&self.gl.context);
            self.billboard_renderer.render(
                &self.gl.context,
                self.cam_pos,
                &self.view_proj,
                &self.gl.glow_backend.painter.managed_textures,
                latest_time,
            );
        }
        self.gl
            .render_egui(meshes, textures_delta, logical_screen_size);
    }

    pub fn present(&mut self) {}

    pub fn resize_framebuffer(&mut self, latest_size: [u32; 2]) {
        tracing::info!(?latest_size, "resizing framebuffer");

        self.viewport = Viewport {
            x: 0,
            y: 0,
            width: latest_size[0],
            height: latest_size[1],
        };
        self.gl.resize_framebuffer(latest_size);
    }
}

impl JokolayComponentDeps for JokoRenderer {}
impl JokolayComponent<(), ()> for JokoRenderer {
    fn bind(
        &mut self,
        _deps: std::collections::HashMap<
            u32,
            tokio::sync::broadcast::Receiver<ComponentDataExchange>,
        >,
        _bound: std::collections::HashMap<u32, PeerComponentChannel>, // ??? scsc if exists, this is a private channel only two bounded modules can use between each others.
        mut input_notification: std::collections::HashMap<
            u32,
            tokio::sync::mpsc::Receiver<ComponentDataExchange>,
        >,
        _notify: std::collections::HashMap<u32, tokio::sync::mpsc::Sender<ComponentDataExchange>>, // used to send a message to another plugin. This is a reversed requirement. A plugin force itself into the path of another.
    ) {
        self.channel_receiver = input_notification.remove(&0);
    }
    fn flush_all_messages(&mut self) {
        let channel_receiver = self.channel_receiver.as_mut().unwrap();

        //two steps reading due to self mutability required by channel
        let mut messages = Vec::new();
        while let Ok(msg) = channel_receiver.try_recv() {
            messages.push(msg.into());
        }
        for msg in messages {
            self.handle_u2u_message(msg);
        }
    }
    fn tick(&mut self, _latest_time: f64) -> Option<&()> {
        let link: Option<&MumbleLink> = None;
        if let Some(link) = link {
            //x positive => east
            //y positive => ascention
            //z positive => north
            self.is_map_open = if let Some(ui_state) = link.ui_state {
                ui_state.contains(UIState::IsMapOpen)
            } else {
                false
            };

            //TODO: change perspective is map is open
            let center = link.cam_pos.0 + link.f_camera_front.0;
            let cam_pos = link.cam_pos;
            /*
            let map_pos_x = (link.player_x - link.map_center_x) / 1.64;
            let map_pos_y = (link.map_center_y - link.player_y) / 1.64;
            let center = if self.is_map_open {
                glam::Vec3{
                    x: link.player_pos.x - map_pos_x,
                    y: link.player_pos.y + 100.0,
                    z: link.player_pos.z - map_pos_y,
                }
            } else {
                link.cam_pos + link.f_camera_front //default old one
            };

            let client_width = (link.client_size.x) as f32;
            let client_height = (link.client_size.y) as f32;

            let cam_pos = if self.is_map_open {
                //TODO: validate values
                glam::Vec3{
                    x: link.player_pos.x - map_pos_x,
                    y: link.player_pos.y + 101.0,
                    z: link.player_pos.z - map_pos_y,
                }
            }else {
                link.cam_pos //default old one
            };*/
            let camera = Camera::new_perspective(
                self.viewport,
                cam_pos.0.to_array().into(),
                center.to_array().into(),
                Vector3::unit_y(),
                Rad(link.fov),
                Self::get_z_near(),
                Self::get_z_far(),
            );
            self.camera = camera;
            /*
            is_map_open:
                target camera direction: 0 -20 1
                have trails seen from further
                have trails fatter drawing

            println!("client: {} {} {} {}", client_width, client_height, client_width.div(client_height), client_height.div(client_width));
            println!("map scale: {}", link.map_scale);
            println!("map position: {} {}", map_pos_x, map_pos_y);
            println!("cam:       {} {} {}", cam_pos.x, cam_pos.y, cam_pos.z);
            println!("center:    {} {} {}", center.x, center.y, center.z);
            println!("H:    {}", cam_pos.y - center.y);
            println!("player:    {} {} {}", link.player_pos.x, link.player_pos.y, link.player_pos.z);
            */

            let view = Mat4::look_at_lh(cam_pos.0, center, glam::Vec3::Y);
            let proj = Mat4::perspective_lh(
                link.fov,
                self.viewport.aspect(),
                Self::get_z_near(),
                Self::get_z_far(),
            );
            self.view_proj = proj * view;
            self.cam_pos = cam_pos.0;
            self.has_link = true;
        } else {
            self.has_link = false;
        }
        None
    }
}
