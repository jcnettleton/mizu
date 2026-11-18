use bevy::{
    asset::RenderAssetUsages,
    camera::{
        visibility::RenderLayers,
        RenderTarget,
    },
    diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin},
    input::mouse::AccumulatedMouseScroll,
    prelude::*,
    reflect::TypePath,
    render::render_resource::{AsBindGroup, Extent3d, ShaderType, TextureDimension, TextureFormat, TextureUsages},
    shader::ShaderRef,
    sprite_render::{AlphaMode2d, Material2d, Material2dPlugin},
    window::PrimaryWindow,
};
use std::f32::consts::TAU;

const ARENA_HALF_W: f32 = 620.0;
const ARENA_HALF_H: f32 = 340.0;
const RT_WIDTH: u32 = 1280;
const RT_HEIGHT: u32 = 720;
const UNIT_RADIUS: f32 = 4.0;
const CELL_SIZE: f32 = 10.0;
const INITIAL_UNITS: usize = 4000;

fn main() -> AppExit {
    App::new()
        .add_plugins((
            DefaultPlugins,
            FrameTimeDiagnosticsPlugin::default(),
            Material2dPlugin::<WaterMaterial>::default(),
        ))
        .insert_resource(ClearColor(Color::srgb(0.05, 0.06, 0.09)))
        .insert_resource(Time::<Fixed>::from_hz(120.0))
        .insert_resource(SimState::default())
        .insert_resource(SpatialGrid::new(ARENA_HALF_W, ARENA_HALF_H, CELL_SIZE))
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (
                handle_input,
                update_water_time,
                update_hud,
            )
                .chain(),
        )
        .add_systems(FixedUpdate, step_simulation)
        .run()
}

#[derive(Resource)]
struct SimState {
    paused: bool,
    water_mode: bool,
    gravity_on: bool,
    force_strength: f32,
    unit_count: usize,
    rng_seed: u64,
}

impl Default for SimState {
    fn default() -> Self {
        Self {
            paused: false,
            water_mode: true,
            gravity_on: false,
            force_strength: 900.0,
            unit_count: 0,
            rng_seed: 0x1234_5678_9abc_def0,
        }
    }
}

impl SimState {
    fn next_f32(&mut self) -> f32 {
        let mut x = self.rng_seed;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.rng_seed = x;
        ((x >> 40) as f32) / ((1u64 << 24) as f32)
    }
}

#[derive(Resource)]
struct UnitAssets {
    circle_texture: Handle<Image>,
    blob_texture: Handle<Image>,
    water_material: Handle<WaterMaterial>,
}

#[derive(Component)]
struct Unit {
    pos: Vec2,
    vel: Vec2,
    base_hue: f32,
}

#[derive(Component)]
struct HudText;

#[derive(Component)]
struct GravityButton;

#[derive(Component)]
struct GravityButtonText;

#[derive(Component)]
struct ForceDownButton;

#[derive(Component)]
struct ForceUpButton;

#[derive(Component)]
struct ForceLabelText;

#[derive(Component)]
struct WaterQuad;

#[derive(Component)]
struct OffscreenCamera;

#[derive(ShaderType, Debug, Clone, Copy)]
struct WaterParams {
    time: f32,
    threshold: f32,
    foam_width: f32,
    _pad: f32,
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
struct WaterMaterial {
    #[texture(0)]
    #[sampler(1)]
    density_texture: Handle<Image>,
    #[uniform(2)]
    params: WaterParams,
}

impl Material2d for WaterMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/water.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }
}

#[derive(Resource)]
struct SpatialGrid {
    cols: usize,
    rows: usize,
    cell_size: f32,
    origin_x: f32,
    origin_y: f32,
    cells: Vec<Vec<usize>>,
}

impl SpatialGrid {
    fn new(half_w: f32, half_h: f32, cell_size: f32) -> Self {
        let cols = ((half_w * 2.0) / cell_size).ceil() as usize + 2;
        let rows = ((half_h * 2.0) / cell_size).ceil() as usize + 2;
        Self {
            cols,
            rows,
            cell_size,
            origin_x: -half_w - cell_size,
            origin_y: -half_h - cell_size,
            cells: vec![Vec::with_capacity(8); cols * rows],
        }
    }

    fn clear(&mut self) {
        for cell in &mut self.cells {
            cell.clear();
        }
    }

    #[inline]
    fn cell_coords(&self, pos: Vec2) -> (isize, isize) {
        let cx = ((pos.x - self.origin_x) / self.cell_size).floor() as isize;
        let cy = ((pos.y - self.origin_y) / self.cell_size).floor() as isize;
        (
            cx.clamp(0, self.cols as isize - 1),
            cy.clamp(0, self.rows as isize - 1),
        )
    }

    #[inline]
    fn insert(&mut self, idx: usize, pos: Vec2) {
        let (cx, cy) = self.cell_coords(pos);
        self.cells[cy as usize * self.cols + cx as usize].push(idx);
    }
}

fn create_circle_image(size: u32) -> Image {
    let mut data = Vec::with_capacity((size * size * 4) as usize);
    let center = (size as f32 - 1.0) * 0.5;
    let radius = size as f32 * 0.46;

    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 - center;
            let dy = y as f32 - center;
            let dist = (dx * dx + dy * dy).sqrt();
            let alpha = (radius - dist + 0.75).clamp(0.0, 1.0);
            data.push(255);
            data.push(255);
            data.push(255);
            data.push((alpha * 255.0) as u8);
        }
    }

    Image::new(
        Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    )
}

fn create_blob_image(size: u32) -> Image {
    let mut data = Vec::with_capacity((size * size * 4) as usize);
    let center = (size as f32 - 1.0) * 0.5;
    let radius = size as f32 * 0.5;

    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 - center;
            let dy = y as f32 - center;
            let dist = ((dx * dx + dy * dy).sqrt() / radius).clamp(0.0, 1.0);
            // Wide, soft falloff so packed non-overlapping clumps build rich density
            let t = 1.0 - dist * dist;
            let alpha = (t * t * t) * 0.27;
            data.push(255);
            data.push(255);
            data.push(255);
            data.push((alpha * 255.0) as u8);
        }
    }

    Image::new(
        Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    )
}

fn setup(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut water_materials: ResMut<Assets<WaterMaterial>>,
    mut state: ResMut<SimState>,
) {
    // Main camera (Layer 0)
    commands.spawn((Camera2d, RenderLayers::layer(0)));

    // Offscreen render target for metaball density pass
    let mut rt_image = Image::new_fill(
        Extent3d {
            width: RT_WIDTH,
            height: RT_HEIGHT,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[0, 0, 0, 0],
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::default(),
    );
    rt_image.texture_descriptor.usage =
        TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST | TextureUsages::RENDER_ATTACHMENT;
    let rt_handle = images.add(rt_image);

    // Offscreen camera (Layer 1 -> renders blobs into rt_handle)
    commands.spawn((
        Camera2d,
        Camera {
            order: -1,
            clear_color: ClearColorConfig::Custom(Color::NONE),
            ..default()
        },
        RenderTarget::Image(rt_handle.clone().into()),
        RenderLayers::layer(1),
        OffscreenCamera,
    ));

    // Arena border background (Layer 0)
    commands.spawn((
        Sprite {
            color: Color::srgb(0.08, 0.10, 0.14),
            custom_size: Some(Vec2::new(ARENA_HALF_W * 2.0 + 16.0, ARENA_HALF_H * 2.0 + 16.0)),
            ..default()
        },
        Transform::from_xyz(0.0, 0.0, -2.0),
        RenderLayers::layer(0),
    ));

    // Full-arena water shader quad (Layer 0)
    let water_material = water_materials.add(WaterMaterial {
        density_texture: rt_handle,
        params: WaterParams {
            time: 0.0,
            threshold: 0.15,
            foam_width: 0.05,
            _pad: 0.0,
        },
    });

    commands.spawn((
        Mesh2d(meshes.add(Rectangle::new(RT_WIDTH as f32, RT_HEIGHT as f32))),
        MeshMaterial2d(water_material.clone()),
        Transform::from_xyz(0.0, 0.0, 1.0),
        RenderLayers::layer(0),
        WaterQuad,
    ));

    let circle_texture = images.add(create_circle_image(24));
    let blob_texture = images.add(create_blob_image(64));
    let assets = UnitAssets {
        circle_texture,
        blob_texture,
        water_material,
    };

    spawn_units(&mut commands, &assets, &mut state, INITIAL_UNITS);
    commands.insert_resource(assets);

    // On-screen HUD (Layer 0)
    commands.spawn((
        Text::new("Bevy 0.18 2D Crowd Demo"),
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

fn spawn_units(
    commands: &mut Commands,
    assets: &UnitAssets,
    state: &mut SimState,
    count: usize,
) {
    let (texture, size, layer) = if state.water_mode {
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

    let mut batch = Vec::with_capacity(count);
    for _ in 0..count {
        let x = (state.next_f32() * 2.0 - 1.0) * (ARENA_HALF_W - UNIT_RADIUS);
        let y = (state.next_f32() * 2.0 - 1.0) * (ARENA_HALF_H - UNIT_RADIUS);
        let angle = state.next_f32() * TAU;
        let speed = 55.0 + state.next_f32() * 65.0;
        let hue = state.next_f32() * 360.0;

        let color = if state.water_mode {
            Color::WHITE
        } else {
            Color::hsl(hue, 0.85, 0.60)
        };

        batch.push((
            Sprite {
                image: texture.clone(),
                color,
                custom_size: Some(size),
                ..default()
            },
            Transform::from_xyz(x, y, 0.0),
            layer.clone(),
            Unit {
                pos: Vec2::new(x, y),
                vel: Vec2::new(angle.cos() * speed, angle.sin() * speed),
                base_hue: hue,
            },
        ));
    }

    commands.spawn_batch(batch);
    state.unit_count += count;
}

fn handle_input(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    scroll: Res<AccumulatedMouseScroll>,
    assets: Res<UnitAssets>,
    mut state: ResMut<SimState>,
    mut units: Query<(Entity, &mut Unit, &mut Transform, &mut Sprite, &mut RenderLayers)>,
    mut water_quad: Query<&mut Visibility, With<WaterQuad>>,
    mut offscreen_cam: Query<&mut Camera, With<OffscreenCamera>>,
    mut grav_btn: Query<(&Interaction, &mut BackgroundColor, &mut BorderColor), (With<GravityButton>, Without<ForceDownButton>, Without<ForceUpButton>)>,
    mut force_down_btn: Query<(&Interaction, &mut BackgroundColor, &mut BorderColor), (With<ForceDownButton>, Without<GravityButton>, Without<ForceUpButton>)>,
    mut force_up_btn: Query<(&Interaction, &mut BackgroundColor, &mut BorderColor), (With<ForceUpButton>, Without<GravityButton>, Without<ForceDownButton>)>,
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
        force_delta += scroll.delta.y * 150.0;
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
            Interaction::Pressed => {
                // Handled below via Changed<Interaction> or mouse just_pressed?
                // Since we want single-click toggle, let's check if mouse was just pressed!
            }
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

fn step_simulation(
    state: Res<SimState>,
    time: Res<Time>,
    buttons: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    camera_q: Query<(&Camera, &GlobalTransform), Without<OffscreenCamera>>,
    mut grid: ResMut<SpatialGrid>,
    mut query: Query<(&mut Unit, &mut Transform, &mut Sprite)>,
) {
    if state.paused {
        return;
    }

    let dt = time.delta_secs().min(0.033);
    if dt <= 1e-5 {
        return;
    }

    let cursor_force = if buttons.pressed(MouseButton::Left) || buttons.pressed(MouseButton::Right) {
        let attract = buttons.pressed(MouseButton::Right);
        windows
            .single()
            .ok()
            .and_then(|w| w.cursor_position())
            .and_then(|cursor_pos| {
                camera_q
                    .single()
                    .ok()
                    .and_then(|(cam, cam_tf)| cam.viewport_to_world_2d(cam_tf, cursor_pos).ok())
            })
            .map(|world_pos| (world_pos, if attract { 1.0_f32 } else { -1.0_f32 }))
    } else {
        None
    };

    let mut data: Vec<(Vec2, Vec2)> = Vec::with_capacity(state.unit_count);
    for (unit, _, _) in &query {
        data.push((unit.pos, unit.vel));
    }

    let n = data.len();
    if n == 0 {
        return;
    }

    let min_x = -ARENA_HALF_W + UNIT_RADIUS;
    let max_x = ARENA_HALF_W - UNIT_RADIUS;
    let min_y = -ARENA_HALF_H + UNIT_RADIUS;
    let max_y = ARENA_HALF_H - UNIT_RADIUS;

    for (pos, vel) in &mut data {
        if state.gravity_on {
            vel.y -= 520.0 * dt;
        }

        if let Some((target, dir)) = cursor_force {
            let delta = target - *pos;
            let dist_sq = delta.length_squared();
            if dist_sq > 1.0 && dist_sq < 220.0 * 220.0 {
                let dist = dist_sq.sqrt();
                let strength = (1.0 - dist / 220.0) * state.force_strength * dir;
                *vel += (delta / dist) * strength * dt;
            }
        }

        let speed = vel.length();
        let base_max: f32 = if state.gravity_on { 460.0 } else { 260.0 };
        let max_speed = base_max.max(state.force_strength * 0.45);
        if speed > max_speed {
            *vel *= max_speed / speed;
        } else if !state.gravity_on && speed > 1.0 && speed < 50.0 {
            *vel *= (speed + 40.0 * dt) / speed;
        }

        *pos += *vel * dt;

        let wall_bounce = if state.gravity_on { 0.45 } else { 1.0 };
        if pos.x < min_x {
            pos.x = min_x;
            vel.x = vel.x.abs() * wall_bounce;
        } else if pos.x > max_x {
            pos.x = max_x;
            vel.x = -vel.x.abs() * wall_bounce;
        }

        if pos.y < min_y {
            pos.y = min_y;
            vel.y = vel.y.abs() * if state.gravity_on { 0.30 } else { 1.0 };
            if state.gravity_on {
                vel.x *= 0.985;
            }
        } else if pos.y > max_y {
            pos.y = max_y;
            vel.y = -vel.y.abs() * wall_bounce;
        }
    }

    grid.clear();
    for (i, (pos, _)) in data.iter().enumerate() {
        grid.insert(i, *pos);
    }

    let min_dist = UNIT_RADIUS * 2.0;
    let min_dist_sq = min_dist * min_dist;
    let cols = grid.cols as isize;
    let rows = grid.rows as isize;

    for i in 0..n {
        let pos_i = data[i].0;
        let (cx, cy) = grid.cell_coords(pos_i);

        for ny in (cy - 1).max(0)..=(cy + 1).min(rows - 1) {
            for nx in (cx - 1).max(0)..=(cx + 1).min(cols - 1) {
                let cell_idx = ny as usize * grid.cols + nx as usize;
                for &j in &grid.cells[cell_idx] {
                    if j <= i {
                        continue;
                    }
                    let delta = data[j].0 - data[i].0;
                    let dist_sq = delta.length_squared();
                    if dist_sq < min_dist_sq && dist_sq > 1e-6 {
                        let dist = dist_sq.sqrt();
                        let normal = delta / dist;
                        let overlap = 0.5 * (min_dist - dist);

                        data[i].0 -= normal * overlap;
                        data[j].0 += normal * overlap;

                        let rel_vel = data[j].1 - data[i].1;
                        let vel_along_normal = rel_vel.dot(normal);
                        if vel_along_normal < 0.0 {
                            let impulse = normal * vel_along_normal * 0.92;
                            data[i].1 += impulse;
                            data[j].1 -= impulse;
                        }
                    }
                }
            }
        }
    }

    let water_mode = state.water_mode;
    for ((mut unit, mut transform, mut sprite), (pos, vel)) in query.iter_mut().zip(data.into_iter()) {
        unit.pos = pos;
        unit.vel = vel;
        transform.translation.x = pos.x;
        transform.translation.y = pos.y;

        if !water_mode {
            let speed_t = (vel.length() / 220.0).clamp(0.0, 1.0);
            sprite.color = Color::hsl(unit.base_hue, 0.85, 0.48 + speed_t * 0.25);
        }
    }
}

fn update_water_time(
    state: Res<SimState>,
    time: Res<Time>,
    assets: Res<UnitAssets>,
    mut water_materials: ResMut<Assets<WaterMaterial>>,
) {
    if state.water_mode && !state.paused {
        if let Some(mat) = water_materials.get_mut(&assets.water_material) {
            mat.params.time = time.elapsed_secs();
        }
    }
}

fn update_hud(
    diagnostics: Res<DiagnosticsStore>,
    state: Res<SimState>,
    mut q: Query<&mut Text, With<HudText>>,
) {
    let fps = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FPS)
        .and_then(|d| d.smoothed())
        .unwrap_or(0.0);

    let mode = if state.water_mode { "WATER SHADER" } else { "PARTICLES" };
    let status = if state.paused { "PAUSED" } else { "RUNNING" };
    for mut text in &mut q {
        **text = format!(
            "Bevy 0.18 2D Crowd Demo | Mode: {mode} | Units: {} | FPS: {fps:.0} | {status}\n\
             W: Water/Particles  |  Left/Right-Drag: Repel/Attract  |  [/] or Scroll: Force  |  +/Up & -/Down: ±500  |  Space: Pause  |  R: Reset",
            state.unit_count
        );
    }
}
