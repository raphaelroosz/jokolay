use std::{rc::Rc, sync::Arc};

use egui::{ClippedMesh, CtxRef};
use glm::{cross, make_vec3, normalize, Vec2};
use glow::{Context, HasContext};
use jokolink::mlink::MumbleLink;

use crate::{gl_error, tactical::localtypes::manager::MarkerManager};

use self::{egui_renderer::EguiGL, marker_renderer::MarkerGl, opengl::texture::TextureManager, trail_renderer::TrailGl};

use super::{fm::FileManager, window::glfw_window::OverlayWindowConfig};

pub mod egui_renderer;
pub mod marker_renderer;
pub mod opengl;
pub mod trail_renderer;
pub struct Renderer {
    pub egui_gl: EguiGL,
    pub marker_gl: MarkerGl,
    pub trail_gl: TrailGl,
    pub tm: TextureManager,
}

impl Renderer {
    pub fn new(gl: Rc<Context>, t: Arc<egui::Texture>) -> Self {
        
        unsafe {
            gl.enable(glow::MULTISAMPLE);
            gl.enable(glow::BLEND);
        }
        gl_error!(gl);
        let egui_gl = EguiGL::new(gl.clone());

        let tm = TextureManager::new(gl.clone());
        let marker_gl = MarkerGl::new(gl.clone());
        let trail_gl = TrailGl::new(gl);
        Self {
            egui_gl,
            marker_gl,
            trail_gl,
            tm,
        }
    }
    pub fn draw_egui(&mut self, meshes: Vec<ClippedMesh>, screen_size: Vec2, fm: &FileManager) {
        unsafe {
            self.egui_gl.gl.disable(glow::SCISSOR_TEST);
            self.egui_gl.gl.clear_color(0.0, 0.0, 0.0, 0.0);
            self.egui_gl.gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
        }
        self.egui_gl
            .draw_meshes(meshes, screen_size, &mut self.tm, fm)
            .unwrap();
            let gl = self.egui_gl.gl.clone();
            gl_error!(gl);
            
    }
    pub fn draw_markers(&mut self, mm: &mut MarkerManager, link: &MumbleLink, fm: &FileManager, wc: OverlayWindowConfig) {
        self.marker_gl.draw_markers(&mut self.tm, mm, link, fm, wc);
    }
    pub fn draw_trails(&mut self, mm: &mut MarkerManager, link: &MumbleLink, fm: &FileManager, ctx: CtxRef) {
        self.trail_gl.draw_trails(
            mm,
            link,
            fm,
            &mut self.tm,
            ctx
        )
    }
}