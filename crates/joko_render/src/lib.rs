pub mod billboard;
use billboard::BillBoardRenderer;
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
use jokolink::MumbleLink;
use three_d::prelude::*;


use joko_marker_format::message::{MarkerObject, TrailObject};

#[macro_export]
macro_rules! gl_error {
    ($gl:expr) => {{
        let e = $gl.get_error();
        if e != egui_render_three_d::three_d::context::NO_ERROR {
            tracing::error!("glerror {} at {} {} {}", e, file!(), line!(), column!());
        }
    }};
}

pub struct JokoRenderer {
    pub view_proj: Mat4,
    pub cam_pos: glam::Vec3,
    pub camera: Camera,
    pub viewport: Viewport,
    pub has_link: bool,
    pub billboard_renderer: BillBoardRenderer,
    pub gl: egui_render_three_d::ThreeDBackend,
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
            gl: backend,
            billboard_renderer,
            cam_pos: Default::default(),
        }
    }
    pub fn get_z_near() -> f32 {
        1.0
    }
    pub fn get_z_far() -> f32 {
        1000.0
    }
    pub fn swap(&mut self) {
        self.billboard_renderer.swap();
    }
    pub fn tick(&mut self, link: Option<&MumbleLink>) {
        if let Some(link) = link {
            let center = link.cam_pos + link.f_camera_front;
            let camera = Camera::new_perspective(
                self.viewport,
                link.cam_pos.to_array().into(),
                center.to_array().into(),
                Vector3::unit_y(),
                Rad(link.fov),
                Self::get_z_near(),
                Self::get_z_far(),
            );
            self.camera = camera;
            let view = Mat4::look_at_lh(link.cam_pos, center, glam::Vec3::Y);
            let proj = Mat4::perspective_lh(
                link.fov,
                self.viewport.aspect(),
                Self::get_z_near(),
                Self::get_z_far(),
            );
            self.view_proj = proj * view;
            self.cam_pos = link.cam_pos;
            self.has_link = true;
        } else {
            self.has_link = false;
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
    ) {
        if self.has_link {
            self.billboard_renderer
                .prepare_render_data(&self.gl.context);
            self.billboard_renderer.render(
                &self.gl.context,
                self.cam_pos,
                &self.view_proj,
                &self.gl.glow_backend.painter.managed_textures,
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
