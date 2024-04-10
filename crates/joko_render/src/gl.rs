#[macro_export]
macro_rules! gl_error {
    ($gl:expr) => {{
        let e = $gl.get_error();
        if e != egui_render_three_d::three_d::context::NO_ERROR {
            tracing::error!("glerror {} at {} {} {}", e, file!(), line!(), column!());
        }
    }};
}