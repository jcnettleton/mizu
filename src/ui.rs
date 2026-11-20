use bevy::{
    camera::visibility::RenderLayers,
    diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin},
    ecs::system::SystemParam,
    input::mouse::{AccumulatedMouseScroll, MouseScrollUnit},
    prelude::*,
};
use std::f32::consts::TAU;

use crate::sim::{
    ARENA_HALF_H, ARENA_HALF_W, DEFAULT_UNIT_RADIUS, MAX_UNIT_RADIUS, MIN_UNIT_RADIUS,
    ParticleColorRamp, ParticleGradientMode, SimState, SpatialGrid, Unit, grid_cell_size,
    particle_color, spawn_units,
};
use crate::water::{
    OffscreenCamera, UnitAssets, WaterMaterial, WaterPalette, WaterParams, WaterQuad,
};

const PANEL_BG: Color = Color::srgba(0.06, 0.08, 0.13, 0.86);
const PANEL_BORDER: Color = Color::srgba(0.45, 0.72, 0.95, 0.35);
const BTN_BG: Color = Color::srgba(0.11, 0.15, 0.24, 0.92);
const BTN_HOVER_BG: Color = Color::srgba(0.18, 0.25, 0.38, 0.98);
const BTN_ACTIVE_BG: Color = Color::srgba(0.12, 0.45, 0.78, 0.95);
const BTN_BORDER: Color = Color::srgba(0.45, 0.72, 0.95, 0.55);
const BTN_HOVER_BORDER: Color = Color::srgb(0.68, 0.90, 1.0);
const TEXT_PRIMARY: Color = Color::srgb(0.92, 0.96, 1.0);
const TEXT_MUTED: Color = Color::srgba(0.72, 0.82, 0.94, 0.88);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FontChoice {
    #[default]
    Libron,
    Default,
}

impl FontChoice {
    pub fn name(self) -> &'static str {
        match self {
            Self::Libron => "Libron",
            Self::Default => "Default",
        }
    }

    pub fn toggle(self) -> Self {
        match self {
            Self::Libron => Self::Default,
            Self::Default => Self::Libron,
        }
    }
}

#[derive(Resource)]
pub struct UiFonts {
    pub libron: Handle<Font>,
}

#[derive(Resource, Default)]
pub struct UiState {
    pub palette: WaterPalette,
    pub shader_panel_open: bool,
    pub font_choice: FontChoice,
}

#[derive(Component)]
pub struct HudText;

#[derive(Component)]
pub struct ShaderPanel;

#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub enum UiAction {
    ForceDown,
    ForceUp,
    ToggleGravity,
    ToggleShaderPanel,
    PalettePrev,
    PaletteNext,
    ThresholdDown,
    ThresholdUp,
    FoamDown,
    FoamUp,
    ShimmerDown,
    ShimmerUp,
    SizeDown,
    SizeUp,
    GradientPrev,
    GradientNext,
    RampPrev,
    RampNext,
    ToggleFont,
    ResetShader,
}

#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub enum UiLabel {
    Force,
    Gravity,
    ShaderPanelBtn,
    Palette,
    Threshold,
    Foam,
    Shimmer,
    Size,
    Gradient,
    Ramp,
    Font,
}

fn spawn_pill_button(
    parent: &mut ChildSpawnerCommands,
    text: &str,
    font: Handle<Font>,
    action: UiAction,
    label: Option<UiLabel>,
    pad_x: f32,
) {
    parent
        .spawn((
            Button,
            Node {
                padding: UiRect::axes(Val::Px(pad_x), Val::Px(6.0)),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(6.0)),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BorderColor::all(BTN_BORDER),
            BackgroundColor(BTN_BG),
            action,
        ))
        .with_children(|btn| {
            let mut text_cmd = btn.spawn((
                Text::new(text),
                TextFont {
                    font,
                    font_size: 14.0,
                    ..default()
                },
                TextColor(TEXT_PRIMARY),
            ));
            if let Some(lbl) = label {
                text_cmd.insert(lbl);
            }
        });
}

fn spawn_stepper_row(
    parent: &mut ChildSpawnerCommands,
    title: &str,
    initial_value: &str,
    font: Handle<Font>,
    dec_action: UiAction,
    inc_action: UiAction,
    value_label: UiLabel,
    dec_text: &str,
    inc_text: &str,
) {
    parent
        .spawn(Node {
            width: Val::Percent(100.0),
            justify_content: JustifyContent::SpaceBetween,
            align_items: AlignItems::Center,
            column_gap: Val::Px(10.0),
            ..default()
        })
        .with_children(|row| {
            row.spawn((
                Text::new(title),
                TextFont {
                    font: font.clone(),
                    font_size: 13.5,
                    ..default()
                },
                TextColor(TEXT_MUTED),
            ));

            row.spawn(Node {
                column_gap: Val::Px(6.0),
                align_items: AlignItems::Center,
                ..default()
            })
            .with_children(|controls| {
                spawn_pill_button(controls, dec_text, font.clone(), dec_action, None, 9.0);
                controls.spawn((
                    Text::new(initial_value),
                    TextFont {
                        font: font.clone(),
                        font_size: 13.5,
                        ..default()
                    },
                    TextColor(TEXT_PRIMARY),
                    Node {
                        min_width: Val::Px(56.0),
                        justify_content: JustifyContent::Center,
                        ..default()
                    },
                    value_label,
                ));
                spawn_pill_button(controls, inc_text, font, inc_action, None, 9.0);
            });
        });
}

pub fn setup_ui(mut commands: Commands, asset_server: Res<AssetServer>) {
    let libron: Handle<Font> = asset_server.load("fonts/Libron/Libron-Regular.ttf");
    commands.insert_resource(UiFonts {
        libron: libron.clone(),
    });

    // Top-left HUD card
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(12.0),
                left: Val::Px(14.0),
                padding: UiRect::axes(Val::Px(12.0), Val::Px(8.0)),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(8.0)),
                ..default()
            },
            BorderColor::all(PANEL_BORDER),
            BackgroundColor(PANEL_BG),
            Interaction::None,
        ))
        .with_children(|card| {
            card.spawn((
                Text::new("MIZU"),
                TextFont {
                    font: libron.clone(),
                    font_size: 14.5,
                    ..default()
                },
                TextColor(TEXT_PRIMARY),
                HudText,
            ));
        });

    // Top-right container (toolbar + collapsible shader panel)
    commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            top: Val::Px(12.0),
            right: Val::Px(14.0),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::End,
            row_gap: Val::Px(8.0),
            ..default()
        })
        .with_children(|col| {
            // Top-right toolbar card
            col.spawn((
                Node {
                    padding: UiRect::axes(Val::Px(10.0), Val::Px(6.0)),
                    border: UiRect::all(Val::Px(1.0)),
                    border_radius: BorderRadius::all(Val::Px(8.0)),
                    column_gap: Val::Px(8.0),
                    align_items: AlignItems::Center,
                    ..default()
                },
                BorderColor::all(PANEL_BORDER),
                BackgroundColor(PANEL_BG),
                Interaction::None,
            ))
            .with_children(|row| {
                spawn_pill_button(row, "-", libron.clone(), UiAction::ForceDown, None, 9.0);
                row.spawn((
                    Text::new("Force: 900"),
                    TextFont {
                        font: libron.clone(),
                        font_size: 14.0,
                        ..default()
                    },
                    TextColor(TEXT_PRIMARY),
                    UiLabel::Force,
                ));
                spawn_pill_button(row, "+", libron.clone(), UiAction::ForceUp, None, 9.0);

                spawn_pill_button(
                    row,
                    "Gravity (G): OFF",
                    libron.clone(),
                    UiAction::ToggleGravity,
                    Some(UiLabel::Gravity),
                    12.0,
                );

                spawn_pill_button(
                    row,
                    "Shader (Tab)",
                    libron.clone(),
                    UiAction::ToggleShaderPanel,
                    Some(UiLabel::ShaderPanelBtn),
                    12.0,
                );
            });

            // Collapsible Water & Shader Control Panel
            col.spawn((
                Node {
                    display: Display::None,
                    width: Val::Px(276.0),
                    padding: UiRect::all(Val::Px(12.0)),
                    border: UiRect::all(Val::Px(1.0)),
                    border_radius: BorderRadius::all(Val::Px(8.0)),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(10.0),
                    ..default()
                },
                BorderColor::all(PANEL_BORDER),
                BackgroundColor(PANEL_BG),
                Interaction::None,
                ShaderPanel,
            ))
            .with_children(|panel| {
                panel.spawn((
                    Text::new("WATER SHADER"),
                    TextFont {
                        font: libron.clone(),
                        font_size: 13.0,
                        ..default()
                    },
                    TextColor(TEXT_MUTED),
                ));

                spawn_stepper_row(
                    panel,
                    "Palette",
                    "Ocean",
                    libron.clone(),
                    UiAction::PalettePrev,
                    UiAction::PaletteNext,
                    UiLabel::Palette,
                    "<",
                    ">",
                );

                spawn_stepper_row(
                    panel,
                    "Threshold",
                    "0.15",
                    libron.clone(),
                    UiAction::ThresholdDown,
                    UiAction::ThresholdUp,
                    UiLabel::Threshold,
                    "-",
                    "+",
                );

                spawn_stepper_row(
                    panel,
                    "Foam Rim",
                    "0.05",
                    libron.clone(),
                    UiAction::FoamDown,
                    UiAction::FoamUp,
                    UiLabel::Foam,
                    "-",
                    "+",
                );

                spawn_stepper_row(
                    panel,
                    "Shimmer",
                    "1.00",
                    libron.clone(),
                    UiAction::ShimmerDown,
                    UiAction::ShimmerUp,
                    UiLabel::Shimmer,
                    "-",
                    "+",
                );

                panel.spawn((
                    Text::new("PARTICLES"),
                    TextFont {
                        font: libron.clone(),
                        font_size: 13.0,
                        ..default()
                    },
                    TextColor(TEXT_MUTED),
                    Node {
                        margin: UiRect::top(Val::Px(4.0)),
                        ..default()
                    },
                ));

                spawn_stepper_row(
                    panel,
                    "Size",
                    "4.0",
                    libron.clone(),
                    UiAction::SizeDown,
                    UiAction::SizeUp,
                    UiLabel::Size,
                    "-",
                    "+",
                );

                spawn_stepper_row(
                    panel,
                    "Gradient",
                    "Speed",
                    libron.clone(),
                    UiAction::GradientPrev,
                    UiAction::GradientNext,
                    UiLabel::Gradient,
                    "<",
                    ">",
                );

                spawn_stepper_row(
                    panel,
                    "Colors",
                    "Rainbow",
                    libron.clone(),
                    UiAction::RampPrev,
                    UiAction::RampNext,
                    UiLabel::Ramp,
                    "<",
                    ">",
                );

                spawn_stepper_row(
                    panel,
                    "Font",
                    "Libron",
                    libron.clone(),
                    UiAction::ToggleFont,
                    UiAction::ToggleFont,
                    UiLabel::Font,
                    "<",
                    ">",
                );

                panel
                    .spawn(Node {
                        width: Val::Percent(100.0),
                        justify_content: JustifyContent::End,
                        margin: UiRect::top(Val::Px(2.0)),
                        ..default()
                    })
                    .with_children(|footer| {
                        spawn_pill_button(
                            footer,
                            "Reset Defaults",
                            libron.clone(),
                            UiAction::ResetShader,
                            None,
                            12.0,
                        );
                    });
            });
        });
}

#[derive(SystemParam)]
pub struct UiQueries<'w, 's> {
    water_quad: Query<'w, 's, &'static mut Visibility, With<WaterQuad>>,
    offscreen_cam: Query<'w, 's, &'static mut Camera, With<OffscreenCamera>>,
    shader_panel: Query<'w, 's, &'static mut Node, With<ShaderPanel>>,
    buttons: Query<
        'w,
        's,
        (
            &'static Interaction,
            &'static UiAction,
            &'static mut BackgroundColor,
            &'static mut BorderColor,
        ),
    >,
    text_fonts: Query<'w, 's, &'static mut TextFont>,
    labels: Query<'w, 's, (&'static UiLabel, &'static mut Text)>,
}

pub fn handle_input(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    scroll: Res<AccumulatedMouseScroll>,
    assets: Res<UnitAssets>,
    ui_fonts: Res<UiFonts>,
    mut state: ResMut<SimState>,
    mut ui_state: ResMut<UiState>,
    mut grid: ResMut<SpatialGrid>,
    mut water_materials: ResMut<Assets<WaterMaterial>>,
    mut units: Query<(
        Entity,
        &mut Unit,
        &mut Transform,
        &mut Sprite,
        &mut RenderLayers,
    )>,
    mut ui: UiQueries,
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

    let mut gravity_toggled = keys.just_pressed(KeyCode::KeyG);
    let mut panel_toggled = keys.just_pressed(KeyCode::Tab);
    let mut shader_dirty = false;
    let mut size_dirty = false;
    let mut colors_dirty = false;
    let mut font_toggled = false;

    for (interaction, action, mut bg, mut border) in &mut ui.buttons {
        let clicked = *interaction == Interaction::Pressed && mouse.just_pressed(MouseButton::Left);
        if clicked {
            match action {
                UiAction::ForceDown => force_delta -= 300.0,
                UiAction::ForceUp => force_delta += 300.0,
                UiAction::ToggleGravity => gravity_toggled = true,
                UiAction::ToggleShaderPanel => panel_toggled = true,
                UiAction::PalettePrev => {
                    ui_state.palette = ui_state.palette.prev();
                    shader_dirty = true;
                }
                UiAction::PaletteNext => {
                    ui_state.palette = ui_state.palette.next();
                    shader_dirty = true;
                }
                UiAction::ThresholdDown => {
                    if let Some(mat) = water_materials.get_mut(&assets.water_material) {
                        mat.params.threshold = (mat.params.threshold - 0.02).clamp(0.05, 0.40);
                        shader_dirty = true;
                    }
                }
                UiAction::ThresholdUp => {
                    if let Some(mat) = water_materials.get_mut(&assets.water_material) {
                        mat.params.threshold = (mat.params.threshold + 0.02).clamp(0.05, 0.40);
                        shader_dirty = true;
                    }
                }
                UiAction::FoamDown => {
                    if let Some(mat) = water_materials.get_mut(&assets.water_material) {
                        mat.params.foam_width = (mat.params.foam_width - 0.01).clamp(0.0, 0.20);
                        shader_dirty = true;
                    }
                }
                UiAction::FoamUp => {
                    if let Some(mat) = water_materials.get_mut(&assets.water_material) {
                        mat.params.foam_width = (mat.params.foam_width + 0.01).clamp(0.0, 0.20);
                        shader_dirty = true;
                    }
                }
                UiAction::ShimmerDown => {
                    if let Some(mat) = water_materials.get_mut(&assets.water_material) {
                        mat.params.shimmer = (mat.params.shimmer - 0.25).clamp(0.0, 2.50);
                        shader_dirty = true;
                    }
                }
                UiAction::ShimmerUp => {
                    if let Some(mat) = water_materials.get_mut(&assets.water_material) {
                        mat.params.shimmer = (mat.params.shimmer + 0.25).clamp(0.0, 2.50);
                        shader_dirty = true;
                    }
                }
                UiAction::SizeDown => {
                    state.unit_radius =
                        (state.unit_radius - 0.5).clamp(MIN_UNIT_RADIUS, MAX_UNIT_RADIUS);
                    size_dirty = true;
                }
                UiAction::SizeUp => {
                    state.unit_radius =
                        (state.unit_radius + 0.5).clamp(MIN_UNIT_RADIUS, MAX_UNIT_RADIUS);
                    size_dirty = true;
                }
                UiAction::GradientPrev => {
                    state.gradient_mode = state.gradient_mode.prev();
                    colors_dirty = true;
                }
                UiAction::GradientNext => {
                    state.gradient_mode = state.gradient_mode.next();
                    colors_dirty = true;
                }
                UiAction::RampPrev => {
                    state.color_ramp = state.color_ramp.prev();
                    colors_dirty = true;
                }
                UiAction::RampNext => {
                    state.color_ramp = state.color_ramp.next();
                    colors_dirty = true;
                }
                UiAction::ToggleFont => {
                    ui_state.font_choice = ui_state.font_choice.toggle();
                    font_toggled = true;
                }
                UiAction::ResetShader => {
                    ui_state.palette = WaterPalette::Ocean;
                    if let Some(mat) = water_materials.get_mut(&assets.water_material) {
                        let current_time = mat.params.time;
                        mat.params = WaterParams {
                            time: current_time,
                            ..WaterParams::default()
                        };
                    }
                    state.unit_radius = DEFAULT_UNIT_RADIUS;
                    state.gradient_mode = ParticleGradientMode::default();
                    state.color_ramp = ParticleColorRamp::default();
                    shader_dirty = true;
                    size_dirty = true;
                    colors_dirty = true;
                }
            }
        }

        let is_active = matches!(
            action,
            UiAction::ToggleGravity if state.gravity_on
        ) || matches!(
            action,
            UiAction::ToggleShaderPanel if ui_state.shader_panel_open
        );

        *border = BorderColor::all(if *interaction == Interaction::Hovered {
            BTN_HOVER_BORDER
        } else {
            BTN_BORDER
        });
        *bg = BackgroundColor(if is_active {
            BTN_ACTIVE_BG
        } else if *interaction == Interaction::Hovered {
            BTN_HOVER_BG
        } else {
            BTN_BG
        });
    }

    if panel_toggled {
        ui_state.shader_panel_open = !ui_state.shader_panel_open;
        for mut node in &mut ui.shader_panel {
            node.display = if ui_state.shader_panel_open {
                Display::Flex
            } else {
                Display::None
            };
        }
    }

    if shader_dirty {
        if let Some(mat) = water_materials.get_mut(&assets.water_material) {
            ui_state.palette.apply_to(&mut mat.params);
        }
    }

    if force_delta != 0.0 {
        state.force_strength = (state.force_strength + force_delta).clamp(300.0, 4500.0);
    }

    if gravity_toggled {
        state.gravity_on = !state.gravity_on;
    }

    if font_toggled {
        let new_font = match ui_state.font_choice {
            FontChoice::Libron => ui_fonts.libron.clone(),
            FontChoice::Default => Handle::default(),
        };
        for mut tf in &mut ui.text_fonts {
            tf.font = new_font.clone();
        }
    }

    if size_dirty {
        grid.rebuild(ARENA_HALF_W, ARENA_HALF_H, grid_cell_size(state.unit_radius));
        let new_size = state.unit_sprite_size();
        for (_, _, _, mut sprite, _) in &mut units {
            sprite.custom_size = Some(new_size);
        }
    }

    if colors_dirty && !state.water_mode {
        for (_, unit, _, mut sprite, _) in &mut units {
            sprite.color = particle_color(unit.pos, unit.vel, unit.density, unit.base_hue, &state);
        }
    }

    if force_delta != 0.0
        || gravity_toggled
        || shader_dirty
        || size_dirty
        || colors_dirty
        || panel_toggled
        || font_toggled
    {
        let params = water_materials
            .get(&assets.water_material)
            .map(|m| m.params)
            .unwrap_or_default();

        for (label, mut text) in &mut ui.labels {
            match label {
                UiLabel::Force => {
                    **text = format!("Force: {:.0}", state.force_strength);
                }
                UiLabel::Gravity => {
                    **text = if state.gravity_on {
                        "Gravity (G): ON".to_string()
                    } else {
                        "Gravity (G): OFF".to_string()
                    };
                }
                UiLabel::ShaderPanelBtn => {
                    **text = if ui_state.shader_panel_open {
                        "Shader (Tab) ▲".to_string()
                    } else {
                        "Shader (Tab)".to_string()
                    };
                }
                UiLabel::Palette => {
                    **text = ui_state.palette.name().to_string();
                }
                UiLabel::Threshold => {
                    **text = format!("{:.2}", params.threshold);
                }
                UiLabel::Foam => {
                    **text = format!("{:.2}", params.foam_width);
                }
                UiLabel::Shimmer => {
                    **text = format!("{:.2}", params.shimmer);
                }
                UiLabel::Size => {
                    **text = format!("{:.1}", state.unit_radius);
                }
                UiLabel::Gradient => {
                    **text = state.gradient_mode.name().to_string();
                }
                UiLabel::Ramp => {
                    **text = state.color_ramp.name().to_string();
                }
                UiLabel::Font => {
                    **text = ui_state.font_choice.name().to_string();
                }
            }
        }
    }

    if keys.just_pressed(KeyCode::Space) {
        state.paused = !state.paused;
    }

    if keys.just_pressed(KeyCode::KeyW) {
        state.water_mode = !state.water_mode;
        let water = state.water_mode;

        for mut vis in &mut ui.water_quad {
            *vis = if water {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
        }
        for mut cam in &mut ui.offscreen_cam {
            cam.is_active = water;
        }

        let size = state.unit_sprite_size();
        let (tex, layer) = if water {
            (assets.blob_texture.clone(), RenderLayers::layer(1))
        } else {
            (assets.circle_texture.clone(), RenderLayers::layer(0))
        };

        for (_, unit, _, mut sprite, mut layers) in &mut units {
            sprite.image = tex.clone();
            sprite.custom_size = Some(size);
            if water {
                sprite.color = Color::WHITE;
            } else {
                sprite.color =
                    particle_color(unit.pos, unit.vel, unit.density, unit.base_hue, &state);
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
        let radius = state.unit_radius;
        let water_mode = state.water_mode;
        let gradient_mode = state.gradient_mode;
        let color_ramp = state.color_ramp;
        for (_, mut unit, mut transform, mut sprite, _) in &mut units {
            let x = (state.next_f32() * 2.0 - 1.0) * (ARENA_HALF_W - radius);
            let y = (state.next_f32() * 2.0 - 1.0) * (ARENA_HALF_H - radius);
            let angle = state.next_f32() * TAU;
            let speed = 55.0 + state.next_f32() * 65.0;
            unit.pos = Vec2::new(x, y);
            unit.vel = Vec2::new(angle.cos() * speed, angle.sin() * speed);
            unit.density = 0.0;
            transform.translation.x = x;
            transform.translation.y = y;
            if !water_mode {
                if color_ramp == ParticleColorRamp::Rainbow {
                    let t = match gradient_mode {
                        ParticleGradientMode::Speed => (unit.vel.length() / 220.0).clamp(0.0, 1.0),
                        ParticleGradientMode::Height => {
                            ((unit.pos.y + ARENA_HALF_H) / (2.0 * ARENA_HALF_H)).clamp(0.0, 1.0)
                        }
                        ParticleGradientMode::Density => 0.0,
                    };
                    sprite.color = Color::hsl(unit.base_hue, 0.85, 0.48 + t * 0.25);
                } else {
                    let t = match gradient_mode {
                        ParticleGradientMode::Speed => (unit.vel.length() / 280.0).clamp(0.0, 1.0),
                        ParticleGradientMode::Height => {
                            ((unit.pos.y + ARENA_HALF_H) / (2.0 * ARENA_HALF_H)).clamp(0.0, 1.0)
                        }
                        ParticleGradientMode::Density => 0.0,
                    };
                    sprite.color = color_ramp.sample(t);
                }
            }
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
            "MIZU  |  {mode}  |  {} units  |  {fps:.0} FPS  |  {status}\n\
             W: View   L/R-Drag: Repel/Attract   [/] Scroll: Force   +/-: ±500   Space: Pause   R: Reset   Tab: Shader",
            state.unit_count
        );
    }
}
