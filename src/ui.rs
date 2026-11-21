use bevy::{
    camera::visibility::RenderLayers,
    diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin},
    ecs::system::SystemParam,
    input::mouse::{AccumulatedMouseScroll, MouseScrollUnit},
    prelude::*,
    window::{MonitorSelection, PrimaryWindow, VideoModeSelection, WindowMode},
};
use std::f32::consts::TAU;

use crate::sim::{
    ARENA_HALF_H, ARENA_HALF_W, ContainerShape, CursorMode, DEFAULT_GRAVITY_STRENGTH,
    DEFAULT_UNIT_RADIUS, GravityDirection, MAX_GRAVITY_STRENGTH, MAX_SPAWN_AMOUNT, MAX_UNIT_RADIUS,
    MIN_GRAVITY_STRENGTH, MIN_SPAWN_AMOUNT, MIN_UNIT_RADIUS, MotionMode, ParticleColorRamp,
    ParticleGradientMode, SimState, SpatialGrid, Unit, grid_cell_size, particle_color,
    particle_color_with, spawn_units, spawn_units_at,
};
use crate::water::{
    ArenaBackground, OffscreenCamera, UnitAssets, WaterMaterial, WaterPalette, WaterParams,
    WaterQuad,
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
pub enum WindowModeChoice {
    #[default]
    Windowed,
    Borderless,
    Fullscreen,
}

impl WindowModeChoice {
    pub const ALL: [Self; 3] = [Self::Windowed, Self::Borderless, Self::Fullscreen];

    pub fn name(self) -> &'static str {
        match self {
            Self::Windowed => "Windowed",
            Self::Borderless => "Borderless",
            Self::Fullscreen => "Fullscreen",
        }
    }

    pub fn next(self) -> Self {
        let idx = Self::ALL.iter().position(|&m| m == self).unwrap_or(0);
        Self::ALL[(idx + 1) % Self::ALL.len()]
    }

    pub fn prev(self) -> Self {
        let idx = Self::ALL.iter().position(|&m| m == self).unwrap_or(0);
        Self::ALL[(idx + Self::ALL.len() - 1) % Self::ALL.len()]
    }

    pub fn to_bevy(self) -> WindowMode {
        match self {
            Self::Windowed => WindowMode::Windowed,
            Self::Borderless => WindowMode::BorderlessFullscreen(MonitorSelection::Current),
            Self::Fullscreen => {
                WindowMode::Fullscreen(MonitorSelection::Current, VideoModeSelection::Current)
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ResolutionPreset {
    #[default]
    Hd720,
    Wide720,
    Fhd1080,
    Wide1080,
    Wxga900,
    Xga768,
}

impl ResolutionPreset {
    pub const ALL: [Self; 6] = [
        Self::Hd720,
        Self::Wide720,
        Self::Fhd1080,
        Self::Wide1080,
        Self::Wxga900,
        Self::Xga768,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Hd720 => "1280×720 (16:9)",
            Self::Wide720 => "1680×720 (21:9)",
            Self::Fhd1080 => "1920×1080 (16:9)",
            Self::Wide1080 => "2560×1080 (21:9)",
            Self::Wxga900 => "1440×900 (16:10)",
            Self::Xga768 => "1024×768 (4:3)",
        }
    }

    pub fn dimensions(self) -> (f32, f32) {
        match self {
            Self::Hd720 => (1280.0, 720.0),
            Self::Wide720 => (1680.0, 720.0),
            Self::Fhd1080 => (1920.0, 1080.0),
            Self::Wide1080 => (2560.0, 1080.0),
            Self::Wxga900 => (1440.0, 900.0),
            Self::Xga768 => (1024.0, 768.0),
        }
    }

    pub fn next(self) -> Self {
        let idx = Self::ALL.iter().position(|&r| r == self).unwrap_or(0);
        Self::ALL[(idx + 1) % Self::ALL.len()]
    }

    pub fn prev(self) -> Self {
        let idx = Self::ALL.iter().position(|&r| r == self).unwrap_or(0);
        Self::ALL[(idx + Self::ALL.len() - 1) % Self::ALL.len()]
    }
}

#[derive(Resource, Default)]
pub struct UiState {
    pub palette: WaterPalette,
    pub shader_panel_open: bool,
    pub ui_hidden: bool,
    pub zen_hint_timer: f32,
    pub pause_menu_open: bool,
    pub paused_before_menu: bool,
    pub window_mode: WindowModeChoice,
    pub resolution_preset: ResolutionPreset,
}

#[derive(Component)]
pub struct HudText;

#[derive(Component)]
pub struct ShaderPanel;

#[derive(Component)]
pub struct LowFpsBanner;

#[derive(Component)]
pub struct ZenHideable;

#[derive(Component)]
pub struct ZenHintBanner;

#[derive(Component)]
pub struct ZenHintText;

#[derive(Component)]
pub struct PauseMenuOverlay;

#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub enum UiAction {
    ForceDown,
    ForceUp,
    ToggleGravity,
    ToggleCursorMode,
    GravityStrengthDown,
    GravityStrengthUp,
    GravityDirPrev,
    GravityDirNext,
    MotionPrev,
    MotionNext,
    ContainerPrev,
    ContainerNext,
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
    ToggleGradientReverse,
    RampPrev,
    RampNext,
    ResetShader,
    ToggleHideUi,
    ClosePauseMenu,
    WindowModePrev,
    WindowModeNext,
    ResolutionPrev,
    ResolutionNext,
}

#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub enum UiLabel {
    Force,
    Gravity,
    CursorMode,
    GravityStrength,
    GravityDir,
    Motion,
    Container,
    ShaderPanelBtn,
    Palette,
    Threshold,
    Foam,
    Shimmer,
    Size,
    Gradient,
    GradientReverse,
    Ramp,
    WindowMode,
    Resolution,
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
            ZenHideable,
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
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(12.0),
                right: Val::Px(14.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::End,
                row_gap: Val::Px(8.0),
                ..default()
            },
            ZenHideable,
        ))
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
                    "Cursor (C): Force",
                    libron.clone(),
                    UiAction::ToggleCursorMode,
                    Some(UiLabel::CursorMode),
                    12.0,
                );

                spawn_pill_button(
                    row,
                    "Visuals (Tab)",
                    libron.clone(),
                    UiAction::ToggleShaderPanel,
                    Some(UiLabel::ShaderPanelBtn),
                    12.0,
                );

                spawn_pill_button(
                    row,
                    "Hide (H)",
                    libron.clone(),
                    UiAction::ToggleHideUi,
                    None,
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
                    Text::new("LIQUID MODE"),
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
                    "Invert",
                    "OFF",
                    libron.clone(),
                    UiAction::ToggleGradientReverse,
                    UiAction::ToggleGradientReverse,
                    UiLabel::GradientReverse,
                    "<",
                    ">",
                );

                panel.spawn((
                    Text::new("GRAVITY"),
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
                    "Strength",
                    "520",
                    libron.clone(),
                    UiAction::GravityStrengthDown,
                    UiAction::GravityStrengthUp,
                    UiLabel::GravityStrength,
                    "-",
                    "+",
                );

                spawn_stepper_row(
                    panel,
                    "Direction",
                    "Down",
                    libron.clone(),
                    UiAction::GravityDirPrev,
                    UiAction::GravityDirNext,
                    UiLabel::GravityDir,
                    "<",
                    ">",
                );

                spawn_stepper_row(
                    panel,
                    "Motion",
                    "Fluid",
                    libron.clone(),
                    UiAction::MotionPrev,
                    UiAction::MotionNext,
                    UiLabel::Motion,
                    "<",
                    ">",
                );

                spawn_stepper_row(
                    panel,
                    "Shape",
                    "Rectangle",
                    libron.clone(),
                    UiAction::ContainerPrev,
                    UiAction::ContainerNext,
                    UiLabel::Container,
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

    // Bottom keybind hint bar (hidden in Zen mode)
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                bottom: Val::Px(12.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            ZenHideable,
        ))
        .with_children(|wrapper| {
            wrapper
                .spawn((
                    Node {
                        padding: UiRect::axes(Val::Px(12.0), Val::Px(6.0)),
                        border: UiRect::all(Val::Px(1.0)),
                        border_radius: BorderRadius::all(Val::Px(8.0)),
                        ..default()
                    },
                    BorderColor::all(PANEL_BORDER),
                    BackgroundColor(PANEL_BG),
                    Interaction::None,
                ))
                .with_children(|bar| {
                    bar.spawn((
                        Text::new("W: View  |  C: Cursor  |  L/R-Drag: Force / Add-Del  |  [/] Scroll: Force/Amt  |  +/-: ±500  |  Space: Pause  |  R: Reset  |  Tab: Visuals  |  H: Hide  |  Esc: Menu"),
                        TextFont {
                            font: libron.clone(),
                            font_size: 12.5,
                            ..default()
                        },
                        TextColor(TEXT_MUTED),
                    ));
                });
        });

    // Centered low-FPS warning banner (hidden by default)
    commands
        .spawn((
            Node {
                display: Display::None,
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                top: Val::Px(72.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            LowFpsBanner,
        ))
        .with_children(|wrapper| {
            wrapper
                .spawn((
                    Node {
                        padding: UiRect::axes(Val::Px(18.0), Val::Px(10.0)),
                        border: UiRect::all(Val::Px(1.5)),
                        border_radius: BorderRadius::all(Val::Px(8.0)),
                        ..default()
                    },
                    BorderColor::all(Color::srgba(1.0, 0.42, 0.32, 0.85)),
                    BackgroundColor(Color::srgba(0.26, 0.06, 0.06, 0.92)),
                ))
                .with_children(|banner| {
                    banner.spawn((
                        Text::new("Low FPS (< 10) — particle spawning paused"),
                        TextFont {
                            font: libron.clone(),
                            font_size: 16.0,
                            ..default()
                        },
                        TextColor(Color::srgb(1.0, 0.88, 0.84)),
                    ));
                });
        });

    // Brief fade-out hint when entering Zen (Hide UI) mode
    commands
        .spawn((
            Node {
                display: Display::None,
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                bottom: Val::Px(22.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            ZenHintBanner,
        ))
        .with_children(|wrapper| {
            wrapper
                .spawn((
                    Node {
                        padding: UiRect::axes(Val::Px(16.0), Val::Px(8.0)),
                        border: UiRect::all(Val::Px(1.0)),
                        border_radius: BorderRadius::all(Val::Px(8.0)),
                        ..default()
                    },
                    BorderColor::all(PANEL_BORDER),
                    BackgroundColor(PANEL_BG),
                ))
                .with_children(|hint| {
                    hint.spawn((
                        Text::new("UI hidden — press H to restore  |  Esc: Menu"),
                        TextFont {
                            font: libron.clone(),
                            font_size: 14.0,
                            ..default()
                        },
                        TextColor(TEXT_PRIMARY),
                        ZenHintText,
                    ));
                });
        });

    // Esc Pause Menu modal overlay
    commands
        .spawn((
            Node {
                display: Display::None,
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.02, 0.03, 0.06, 0.72)),
            ZIndex(100),
            Interaction::None,
            PauseMenuOverlay,
        ))
        .with_children(|backdrop| {
            backdrop
                .spawn((
                    Node {
                        width: Val::Px(700.0),
                        padding: UiRect::all(Val::Px(24.0)),
                        border: UiRect::all(Val::Px(1.5)),
                        border_radius: BorderRadius::all(Val::Px(12.0)),
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(16.0),
                        ..default()
                    },
                    BorderColor::all(PANEL_BORDER),
                    BackgroundColor(Color::srgba(0.06, 0.08, 0.13, 0.96)),
                    Interaction::None,
                ))
                .with_children(|modal| {
                    // Header: MIZU + PAUSED subtitle
                    modal
                        .spawn(Node {
                            width: Val::Percent(100.0),
                            justify_content: JustifyContent::SpaceBetween,
                            align_items: AlignItems::Baseline,
                            ..default()
                        })
                        .with_children(|header| {
                            header.spawn((
                                Text::new("MIZU"),
                                TextFont {
                                    font: libron.clone(),
                                    font_size: 32.0,
                                    ..default()
                                },
                                TextColor(TEXT_PRIMARY),
                            ));
                            header.spawn((
                                Text::new("PAUSED"),
                                TextFont {
                                    font: libron.clone(),
                                    font_size: 13.0,
                                    ..default()
                                },
                                TextColor(TEXT_MUTED),
                            ));
                        });

                    // Horizontal divider below header
                    modal.spawn((
                        Node {
                            width: Val::Percent(100.0),
                            height: Val::Px(1.0),
                            ..default()
                        },
                        BackgroundColor(PANEL_BORDER),
                    ));

                    // Two-column body: description (left) | divider | display controls (right)
                    modal
                        .spawn(Node {
                            width: Val::Percent(100.0),
                            flex_direction: FlexDirection::Row,
                            column_gap: Val::Px(20.0),
                            align_items: AlignItems::Stretch,
                            ..default()
                        })
                        .with_children(|body| {
                            // Left column: about Mizu
                            body.spawn(Node {
                                flex_basis: Val::Px(0.0),
                                flex_grow: 1.05,
                                flex_direction: FlexDirection::Column,
                                row_gap: Val::Px(10.0),
                                justify_content: JustifyContent::Center,
                                ..default()
                            })
                            .with_children(|about| {
                                about.spawn((
                                    Text::new("Mizu is a game with no objective—no achievements, no quests, no story. It's simply about enjoying something that doesn't ask anything of you."),
                                    TextFont {
                                        font: libron.clone(),
                                        font_size: 13.5,
                                        ..default()
                                    },
                                    TextColor(TEXT_MUTED),
                                ));
                                about.spawn((
                                    Text::new("Best enjoyed with your favorite music playing in the background, or to help keep focus while listening to something memorable."),
                                    TextFont {
                                        font: libron.clone(),
                                        font_size: 13.5,
                                        ..default()
                                    },
                                    TextColor(TEXT_MUTED),
                                ));
                                about.spawn((
                                    Text::new("So slow down, calm down, and enjoy your time with the particles."),
                                    TextFont {
                                        font: libron.clone(),
                                        font_size: 13.5,
                                        ..default()
                                    },
                                    TextColor(TEXT_PRIMARY),
                                ));
                            });

                            // Vertical divider
                            body.spawn((
                                Node {
                                    width: Val::Px(1.0),
                                    align_self: AlignSelf::Stretch,
                                    ..default()
                                },
                                BackgroundColor(PANEL_BORDER),
                            ));

                            // Right column: display settings & resume
                            body.spawn(Node {
                                flex_basis: Val::Px(0.0),
                                flex_grow: 1.0,
                                flex_direction: FlexDirection::Column,
                                row_gap: Val::Px(12.0),
                                justify_content: JustifyContent::SpaceBetween,
                                ..default()
                            })
                            .with_children(|controls| {
                                controls
                                    .spawn(Node {
                                        width: Val::Percent(100.0),
                                        flex_direction: FlexDirection::Column,
                                        row_gap: Val::Px(12.0),
                                        ..default()
                                    })
                                    .with_children(|settings| {
                                        settings.spawn((
                                            Text::new("DISPLAY"),
                                            TextFont {
                                                font: libron.clone(),
                                                font_size: 13.0,
                                                ..default()
                                            },
                                            TextColor(TEXT_MUTED),
                                        ));

                                        spawn_stepper_row(
                                            settings,
                                            "Window Mode",
                                            "Windowed",
                                            libron.clone(),
                                            UiAction::WindowModePrev,
                                            UiAction::WindowModeNext,
                                            UiLabel::WindowMode,
                                            "<",
                                            ">",
                                        );

                                        spawn_stepper_row(
                                            settings,
                                            "Resolution",
                                            "1280×720 (16:9)",
                                            libron.clone(),
                                            UiAction::ResolutionPrev,
                                            UiAction::ResolutionNext,
                                            UiLabel::Resolution,
                                            "<",
                                            ">",
                                        );
                                    });

                                controls
                                    .spawn(Node {
                                        width: Val::Percent(100.0),
                                        justify_content: JustifyContent::End,
                                        margin: UiRect::top(Val::Px(8.0)),
                                        ..default()
                                    })
                                    .with_children(|footer| {
                                        spawn_pill_button(
                                            footer,
                                            "Resume (Esc)",
                                            libron,
                                            UiAction::ClosePauseMenu,
                                            None,
                                            16.0,
                                        );
                                    });
                            });
                        });
                });
        });
}

#[derive(SystemParam)]
pub struct UiQueries<'w, 's> {
    arena_bg: Query<'w, 's, &'static mut Mesh2d, With<ArenaBackground>>,
    water_quad: Query<'w, 's, &'static mut Visibility, With<WaterQuad>>,
    offscreen_cam: Query<'w, 's, &'static mut Camera, With<OffscreenCamera>>,
    shader_panel: Query<
        'w,
        's,
        &'static mut Node,
        (With<ShaderPanel>, Without<ZenHideable>, Without<PauseMenuOverlay>),
    >,
    zen_hideable: Query<
        'w,
        's,
        &'static mut Node,
        (With<ZenHideable>, Without<ShaderPanel>, Without<PauseMenuOverlay>),
    >,
    pause_menu: Query<
        'w,
        's,
        &'static mut Node,
        (With<PauseMenuOverlay>, Without<ShaderPanel>, Without<ZenHideable>),
    >,
    windows: Query<'w, 's, &'static mut Window, With<PrimaryWindow>>,
    camera_q: Query<'w, 's, (&'static Camera, &'static GlobalTransform), Without<OffscreenCamera>>,
    panel_interactions: Query<'w, 's, &'static Interaction, Without<UiAction>>,
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
    labels: Query<'w, 's, (&'static UiLabel, &'static mut Text)>,
}

pub fn handle_input(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    scroll: Res<AccumulatedMouseScroll>,
    assets: Res<UnitAssets>,
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
    let mut amount_delta: isize = 0;
    let is_spawn_mode = state.cursor_mode == CursorMode::Spawn;

    if keys.just_pressed(KeyCode::BracketRight) {
        if is_spawn_mode {
            amount_delta += 5;
        } else {
            force_delta += 300.0;
        }
    }
    if keys.just_pressed(KeyCode::BracketLeft) {
        if is_spawn_mode {
            amount_delta -= 5;
        } else {
            force_delta -= 300.0;
        }
    }
    if scroll.delta.y.abs() > 0.0 {
        if is_spawn_mode {
            let step = match scroll.unit {
                MouseScrollUnit::Line => (scroll.delta.y * 2.0).round() as isize,
                MouseScrollUnit::Pixel => (scroll.delta.y * 0.05).round().clamp(-5.0, 5.0) as isize,
            };
            amount_delta += if step == 0 {
                scroll.delta.y.signum() as isize
            } else {
                step
            };
        } else {
            let step = match scroll.unit {
                MouseScrollUnit::Line => scroll.delta.y * 150.0,
                MouseScrollUnit::Pixel => (scroll.delta.y * 3.0).clamp(-300.0, 300.0),
            };
            force_delta += step;
        }
    }

    let mut gravity_toggled = keys.just_pressed(KeyCode::KeyG);
    let mut cursor_toggled = keys.just_pressed(KeyCode::KeyC);
    let mut panel_toggled = keys.just_pressed(KeyCode::Tab) && !ui_state.pause_menu_open;
    let mut hide_ui_toggled = keys.just_pressed(KeyCode::KeyH) && !ui_state.pause_menu_open;
    let mut pause_menu_toggled = keys.just_pressed(KeyCode::Escape);
    let mut shader_dirty = false;
    let mut size_dirty = false;
    let mut colors_dirty = false;
    let mut gravity_dirty = false;
    let mut motion_dirty = false;
    let mut container_dirty = false;
    let mut window_mode_dirty = false;
    let mut resolution_dirty = false;

    for (interaction, action, mut bg, mut border) in &mut ui.buttons {
        let clicked = *interaction == Interaction::Pressed && mouse.just_pressed(MouseButton::Left);
        if clicked {
            match action {
                UiAction::ForceDown => {
                    if is_spawn_mode {
                        amount_delta -= 5;
                    } else {
                        force_delta -= 300.0;
                    }
                }
                UiAction::ForceUp => {
                    if is_spawn_mode {
                        amount_delta += 5;
                    } else {
                        force_delta += 300.0;
                    }
                }
                UiAction::ToggleGravity => gravity_toggled = true,
                UiAction::ToggleCursorMode => cursor_toggled = true,
                UiAction::GravityStrengthDown => {
                    state.gravity_strength = (state.gravity_strength - 100.0)
                        .clamp(MIN_GRAVITY_STRENGTH, MAX_GRAVITY_STRENGTH);
                    gravity_dirty = true;
                }
                UiAction::GravityStrengthUp => {
                    state.gravity_strength = (state.gravity_strength + 100.0)
                        .clamp(MIN_GRAVITY_STRENGTH, MAX_GRAVITY_STRENGTH);
                    gravity_dirty = true;
                }
                UiAction::GravityDirPrev => {
                    state.gravity_direction = state.gravity_direction.prev();
                    gravity_dirty = true;
                }
                UiAction::GravityDirNext => {
                    state.gravity_direction = state.gravity_direction.next();
                    gravity_dirty = true;
                }
                UiAction::MotionPrev => {
                    state.motion_mode = state.motion_mode.prev();
                    motion_dirty = true;
                }
                UiAction::MotionNext => {
                    state.motion_mode = state.motion_mode.next();
                    motion_dirty = true;
                }
                UiAction::ContainerPrev => {
                    state.container_shape = state.container_shape.prev();
                    container_dirty = true;
                }
                UiAction::ContainerNext => {
                    state.container_shape = state.container_shape.next();
                    container_dirty = true;
                }
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
                        mat.params.threshold = (mat.params.threshold - 0.04).clamp(0.04, 0.44);
                        shader_dirty = true;
                    }
                }
                UiAction::ThresholdUp => {
                    if let Some(mat) = water_materials.get_mut(&assets.water_material) {
                        mat.params.threshold = (mat.params.threshold + 0.04).clamp(0.04, 0.44);
                        shader_dirty = true;
                    }
                }
                UiAction::FoamDown => {
                    if let Some(mat) = water_materials.get_mut(&assets.water_material) {
                        mat.params.foam_width = (mat.params.foam_width - 0.03).clamp(0.0, 0.27);
                        shader_dirty = true;
                    }
                }
                UiAction::FoamUp => {
                    if let Some(mat) = water_materials.get_mut(&assets.water_material) {
                        mat.params.foam_width = (mat.params.foam_width + 0.03).clamp(0.0, 0.27);
                        shader_dirty = true;
                    }
                }
                UiAction::ShimmerDown => {
                    if let Some(mat) = water_materials.get_mut(&assets.water_material) {
                        mat.params.shimmer = (mat.params.shimmer - 0.50).clamp(0.0, 3.00);
                        shader_dirty = true;
                    }
                }
                UiAction::ShimmerUp => {
                    if let Some(mat) = water_materials.get_mut(&assets.water_material) {
                        mat.params.shimmer = (mat.params.shimmer + 0.50).clamp(0.0, 3.00);
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
                UiAction::ToggleGradientReverse => {
                    state.gradient_reversed = !state.gradient_reversed;
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
                    state.gradient_reversed = false;
                    state.color_ramp = ParticleColorRamp::default();
                    state.gravity_strength = DEFAULT_GRAVITY_STRENGTH;
                    state.gravity_direction = GravityDirection::default();
                    state.motion_mode = MotionMode::default();
                    state.container_shape = ContainerShape::default();
                    shader_dirty = true;
                    size_dirty = true;
                    colors_dirty = true;
                    gravity_dirty = true;
                    motion_dirty = true;
                    container_dirty = true;
                }
                UiAction::ToggleHideUi => hide_ui_toggled = true,
                UiAction::ClosePauseMenu => {
                    if ui_state.pause_menu_open {
                        pause_menu_toggled = true;
                    }
                }
                UiAction::WindowModePrev => {
                    ui_state.window_mode = ui_state.window_mode.prev();
                    window_mode_dirty = true;
                }
                UiAction::WindowModeNext => {
                    ui_state.window_mode = ui_state.window_mode.next();
                    window_mode_dirty = true;
                }
                UiAction::ResolutionPrev => {
                    ui_state.resolution_preset = ui_state.resolution_preset.prev();
                    resolution_dirty = true;
                }
                UiAction::ResolutionNext => {
                    ui_state.resolution_preset = ui_state.resolution_preset.next();
                    resolution_dirty = true;
                }
            }
        }

        let is_active = matches!(
            action,
            UiAction::ToggleGravity if state.gravity_enabled
        ) || matches!(
            action,
            UiAction::ToggleCursorMode if state.cursor_mode == CursorMode::Spawn
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

    if hide_ui_toggled {
        ui_state.ui_hidden = !ui_state.ui_hidden;
        ui_state.zen_hint_timer = if ui_state.ui_hidden { 2.5 } else { 0.0 };
        let disp = if ui_state.ui_hidden {
            Display::None
        } else {
            Display::Flex
        };
        for mut node in &mut ui.zen_hideable {
            node.display = disp;
        }
    }

    if pause_menu_toggled {
        ui_state.pause_menu_open = !ui_state.pause_menu_open;
        if ui_state.pause_menu_open {
            ui_state.paused_before_menu = state.paused;
            state.paused = true;
        } else {
            state.paused = ui_state.paused_before_menu;
        }
        let disp = if ui_state.pause_menu_open {
            Display::Flex
        } else {
            Display::None
        };
        for mut node in &mut ui.pause_menu {
            node.display = disp;
        }
    }

    if resolution_dirty && ui_state.window_mode != WindowModeChoice::Windowed {
        ui_state.window_mode = WindowModeChoice::Windowed;
        window_mode_dirty = true;
    }

    if window_mode_dirty || resolution_dirty {
        if let Ok(mut window) = ui.windows.single_mut() {
            window.mode = ui_state.window_mode.to_bevy();
            if ui_state.window_mode == WindowModeChoice::Windowed {
                let (w, h) = ui_state.resolution_preset.dimensions();
                window.resolution.set(w, h);
            }
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

    if amount_delta != 0 {
        state.spawn_amount = (state.spawn_amount as isize + amount_delta)
            .clamp(MIN_SPAWN_AMOUNT as isize, MAX_SPAWN_AMOUNT as isize)
            as usize;
    }

    if gravity_toggled {
        state.gravity_enabled = !state.gravity_enabled;
    }

    if cursor_toggled {
        state.cursor_mode = state.cursor_mode.toggle();
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

    if container_dirty {
        let new_mesh = assets.arena_mesh(state.container_shape);
        for mut mesh2d in &mut ui.arena_bg {
            mesh2d.0 = new_mesh.clone();
        }
    }

    if force_delta != 0.0
        || amount_delta != 0
        || gravity_toggled
        || cursor_toggled
        || gravity_dirty
        || motion_dirty
        || container_dirty
        || shader_dirty
        || size_dirty
        || colors_dirty
        || panel_toggled
        || window_mode_dirty
        || resolution_dirty
    {
        let params = water_materials
            .get(&assets.water_material)
            .map(|m| m.params)
            .unwrap_or_default();

        for (label, mut text) in &mut ui.labels {
            match label {
                UiLabel::Force => {
                    **text = match state.cursor_mode {
                        CursorMode::Force => format!("Force: {:.0}", state.force_strength),
                        CursorMode::Spawn => format!("Amount: {}", state.spawn_amount),
                    };
                }
                UiLabel::Gravity => {
                    **text = if state.gravity_enabled {
                        "Gravity (G): ON".to_string()
                    } else {
                        "Gravity (G): OFF".to_string()
                    };
                }
                UiLabel::CursorMode => {
                    **text = format!("Cursor (C): {}", state.cursor_mode.name());
                }
                UiLabel::GravityStrength => {
                    **text = format!("{:.0}", state.gravity_strength);
                }
                UiLabel::GravityDir => {
                    **text = state.gravity_direction.name().to_string();
                }
                UiLabel::Motion => {
                    **text = state.motion_mode.name().to_string();
                }
                UiLabel::Container => {
                    **text = state.container_shape.name().to_string();
                }
                UiLabel::ShaderPanelBtn => {
                    **text = if ui_state.shader_panel_open {
                        "Visuals (Tab) ▲".to_string()
                    } else {
                        "Visuals (Tab)".to_string()
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
                UiLabel::GradientReverse => {
                    **text = if state.gradient_reversed {
                        "ON".to_string()
                    } else {
                        "OFF".to_string()
                    };
                }
                UiLabel::Ramp => {
                    **text = state.color_ramp.name().to_string();
                }
                UiLabel::WindowMode => {
                    **text = ui_state.window_mode.name().to_string();
                }
                UiLabel::Resolution => {
                    **text = ui_state.resolution_preset.name().to_string();
                }
            }
        }
    }

    if keys.just_pressed(KeyCode::Space) && !ui_state.pause_menu_open {
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

    if (keys.just_pressed(KeyCode::Equal) || keys.just_pressed(KeyCode::ArrowUp)) && !state.low_fps {
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

    if !ui_state.pause_menu_open
        && state.cursor_mode == CursorMode::Spawn
        && (mouse.pressed(MouseButton::Left) || mouse.pressed(MouseButton::Right))
    {
        let over_ui = ui.buttons.iter().any(|(i, ..)| *i != Interaction::None)
            || ui.panel_interactions.iter().any(|i| *i != Interaction::None);
        if !over_ui
            && let Some(world_pos) = ui
                .windows
                .single()
                .ok()
                .and_then(|w| w.cursor_position())
                .and_then(|cursor_pos| {
                    ui.camera_q
                        .single()
                        .ok()
                        .and_then(|(cam, cam_tf)| cam.viewport_to_world_2d(cam_tf, cursor_pos).ok())
                })
        {
            if mouse.pressed(MouseButton::Left) && !state.low_fps {
                let count = state.spawn_amount;
                let spread = (state.unit_radius * 3.5 + count as f32 * 0.45).max(16.0);
                spawn_units_at(&mut commands, &assets, &mut state, world_pos, count, spread);
            } else if mouse.pressed(MouseButton::Right) && state.unit_count > 0 {
                let brush_r_sq =
                    ((state.unit_radius * 6.0).max(24.0) + state.spawn_amount as f32 * 1.2).powi(2);
                let mut remaining = state.spawn_amount;
                for (entity, unit, _, _, _) in &units {
                    if remaining == 0 {
                        break;
                    }
                    if (unit.pos - world_pos).length_squared() <= brush_r_sq {
                        commands.entity(entity).despawn();
                        state.unit_count = state.unit_count.saturating_sub(1);
                        remaining -= 1;
                    }
                }
            }
        }
    }

    if keys.just_pressed(KeyCode::KeyR) {
        let radius = state.unit_radius;
        let shape = state.container_shape;
        let water_mode = state.water_mode;
        let gradient_mode = state.gradient_mode;
        let gradient_reversed = state.gradient_reversed;
        let color_ramp = state.color_ramp;
        for (_, mut unit, mut transform, mut sprite, _) in &mut units {
            let pos = shape.random_point(&mut state, radius);
            let angle = state.next_f32() * TAU;
            let speed = 55.0 + state.next_f32() * 65.0;
            unit.pos = pos;
            unit.vel = Vec2::new(angle.cos() * speed, angle.sin() * speed);
            unit.density = 0.0;
            transform.translation.x = pos.x;
            transform.translation.y = pos.y;
            if !water_mode {
                sprite.color = particle_color_with(
                    unit.pos,
                    unit.vel,
                    0.0,
                    unit.base_hue,
                    gradient_mode,
                    gradient_reversed,
                    color_ramp,
                );
            }
        }
    }
}

pub fn update_hud(
    diagnostics: Res<DiagnosticsStore>,
    time: Res<Time>,
    mut state: ResMut<SimState>,
    mut ui_state: ResMut<UiState>,
    mut q: Query<&mut Text, With<HudText>>,
    mut banner_q: Query<&mut Node, With<LowFpsBanner>>,
    mut zen_hint_q: Query<&mut Node, (With<ZenHintBanner>, Without<LowFpsBanner>)>,
    mut zen_hint_text_q: Query<&mut TextColor, With<ZenHintText>>,
) {
    let smoothed_fps = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FPS)
        .and_then(|d| d.smoothed());
    let fps = smoothed_fps.unwrap_or(0.0);

    if time.elapsed_secs() > 1.0
        && let Some(val) = smoothed_fps
    {
        state.low_fps = val < 10.0;
    }

    for mut node in &mut banner_q {
        node.display = if state.low_fps {
            Display::Flex
        } else {
            Display::None
        };
    }

    if ui_state.zen_hint_timer > 0.0 {
        ui_state.zen_hint_timer = (ui_state.zen_hint_timer - time.delta_secs()).max(0.0);
    }
    let show_zen_hint = ui_state.ui_hidden && !ui_state.pause_menu_open && ui_state.zen_hint_timer > 0.0;
    for mut node in &mut zen_hint_q {
        node.display = if show_zen_hint {
            Display::Flex
        } else {
            Display::None
        };
    }
    if show_zen_hint {
        let alpha = (ui_state.zen_hint_timer / 0.8).clamp(0.0, 1.0);
        for mut tc in &mut zen_hint_text_q {
            tc.0 = TEXT_PRIMARY.with_alpha(alpha);
        }
    }

    let mode = if state.water_mode {
        "LIQUID MODE"
    } else {
        "PARTICLES"
    };
    let status = if state.paused { "PAUSED" } else { "RUNNING" };
    for mut text in &mut q {
        **text = format!(
            "MIZU  |  {mode}  |  {} units  |  {fps:.0} FPS  |  {status}",
            state.unit_count
        );
    }
}
