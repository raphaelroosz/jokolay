use egui::ahash::HashMap;
use egui_render_three_d::{
    three_d::{context::*, Context, HasContext},
    GpuTexture,
};
use glam::Vec2;
use joko_render_models::{
    marker::{MarkerObject, MarkerVertex},
    trail::TrailObject,
};
use tracing::{error, info, trace, warn};

use crate::gl_error;

const MARKER_VERTEX_STRIDE: i32 = std::mem::size_of::<MarkerVertex>() as _;
pub struct BillBoardRenderer {
    pub markers: Vec<MarkerObject>,
    pub trails: Vec<TrailObject>,
    pub markers_wip: Vec<MarkerObject>, //work in progress: this is where the markers are inserted
    pub trails_wip: Vec<TrailObject>,   //work in progress: this is where the markers are inserted
    marker_program: NativeProgram,
    marker_vertex_buffer: NativeBuffer,
    marker_vertex_array: NativeVertexArray,

    trail_program: NativeProgram,
    trail_vertex_buffers: Vec<NativeBuffer>,
    trail_vertex_arrays: Vec<NativeVertexArray>,
}

const MARKER_VERTEX_SHADER: &str = include_str!("../shaders/marker.vs");
const MARKER_FRAGMENT_SHADER: &str = include_str!("../shaders/marker.fs");
const TRAIL_VERTEX_SHADER: &str = include_str!("../shaders/trail.vs");
const TRAIL_FRAGMENT_SHADER: &str = include_str!("../shaders/trail.fs");

impl BillBoardRenderer {
    pub fn new(gl: &Context) -> Self {
        unsafe {
            let marker_program =
                new_program(gl, MARKER_VERTEX_SHADER, MARKER_FRAGMENT_SHADER, None);
            gl_error!(gl);

            let trail_shift_program =
                new_program(gl, TRAIL_VERTEX_SHADER, TRAIL_FRAGMENT_SHADER, None);
            gl_error!(gl);

            let marker_vertex_buffer = create_buffer(gl);
            let marker_vertex_array = create_marker_array(gl, marker_vertex_buffer);
            gl_error!(gl);

            Self {
                markers: Vec::new(),
                markers_wip: Vec::new(),

                marker_program,
                marker_vertex_buffer,
                marker_vertex_array,

                trails: Vec::new(),
                trails_wip: Vec::new(),

                trail_program: trail_shift_program,
                trail_vertex_buffers: Default::default(),
                trail_vertex_arrays: Default::default(),
            }
        }
    }

    pub fn begin(&mut self) {
        trace!("Begin with a fresh list of markers and trails");
        self.markers_wip.clear();
        self.trails_wip.clear();
    }
    pub fn flush(&mut self) {
        trace!(
            "Flush UI to display {} markers, {} trails",
            self.markers_wip.len(),
            self.trails_wip.len()
        );
        self.markers.clone_from(&self.markers_wip);
        self.trails.clone_from(&self.trails_wip);
    }
    pub fn swap(&mut self) {
        trace!(
            "swap UI to display {} markers, {} trails",
            self.markers_wip.len(),
            self.trails_wip.len()
        );
        self.markers = std::mem::take(&mut self.markers_wip);
        self.trails = std::mem::take(&mut self.trails_wip);
    }

    pub fn prepare_render_data(&mut self, gl: &Context) {
        /*
        TODO: map view (view from above)
            trim down the trails too far ?
            fatten them ?
        */
        unsafe {
            gl_error!(gl);
        }
        // sort by depth
        self.markers.sort_unstable_by(|first, second| {
            first.distance.total_cmp(&second.distance).reverse() // we need the farther markers (more distance from camera) to be rendered first, for correct alpha blending
        });

        let mut required_size_in_bytes =
            (self.markers.len() * 6 * std::mem::size_of::<MarkerVertex>()) as u64;
        for trail in self.trails.iter() {
            let len = (trail.vertices.len() * std::mem::size_of::<MarkerVertex>()) as u64;
            required_size_in_bytes = required_size_in_bytes.max(len);
        }
        let mut vb: Vec<MarkerVertex> = Vec::with_capacity(self.markers.len() * 6);

        for marker_object in self.markers.iter() {
            vb.extend_from_slice(&marker_object.vertices);
        }
        unsafe {
            gl_error!(gl);
            gl.bind_buffer(ARRAY_BUFFER, Some(self.marker_vertex_buffer));
            gl.buffer_data_u8_slice(ARRAY_BUFFER, bytemuck::cast_slice(&vb), DYNAMIC_DRAW);
            gl_error!(gl);
        }
        if self.trails.len() > self.trail_vertex_buffers.len() {
            let needs = self.trails.len() - self.trail_vertex_buffers.len();
            for _ in 0..needs {
                let vb = unsafe { create_buffer(gl) };
                self.trail_vertex_buffers.push(vb);
                let trail_vertex_array = unsafe { create_trail_array(gl, vb, 1) };
                self.trail_vertex_arrays.push(trail_vertex_array);
            }
        }
        for (trail, trail_buffer) in self.trails.iter().zip(self.trail_vertex_buffers.iter()) {
            unsafe {
                gl.bind_buffer(ARRAY_BUFFER, Some(*trail_buffer));
                gl.buffer_data_u8_slice(
                    ARRAY_BUFFER,
                    bytemuck::cast_slice(trail.vertices.as_ref()),
                    DYNAMIC_DRAW,
                );
            }
        }
        unsafe {
            gl_error!(gl);
        }
    }
    pub fn render(
        &self,
        gl: &Context,
        cam_pos: glam::Vec3,
        view_proj: &glam::Mat4,
        textures: &HashMap<u64, GpuTexture>,
        latest_time: f64,
    ) {
        unsafe {
            gl_error!(gl);
            gl.disable(SCISSOR_TEST);

            gl.use_program(Some(self.trail_program));
            gl_error!(gl);
            gl.active_texture(TEXTURE0);
            gl_error!(gl);
            let scroll_texture: Vec2 = Vec2 {
                x: 0.0,
                y: (latest_time as f32 % 2.0) - 1.0,
            }; //TODO: manage speed in some configurations. per trail ?

            gl.uniform_2_f32_slice(Some(&NativeUniformLocation(3)), scroll_texture.as_ref());
            //https://stackoverflow.com/questions/27771902/opengl-changing-texture-coordinates-on-the-fly
            //https://www.khronos.org/opengl/wiki/Uniform_(GLSL)
            for ((trail, trail_buffer), trail_array) in self
                .trails
                .iter()
                .zip(self.trail_vertex_buffers.iter())
                .zip(self.trail_vertex_arrays.iter())
            {
                if let Some(texture) = textures.get(&trail.texture) {
                    gl.bind_vertex_array(Some(*trail_array));
                    gl.uniform_3_f32_slice(Some(&NativeUniformLocation(0)), cam_pos.as_ref());
                    gl.uniform_matrix_4_f32_slice(
                        Some(&NativeUniformLocation(2)),
                        false,
                        view_proj.to_cols_array().as_ref(),
                    );
                    gl_error!(gl);

                    gl.bind_vertex_buffer(0, Some(*trail_buffer), 0, MARKER_VERTEX_STRIDE);
                    gl.bind_buffer(ARRAY_BUFFER, Some(*trail_buffer));
                    gl.bind_texture(TEXTURE_2D, Some(texture.handle));
                    gl.bind_sampler(0, Some(texture.sampler));
                    gl_error!(gl);
                    gl.draw_arrays(TRIANGLES, 0, trail.vertices.len() as _);
                    gl_error!(gl);

                    /*
                    gl.polygon_mode(FRONT_AND_BACK, LINE);
                    gl.draw_arrays(TRIANGLES, 0, trail.vertices.len() as _);
                    gl.polygon_mode(FRONT_AND_BACK, FILL);
                    gl_error!(gl);
                    */
                }
            }
            gl.use_program(Some(self.marker_program));
            gl_error!(gl);
            gl.bind_vertex_array(Some(self.marker_vertex_array));
            gl_error!(gl);
            gl.uniform_3_f32_slice(Some(&NativeUniformLocation(0)), cam_pos.as_ref());
            gl.uniform_matrix_4_f32_slice(
                Some(&NativeUniformLocation(2)),
                false,
                view_proj.to_cols_array().as_ref(),
            );
            gl_error!(gl);
            gl.bind_vertex_buffer(0, Some(self.marker_vertex_buffer), 0, MARKER_VERTEX_STRIDE);
            gl.bind_buffer(ARRAY_BUFFER, Some(self.marker_vertex_buffer));
            for (index, mo) in self.markers.iter().enumerate() {
                let index: u32 = index.try_into().unwrap();
                if let Some(texture) = textures.get(&mo.texture) {
                    gl.bind_texture(TEXTURE_2D, Some(texture.handle));
                    gl.bind_sampler(0, Some(texture.sampler));
                    gl.draw_arrays(TRIANGLES, index as i32 * 6, 6);
                }
            }
            gl_error!(gl);
            gl.bind_vertex_array(None);
        }
    }
}

/// takes in strings containing vertex/fragment shaders and returns a Shaderprogram with them attached
#[tracing::instrument(skip(gl))]
pub fn new_program(
    gl: &Context,
    vertex_shader_source: &str,
    fragment_shader_source: &str,
    _geometry_shader_source: Option<&str>,
) -> NativeProgram {
    //https://www.khronos.org/opengl/wiki/Shader_Compilation#Program_setup
    unsafe {
        gl_error!(gl);

        let program = gl.create_program().unwrap();
        let vertex_shader = gl.create_shader(VERTEX_SHADER).unwrap();
        gl.shader_source(vertex_shader, vertex_shader_source);
        gl.compile_shader(vertex_shader);
        if !gl.get_shader_compile_status(vertex_shader) {
            let e = gl.get_shader_info_log(vertex_shader);
            error!("{}", &e);
            panic!("vertex shader compilation error: {}", &e);
        }
        let frag_shader = gl.create_shader(FRAGMENT_SHADER).unwrap();
        gl.shader_source(frag_shader, fragment_shader_source);
        gl.compile_shader(frag_shader);
        if !gl.get_shader_compile_status(frag_shader) {
            let e = gl.get_shader_info_log(frag_shader);
            error!("frag shader compilation error:{}", &e);
            panic!("frag shader compilation error: {}", &e);
        }
        gl.attach_shader(program, vertex_shader);
        gl.attach_shader(program, frag_shader);
        // let geometry_shader;
        // geometry_shader = gl.create_shader(glow::GEOMETRY_SHADER).unwrap();
        // if let Some(gss) = geometry_shader_source {
        //     gl.shader_source(geometry_shader, gss);
        //     gl.compile_shader(geometry_shader);
        //     if !gl.get_shader_compile_status(geometry_shader) {
        //         let e = gl.get_shader_info_log(geometry_shader);
        //         error!("frag shader compilation error:{}", &e);
        //         panic!("geometry shader compilation error: {}", &e);
        //     }
        //     gl.attach_shader(shader_program, geometry_shader);
        // }
        gl.link_program(program);
        if !gl.get_program_link_status(program) {
            let e = gl.get_program_info_log(program);
            error!("shader program link error: {}", &e);
            panic!("shader program link error: {}", &e);
        }
        gl.delete_shader(vertex_shader);
        // if geometry_shader_source.is_some() {
        //     gl.delete_shader(geometry_shader);
        // }
        gl.delete_shader(frag_shader);
        gl_error!(gl);
        let active_attribute_count = gl.get_active_attributes(program);
        let mut shader_info = format!("Shader Info:\nvertex attributes: {active_attribute_count}");
        for index in 0..active_attribute_count {
            if let Some(attr) = gl.get_active_attribute(program, index) {
                let location = gl.get_attrib_location(program, &attr.name);
                shader_info = format!("{shader_info}\n{} @ {}", attr.name, location.unwrap());
            } else {
                warn!("attribute with index {index} doesn't exist");
            }
        }
        let active_uniform_count = gl.get_active_uniforms(program);
        shader_info = format!("{shader_info}\nuniform locations:{active_uniform_count}");
        for index in 0..active_uniform_count {
            if let Some(attr) = gl.get_active_uniform(program, index) {
                let location = gl.get_uniform_location(program, &attr.name);
                shader_info = format!("{shader_info}\n{} @ {}", attr.name, location.unwrap().0);
            } else {
                warn!("uniform with index {index} doesn't exist");
            }
        }
        info!("{shader_info}");
        program
    }
}
unsafe fn create_buffer(gl: &Context) -> NativeBuffer {
    gl_error!(gl);
    let vb = gl.create_buffer().expect("failed to create vb for markers");
    gl_error!(gl);

    gl.bind_vertex_array(None);
    gl.bind_buffer(ARRAY_BUFFER, Some(vb));
    gl_error!(gl);

    gl.bind_buffer(ARRAY_BUFFER, None);
    gl_error!(gl);
    vb
}

unsafe fn create_marker_array(gl: &Context, vertex_buffer: NativeBuffer) -> NativeVertexArray {
    create_array(gl, vertex_buffer, 1)
}

unsafe fn create_array(
    gl: &Context,
    vertex_buffer: NativeBuffer,
    binding_index: u32,
) -> NativeVertexArray {
    let marker_vertex_array = gl.create_vertex_array().expect("failed to create egui vao");
    gl.bind_vertex_array(Some(marker_vertex_array));
    gl.bind_vertex_buffer(binding_index, Some(vertex_buffer), 0, MARKER_VERTEX_STRIDE);
    gl_error!(gl);

    gl.enable_vertex_array_attrib(marker_vertex_array, 0);
    gl.vertex_array_attrib_format_f32(marker_vertex_array, 0, 3, FLOAT, false, 0);
    gl.vertex_array_attrib_binding_f32(marker_vertex_array, 0, 0);
    gl_error!(gl);

    gl.enable_vertex_array_attrib(marker_vertex_array, 1);
    gl.vertex_array_attrib_format_f32(marker_vertex_array, 1, 1, FLOAT, false, 12);
    gl.vertex_array_attrib_binding_f32(marker_vertex_array, 1, 0);
    gl_error!(gl);

    gl.enable_vertex_array_attrib(marker_vertex_array, 2);
    gl.vertex_array_attrib_format_f32(marker_vertex_array, 2, 2, FLOAT, false, 16);
    gl.vertex_array_attrib_binding_f32(marker_vertex_array, 2, 0);
    gl_error!(gl);

    gl.enable_vertex_array_attrib(marker_vertex_array, 3);
    gl.vertex_array_attrib_format_f32(marker_vertex_array, 3, 2, FLOAT, false, 24);
    gl.vertex_array_attrib_binding_f32(marker_vertex_array, 3, 0);
    gl_error!(gl);

    gl.enable_vertex_array_attrib(marker_vertex_array, 4);
    gl.vertex_array_attrib_format_f32(marker_vertex_array, 4, 4, UNSIGNED_BYTE, true, 32);
    gl.vertex_array_attrib_binding_f32(marker_vertex_array, 4, 0);
    gl_error!(gl);
    marker_vertex_array
}

unsafe fn create_trail_array(
    gl: &Context,
    vertex_buffer: NativeBuffer,
    binding_index: u32,
) -> NativeVertexArray {
    let trail_vertex_array = create_array(gl, vertex_buffer, binding_index);
    gl.enable_vertex_array_attrib(trail_vertex_array, 5);
    gl.vertex_array_attrib_format_f32(trail_vertex_array, 5, 2, FLOAT, false, 36);
    gl.vertex_array_attrib_binding_f32(trail_vertex_array, 5, 0);
    gl_error!(gl);

    trail_vertex_array
}
