mod sim;
mod ui;
mod water;

use bevy::{
    diagnostic::FrameTimeDiagnosticsPlugin,
    prelude::*,
    sprite_render::Material2dPlugin,
};

use sim::{
    ARENA_HALF_H, ARENA_HALF_W, DEFAULT_UNIT_RADIUS, SimState, SpatialGrid, grid_cell_size,
    step_simulation,
};
use ui::{UiState, handle_input, setup_ui, update_hud};
use water::{WaterMaterial, setup_scene, update_water_time};

fn main() -> AppExit {
    App::new()
        .add_plugins((
            DefaultPlugins.set(WindowPlugin {
                primary_window: Some(Window {
                    title: "MIZU".into(),
                    ..default()
                }),
                ..default()
            }),
            FrameTimeDiagnosticsPlugin::default(),
            Material2dPlugin::<WaterMaterial>::default(),
        ))
        .insert_resource(ClearColor(Color::srgb(0.05, 0.06, 0.09)))
        .insert_resource(Time::<Fixed>::from_hz(120.0))
        .insert_resource(SimState::default())
        .insert_resource(UiState::default())
        .insert_resource(SpatialGrid::new(
            ARENA_HALF_W,
            ARENA_HALF_H,
            grid_cell_size(DEFAULT_UNIT_RADIUS),
        ))
        .add_systems(Startup, (setup_scene, setup_ui))
        .add_systems(
            Update,
            (handle_input, update_water_time, update_hud).chain(),
        )
        .add_systems(FixedUpdate, step_simulation)
        .run()
}
