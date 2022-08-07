use bevy::prelude::*;
use bevy_egui::EguiContext;


pub mod mumble;
#[cfg(target_family = "wasm")]
compile_error!("joko_desktop crate is not allowed to compile on wasm platforms");

pub fn add_desktop_addons(app: &mut App) {
    app.insert_resource(ClearColor(Color::NONE))
        .insert_resource(WindowDescriptor {
            transparent: true,
            ..Default::default()
        });
    app.add_startup_system(insert_camera);
    app.add_plugins(bevy::DefaultPlugins);
    app.add_plugin(bevy_glfw::GlfwPlugin);

    app.add_plugin(bevy_egui::EguiPlugin);

    // app.add_plugin(bevy_inspector_egui::WorldInspectorPlugin::new());
    app.add_system_to_stage(CoreStage::Last, egui_glfw_passthrough);
    app.add_plugin(mumble::MumblePlugin);
    app.add_plugin(jmf::bevy::MarkerPlugin);
}
fn insert_camera(mut commands: Commands) {
    commands.spawn_bundle(Camera3dBundle::default());
}
fn spawn_some_objects(_commands: Commands) {}
fn egui_glfw_passthrough(
    mut ectx: ResMut<EguiContext>,
    mut glfw_backend: NonSendMut<bevy_glfw::GlfwBackend>,
    windows: Res<Windows>,
) {
    for win in windows.iter() {
        let window_id = win.id();
        let ctx = ectx.ctx_for_window_mut(window_id);
        if let Some(window_state) = glfw_backend.get_window_mut(&window_id) {
            if ctx.wants_keyboard_input() || ctx.wants_pointer_input() || ctx.is_using_pointer() {
                window_state.set_passthrough(false);
            } else {
                window_state.set_passthrough(true);
            }
        }
    }
}
