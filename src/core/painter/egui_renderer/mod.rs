use std::rc::Rc;

use egui::{ClippedMesh, CtxRef, Rect};
use glm::Vec2;
use glow::{Context, HasContext, NativeUniformLocation, UNSIGNED_INT};


use crate::core::fm::FileManager;

use super::opengl::{buffer::Buffer, shader::ShaderProgram, texture::TextureManager, vertex_array::VertexArrayObject};

pub struct EguiGL {
    pub vao: VertexArrayObject,
    pub sp: ShaderProgram,
    pub vb: Buffer,
    pub ib: Buffer,
    pub u_sampler: NativeUniformLocation,
    pub u_sampler_layer: NativeUniformLocation,
    pub u_screen_size: NativeUniformLocation,
    pub gl: Rc<glow::Context>,
}

impl EguiGL {
    pub fn new(gl: Rc<Context>) -> EguiGL {
        let layout = VertexRgba::get_layout();
        let vao = VertexArrayObject::new(gl.clone(), layout);
        let vb = Buffer::new(gl.clone(), glow::ARRAY_BUFFER);
        let ib = Buffer::new(gl.clone(), glow::ELEMENT_ARRAY_BUFFER);
        let program = ShaderProgram::new(
            gl.clone(),
            EGUI_VERTEX_SHADER_SRC,
            EGUI_FRAGMENT_SHADER_SRC,
            None,
        );

        let u_sampler;
        let u_sampler_layer;
        let u_screen_size;

        unsafe {
            u_sampler = gl.get_uniform_location(program.id, "sampler").unwrap();
            u_sampler_layer = gl
                .get_uniform_location(program.id, "sampler_layer")
                .unwrap();
            u_screen_size = gl.get_uniform_location(program.id, "screen_size").unwrap();
        }

        let egui_gl = EguiGL {
            vao,
            vb,
            ib,
            sp: program,
            u_sampler,
            u_sampler_layer,
            u_screen_size,
            gl: gl.clone(),
        };
        egui_gl.bind();

        return egui_gl;
    }

    pub fn draw_meshes(
        &mut self,
        meshes: Vec<ClippedMesh>,
        screen_size: Vec2,
        tm: &mut TextureManager,
        fm: &FileManager,
        ctx: CtxRef
    ) -> anyhow::Result<()> {
        self.bind();

        unsafe {
            self.gl.enable(glow::FRAMEBUFFER_SRGB);
            self.gl.disable(glow::DEPTH_TEST);
            self.gl.blend_func(glow::ONE, glow::ONE_MINUS_SRC_ALPHA);
            self.gl.enable(glow::SCISSOR_TEST);
        }
        unsafe {
            self.gl
                .uniform_2_f32_slice(Some(&self.u_screen_size), screen_size.as_slice());
        }
        for clipped_mesh in meshes {
            self.draw_mesh(clipped_mesh, screen_size, tm, fm, ctx.clone())?;
        }
        unsafe {
            self.gl.disable(glow::SCISSOR_TEST);
            self.gl.disable(glow::FRAMEBUFFER_SRGB);
        }

        Ok(())
    }
    pub fn draw_mesh(
        &mut self,
        clipped_mesh: ClippedMesh,
        screen_size: Vec2,
        tm: &mut TextureManager,
        fm: &FileManager,
        ctx: CtxRef
    ) -> anyhow::Result<()> {
        Self::set_scissor(clipped_mesh.0, self.gl.clone(), screen_size);
        let mesh = &clipped_mesh.1;
        let vertices: Vec<VertexRgba> = mesh.vertices.iter().map(|v| VertexRgba::from(v)).collect();
        let indices = &mesh.indices;

        self.update_buffers(
            Some((bytemuck::cast_slice(&vertices), glow::DYNAMIC_DRAW)),
            Some((bytemuck::cast_slice(indices), glow::DYNAMIC_DRAW)),
        );
        let (slot, _, _, z) = tm.get_etex(mesh.texture_id, fm, ctx);
        unsafe {
            //sampler uniforms are i32
            self.gl.uniform_1_i32(Some(&self.u_sampler), slot as i32);
            self.gl.uniform_1_i32(Some(&self.u_sampler_layer), z as i32);
        }

        self.render(indices.len() as u32, 0);
        Ok(())
    }
    fn set_scissor(clip_rect: Rect, gl: Rc<glow::Context>, screen_size: Vec2) {
        //clip rectangle copy pasted from glium
        let clip_min_x = clip_rect.min.x;
        let clip_min_y = clip_rect.min.y;
        let clip_max_x = clip_rect.max.x;
        let clip_max_y = clip_rect.max.y;

        // Make sure clip rect can fit within a `u32`:
        let clip_min_x = clip_min_x.clamp(0.0, screen_size.x);
        let clip_min_y = clip_min_y.clamp(0.0, screen_size.y);
        let clip_max_x = clip_max_x.clamp(clip_min_x, screen_size.x);
        let clip_max_y = clip_max_y.clamp(clip_min_y, screen_size.y);

        let clip_min_x = clip_min_x.round() as u32;
        let clip_min_y = clip_min_y.round() as u32;
        let clip_max_x = clip_max_x.round() as u32;
        let clip_max_y = clip_max_y.round() as u32;

        unsafe {
            gl.scissor(
                clip_min_x as i32,
                (screen_size.y - clip_max_y as f32) as i32,
                (clip_max_x - clip_min_x) as i32,
                (clip_max_y - clip_min_y) as i32,
            );
        }
    }
}
impl EguiGL {
    fn bind(&self) {
        self.vao.bind();
        self.vb.bind();
        self.ib.bind();
        self.sp.bind();
    }

    fn update_buffers(&self, vb: Option<(&[u8], u32)>, ib: Option<(&[u8], u32)>) {
        if let Some((data, usage)) = vb {
            self.vb.update(data, usage);
        }
        if let Some((data, usage)) = ib {
            self.ib.update(data, usage);
        }
    }

    fn render(&self, count: u32, offset: u32) {
        unsafe {
            self.gl
                .draw_elements(glow::TRIANGLES, count as i32, UNSIGNED_INT, offset as i32);
        }
    }
    fn _unbind(&self) {
        self.vao.unbind();
        self.vb.unbind();
        self.ib.unbind();
        self.sp.unbind();
    }
}

use egui::{epaint::Vertex, Pos2};

use crate::core::painter::opengl::buffer::{VertexBufferLayout, VertexBufferLayoutTrait};

#[derive(Debug, Clone, Copy)]
pub struct VertexRgba {
    pub pos: Pos2,
    pub uv: Pos2,
    pub color: [u8; 4],
}

impl VertexBufferLayoutTrait for VertexRgba {
    fn get_layout() -> VertexBufferLayout {
        let mut vbl = VertexBufferLayout::default();
        vbl.push_f32(2, false);
        vbl.push_f32(2, false);
        vbl.push_u8(4, false);
        vbl
    }
}
impl From<&Vertex> for VertexRgba {
    fn from(vert: &Vertex) -> Self {
        VertexRgba {
            pos: vert.pos,
            uv: vert.uv,
            color: vert.color.to_array(),
        }
    }
}
impl From<Vertex> for VertexRgba {
    fn from(vert: Vertex) -> Self {
        VertexRgba {
            pos: vert.pos,
            uv: vert.uv,
            color: vert.color.to_array(),
        }
    }
}

unsafe impl bytemuck::Zeroable for VertexRgba {
    fn zeroed() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
unsafe impl bytemuck::Pod for VertexRgba {}

const EGUI_FRAGMENT_SHADER_SRC: &str = include_str!("shader.fs");

const EGUI_VERTEX_SHADER_SRC: &str = include_str!("shader.vs");