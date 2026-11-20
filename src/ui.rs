use bevy::{
    camera::visibility::RenderLayers,
    diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin},
    input::mouse::{AccumulatedMouseScroll, MouseScrollUnit},
    prelude::*,
};
use std::f32::consts::TAU;

use crate::sim::{ARENA_HALF_H, ARENA_HALF_W, SimState, UNIT_RADIUS, Unit, spawn_units};
use crate::water::{OffscreenCamera, UnitAssets, WaterQuad};

#[derive(Component)]
pub struct HudText;

#[derive(Component)]
pub struct GravityButton;

#[derive(Component)]
pub struct GravityButtonText;

#[derive(Component)]
pub struct ForceDownButton;

#[derive(Component)]
pub struct ForceUpButton;

#[derive(Component)]
pub struct ForceLabelText;

pub fn setup_ui(mut commands: Commands) {
    // On-screen HUD (Layer 0)
    commands.spawn((
        Text::new("MIZU"),
        TextFont {
            font_size: 16.0,
            ..default()
        },
        TextColor(Color::srgba(0.92, 0.95, 1.0, 0.92)),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(12.0),
            left: Val::Px(14.0),
            ..default()
        },
        HudText,
    ));

    // Top-right control bar (Force - / + and Gravity toggle)
    commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            top: Val::Px(12.0),
            right: Val::Px(14.0),
            column_gap: Val::Px(8.0),
            align_items: AlignItems::Center,
            ..default()
        })
        .with_children(|row| {
            row.spawn((
                Button,
                Node {
                    padding: UiRect::axes(Val::Px(10.0), Val::Px(8.0)),
                    border: UiRect::all(Val::Px(1.5)),
                    border_radius: BorderRadius::all(Val::Px(6.0)),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
                BorderColor::all(Color::srgba(0.45, 0.72, 0.95, 0.7)),
                BackgroundColor(Color::srgba(0.10, 0.14, 0.22, 0.9)),
                ForceDownButton,
            ))
            .with_children(|btn| {
                btn.spawn((
                    Text::new("-"),
                    TextFont {
                        font_size: 16.0,
                        ..default()
                    },
                    TextColor(Color::srgb(0.90, 0.95, 1.0)),
                ));
            });

            row.spawn((
                Text::new("Force: 900"),
                TextFont {
                    font_size: 15.0,
                    ..default()
                },
                TextColor(Color::srgb(0.90, 0.95, 1.0)),
                ForceLabelText,
            ));

            row.spawn((
                Button,
                Node {
                    padding: UiRect::axes(Val::Px(10.0), Val::Px(8.0)),
                    border: UiRect::all(Val::Px(1.5)),
                    border_radius: BorderRadius::all(Val::Px(6.0)),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
                BorderColor::all(Color::srgba(0.45, 0.72, 0.95, 0.7)),
                BackgroundColor(Color::srgba(0.10, 0.14, 0.22, 0.9)),
                ForceUpButton,
            ))
            .with_children(|btn| {
                btn.spawn((
                    Text::new("+"),
                    TextFont {
                        font_size: 16.0,
                        ..default()
                    },
                    TextColor(Color::srgb(0.90, 0.95, 1.0)),
                ));
            });

            row.spawn((
                Button,
                Node {
                    padding: UiRect::axes(Val::Px(14.0), Val::Px(8.0)),
                    border: UiRect::all(Val::Px(1.5)),
                    border_radius: BorderRadius::all(Val::Px(6.0)),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
                BorderColor::all(Color::srgba(0.45, 0.72, 0.95, 0.7)),
                BackgroundColor(Color::srgba(0.10, 0.14, 0.22, 0.9)),
                GravityButton,
            ))
            .with_children(|parent| {
                parent.spawn((
                    Text::new("Gravity (G): OFF"),
                    TextFont {
                        font_size: 15.0,
                        ..default()
                    },
                    TextColor(Color::srgb(0.90, 0.95, 1.0)),
                    GravityButtonText,
                ));
            });
        });
}

pub fn handle_input(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    scroll: Res<AccumulatedMouseScroll>,
    assets: Res<UnitAssets>,
    mut state: ResMut<SimState>,
    mut units: Query<(
        Entity,
        &mut Unit,
        &mut Transform,
        &mut Sprite,
        &mut RenderLayers,
    )>,
    mut water_quad: Query<&mut Visibility, With<WaterQuad>>,
    mut offscreen_cam: Query<&mut Camera, With<OffscreenCamera>>,
    mut grav_btn: Query<
        (&Interaction, &mut BackgroundColor, &mut BorderColor),
        (
            With<GravityButton>,
            Without<ForceDownButton>,
            Without<ForceUpButton>,
        ),
    >,
    mut force_down_btn: Query<
        (&Interaction, &mut BackgroundColor, &mut BorderColor),
        (
            With<ForceDownButton>,
            Without<GravityButton>,
            Without<ForceUpButton>,
        ),
    >,
    mut force_up_btn: Query<
        (&Interaction, &mut BackgroundColor, &mut BorderColor),
        (
            With<ForceUpButton>,
            Without<GravityButton>,
            Without<ForceDownButton>,
        ),
    >,
    mut grav_text: Query<&mut Text, (With<GravityButtonText>, Without<ForceLabelText>)>,
    mut force_text: Query<&mut Text, (With<ForceLabelText>, Without<GravityButtonText>)>,
) {
    let mut force_delta: f32 = 0.0;
    if keys.just_pressed(KeyCode::BracketRight) {
        force_delta += 300.0;
    }
    if keys.just_pressed(KeyCode::BracketLeft) {
        force_delta -= 300.0;
    }
    if scroll.delta.y.abs() > 0.0 {
        let step = match scroll.unit {
            MouseScrollUnit::Line => scroll.delta.y * 150.0,
            MouseScrollUnit::Pixel => (scroll.delta.y * 3.0).clamp(-300.0, 300.0),
        };
        force_delta += step;
    }

    for (interaction, mut bg, mut border) in &mut force_down_btn {
        if *interaction == Interaction::Pressed && mouse.just_pressed(MouseButton::Left) {
            force_delta -= 300.0;
        }
        *border = BorderColor::all(if *interaction == Interaction::Hovered {
            Color::srgb(0.65, 0.88, 1.0)
        } else {
            Color::srgba(0.45, 0.72, 0.95, 0.7)
        });
        *bg = BackgroundColor(if *interaction == Interaction::Hovered {
            Color::srgba(0.16, 0.22, 0.34, 0.95)
        } else {
            Color::srgba(0.10, 0.14, 0.22, 0.9)
        });
    }

    for (interaction, mut bg, mut border) in &mut force_up_btn {
        if *interaction == Interaction::Pressed && mouse.just_pressed(MouseButton::Left) {
            force_delta += 300.0;
        }
        *border = BorderColor::all(if *interaction == Interaction::Hovered {
            Color::srgb(0.65, 0.88, 1.0)
        } else {
            Color::srgba(0.45, 0.72, 0.95, 0.7)
        });
        *bg = BackgroundColor(if *interaction == Interaction::Hovered {
            Color::srgba(0.16, 0.22, 0.34, 0.95)
        } else {
            Color::srgba(0.10, 0.14, 0.22, 0.9)
        });
    }

    if force_delta != 0.0 {
        state.force_strength = (state.force_strength + force_delta).clamp(300.0, 4500.0);
        for mut text in &mut force_text {
            **text = format!("Force: {:.0}", state.force_strength);
        }
    }

    let mut gravity_toggled = keys.just_pressed(KeyCode::KeyG);

    for (interaction, mut bg, mut border) in &mut grav_btn {
        match *interaction {
            Interaction::Pressed => {}
            Interaction::Hovered => {
                *border = BorderColor::all(Color::srgb(0.65, 0.88, 1.0));
            }
            Interaction::None => {
                *border = BorderColor::all(Color::srgba(0.45, 0.72, 0.95, 0.7));
            }
        }
        if *interaction == Interaction::Pressed && mouse.just_pressed(MouseButton::Left) {
            gravity_toggled = true;
        }
        *bg = if state.gravity_on {
            BackgroundColor(Color::srgba(0.12, 0.45, 0.78, 0.95))
        } else if *interaction == Interaction::Hovered {
            BackgroundColor(Color::srgba(0.16, 0.22, 0.34, 0.95))
        } else {
            BackgroundColor(Color::srgba(0.10, 0.14, 0.22, 0.9))
        };
    }

    if gravity_toggled {
        state.gravity_on = !state.gravity_on;
        for mut text in &mut grav_text {
            **text = if state.gravity_on {
                "Gravity (G): ON".to_string()
            } else {
                "Gravity (G): OFF".to_string()
            };
        }
        for (_, mut bg, _) in &mut grav_btn {
            *bg = if state.gravity_on {
                BackgroundColor(Color::srgba(0.12, 0.45, 0.78, 0.95))
            } else {
                BackgroundColor(Color::srgba(0.10, 0.14, 0.22, 0.9))
            };
        }
    }

    if keys.just_pressed(KeyCode::Space) {
        state.paused = !state.paused;
    }

    if keys.just_pressed(KeyCode::KeyW) {
        state.water_mode = !state.water_mode;
        let water = state.water_mode;

        for mut vis in &mut water_quad {
            *vis = if water {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
        }
        for mut cam in &mut offscreen_cam {
            cam.is_active = water;
        }

        let (tex, size, layer) = if water {
            (
                assets.blob_texture.clone(),
                Vec2::splat(UNIT_RADIUS * 9.5),
                RenderLayers::layer(1),
            )
        } else {
            (
                assets.circle_texture.clone(),
                Vec2::splat(UNIT_RADIUS * 2.0),
                RenderLayers::layer(0),
            )
        };

        for (_, unit, _, mut sprite, mut layers) in &mut units {
            sprite.image = tex.clone();
            sprite.custom_size = Some(size);
            if water {
                sprite.color = Color::WHITE;
            } else {
                let speed_t = (unit.vel.length() / 220.0).clamp(0.0, 1.0);
                sprite.color = Color::hsl(unit.base_hue, 0.85, 0.48 + speed_t * 0.25);
            }
            *layers = layer.clone();
        }
    }

    if keys.just_pressed(KeyCode::Equal) || keys.just_pressed(KeyCode::ArrowUp) {
        spawn_units(&mut commands, &assets, &mut state, 500);
    }

    if keys.just_pressed(KeyCode::Minus) || keys.just_pressed(KeyCode::ArrowDown) {
        let mut to_remove = 500usize.min(state.unit_count);
        if to_remove > 0 {
            for (entity, _, _, _, _) in &units {
                if to_remove == 0 {
                    break;
                }
                commands.entity(entity).despawn();
                to_remove -= 1;
                state.unit_count -= 1;
            }
        }
    }

    if keys.just_pressed(KeyCode::KeyR) {
        for (_, mut unit, mut transform, _, _) in &mut units {
            let x = (state.next_f32() * 2.0 - 1.0) * (ARENA_HALF_W - UNIT_RADIUS);
            let y = (state.next_f32() * 2.0 - 1.0) * (ARENA_HALF_H - UNIT_RADIUS);
            let angle = state.next_f32() * TAU;
            let speed = 55.0 + state.next_f32() * 65.0;
            unit.pos = Vec2::new(x, y);
            unit.vel = Vec2::new(angle.cos() * speed, angle.sin() * speed);
            transform.translation.x = x;
            transform.translation.y = y;
        }
    }
}

pub fn update_hud(
    diagnostics: Res<DiagnosticsStore>,
    state: Res<SimState>,
    mut q: Query<&mut Text, With<HudText>>,
) {
    let fps = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FPS)
        .and_then(|d| d.smoothed())
        .unwrap_or(0.0);

    let mode = if state.water_mode {
        "WATER SHADER"
    } else {
        "PARTICLES"
    };
    let status = if state.paused { "PAUSED" } else { "RUNNING" };
    for mut text in &mut q {
        **text = format!(
            "MIZU | Mode: {mode} | Units: {} | FPS: {fps:.0} | {status}\n\
             W: Water/Particles  |  Left/Right-Drag: Repel/Attract  |  [/] or Scroll: Force  |  +/Up & -/Down: ±500  |  Space: Pause  |  R: Reset",
            state.unit_count
        );
    }
}
