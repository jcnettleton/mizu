use bevy::{camera::visibility::RenderLayers, prelude::*, window::PrimaryWindow};
use std::f32::consts::TAU;

use crate::water::{OffscreenCamera, UnitAssets};

pub const ARENA_HALF_W: f32 = 620.0;
pub const ARENA_HALF_H: f32 = 340.0;
pub const DEFAULT_UNIT_RADIUS: f32 = 4.0;
pub const MIN_UNIT_RADIUS: f32 = 2.5;
pub const MAX_UNIT_RADIUS: f32 = 10.0;
pub const INITIAL_UNITS: usize = 4000;

#[inline]
pub fn grid_cell_size(unit_radius: f32) -> f32 {
    (unit_radius * 3.0).max(12.0)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ParticleGradientMode {
    #[default]
    Speed,
    Height,
    Density,
}

impl ParticleGradientMode {
    pub const ALL: [Self; 3] = [Self::Speed, Self::Height, Self::Density];

    pub fn name(self) -> &'static str {
        match self {
            Self::Speed => "Speed",
            Self::Height => "Height",
            Self::Density => "Density",
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ParticleColorRamp {
    #[default]
    Rainbow,
    Ocean,
    Sunset,
    Neon,
    Ember,
    Aurora,
}

impl ParticleColorRamp {
    pub const ALL: [Self; 6] = [
        Self::Rainbow,
        Self::Ocean,
        Self::Sunset,
        Self::Neon,
        Self::Ember,
        Self::Aurora,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Rainbow => "Rainbow",
            Self::Ocean => "Ocean",
            Self::Sunset => "Sunset",
            Self::Neon => "Neon",
            Self::Ember => "Ember",
            Self::Aurora => "Aurora",
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

    pub fn stops(self) -> (Vec3, Vec3, Vec3) {
        match self {
            Self::Rainbow | Self::Ocean => (
                Vec3::new(0.05, 0.16, 0.45),
                Vec3::new(0.10, 0.68, 0.90),
                Vec3::new(0.78, 0.98, 1.00),
            ),
            Self::Sunset => (
                Vec3::new(0.20, 0.06, 0.32),
                Vec3::new(0.90, 0.25, 0.48),
                Vec3::new(1.00, 0.82, 0.36),
            ),
            Self::Neon => (
                Vec3::new(0.16, 0.08, 0.66),
                Vec3::new(0.96, 0.16, 0.74),
                Vec3::new(0.22, 0.98, 0.92),
            ),
            Self::Ember => (
                Vec3::new(0.20, 0.04, 0.04),
                Vec3::new(0.96, 0.34, 0.06),
                Vec3::new(1.00, 0.92, 0.48),
            ),
            Self::Aurora => (
                Vec3::new(0.04, 0.18, 0.28),
                Vec3::new(0.12, 0.82, 0.48),
                Vec3::new(0.76, 0.44, 0.98),
            ),
        }
    }

    #[inline]
    pub fn sample(self, t: f32) -> Color {
        let t = t.clamp(0.0, 1.0);
        let (c0, c1, c2) = self.stops();
        let rgb = if t < 0.5 {
            let u = t * 2.0;
            let s = u * u * (3.0 - 2.0 * u);
            c0.lerp(c1, s)
        } else {
            let u = (t - 0.5) * 2.0;
            let s = u * u * (3.0 - 2.0 * u);
            c1.lerp(c2, s)
        };
        Color::srgb(rgb.x, rgb.y, rgb.z)
    }
}

#[derive(Resource)]
pub struct SimState {
    pub paused: bool,
    pub water_mode: bool,
    pub gravity_on: bool,
    pub force_strength: f32,
    pub unit_radius: f32,
    pub gradient_mode: ParticleGradientMode,
    pub color_ramp: ParticleColorRamp,
    pub unit_count: usize,
    pub rng_seed: u64,
}

impl Default for SimState {
    fn default() -> Self {
        Self {
            paused: false,
            water_mode: true,
            gravity_on: false,
            force_strength: 900.0,
            unit_radius: DEFAULT_UNIT_RADIUS,
            gradient_mode: ParticleGradientMode::default(),
            color_ramp: ParticleColorRamp::default(),
            unit_count: 0,
            rng_seed: 0x1234_5678_9abc_def0,
        }
    }
}

impl SimState {
    pub fn next_f32(&mut self) -> f32 {
        let mut x = self.rng_seed;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.rng_seed = x;
        ((x >> 40) as f32) / ((1u64 << 24) as f32)
    }

    #[inline]
    pub fn unit_sprite_size(&self) -> Vec2 {
        if self.water_mode {
            Vec2::splat(self.unit_radius * 9.5)
        } else {
            Vec2::splat(self.unit_radius * 2.0)
        }
    }
}

#[inline]
pub fn particle_color(
    pos: Vec2,
    vel: Vec2,
    density: f32,
    base_hue: f32,
    state: &SimState,
) -> Color {
    if state.color_ramp == ParticleColorRamp::Rainbow {
        let t = match state.gradient_mode {
            ParticleGradientMode::Speed => (vel.length() / 220.0).clamp(0.0, 1.0),
            ParticleGradientMode::Height => {
                ((pos.y + ARENA_HALF_H) / (2.0 * ARENA_HALF_H)).clamp(0.0, 1.0)
            }
            ParticleGradientMode::Density => density.clamp(0.0, 1.0),
        };
        return Color::hsl(base_hue, 0.85, 0.48 + t * 0.25);
    }

    let t = match state.gradient_mode {
        ParticleGradientMode::Speed => (vel.length() / 280.0).clamp(0.0, 1.0),
        ParticleGradientMode::Height => {
            ((pos.y + ARENA_HALF_H) / (2.0 * ARENA_HALF_H)).clamp(0.0, 1.0)
        }
        ParticleGradientMode::Density => density.clamp(0.0, 1.0),
    };
    state.color_ramp.sample(t)
}

#[derive(Component)]
pub struct Unit {
    pub pos: Vec2,
    pub vel: Vec2,
    pub base_hue: f32,
    pub density: f32,
}

#[derive(Resource)]
pub struct SpatialGrid {
    pub cols: usize,
    pub rows: usize,
    pub cell_size: f32,
    pub origin_x: f32,
    pub origin_y: f32,
    pub cells: Vec<Vec<usize>>,
}

impl SpatialGrid {
    pub fn new(half_w: f32, half_h: f32, cell_size: f32) -> Self {
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

    pub fn clear(&mut self) {
        for cell in &mut self.cells {
            cell.clear();
        }
    }

    pub fn rebuild(&mut self, half_w: f32, half_h: f32, cell_size: f32) {
        let cols = ((half_w * 2.0) / cell_size).ceil() as usize + 2;
        let rows = ((half_h * 2.0) / cell_size).ceil() as usize + 2;
        self.cols = cols;
        self.rows = rows;
        self.cell_size = cell_size;
        self.origin_x = -half_w - cell_size;
        self.origin_y = -half_h - cell_size;
        let needed = cols * rows;
        if self.cells.len() < needed {
            self.cells.resize_with(needed, || Vec::with_capacity(8));
        } else {
            self.cells.truncate(needed);
        }
    }

    #[inline]
    pub fn cell_coords(&self, pos: Vec2) -> (isize, isize) {
        let cx = ((pos.x - self.origin_x) / self.cell_size).floor() as isize;
        let cy = ((pos.y - self.origin_y) / self.cell_size).floor() as isize;
        (
            cx.clamp(0, self.cols as isize - 1),
            cy.clamp(0, self.rows as isize - 1),
        )
    }

    #[inline]
    pub fn insert(&mut self, idx: usize, pos: Vec2) {
        let (cx, cy) = self.cell_coords(pos);
        self.cells[cy as usize * self.cols + cx as usize].push(idx);
    }
}

pub fn spawn_units(
    commands: &mut Commands,
    assets: &UnitAssets,
    state: &mut SimState,
    count: usize,
) {
    let size = state.unit_sprite_size();
    let (texture, layer) = if state.water_mode {
        (assets.blob_texture.clone(), RenderLayers::layer(1))
    } else {
        (assets.circle_texture.clone(), RenderLayers::layer(0))
    };

    let radius = state.unit_radius;
    let mut batch = Vec::with_capacity(count);
    for _ in 0..count {
        let x = (state.next_f32() * 2.0 - 1.0) * (ARENA_HALF_W - radius);
        let y = (state.next_f32() * 2.0 - 1.0) * (ARENA_HALF_H - radius);
        let angle = state.next_f32() * TAU;
        let speed = 55.0 + state.next_f32() * 65.0;
        let base_hue = state.next_f32() * 360.0;
        let pos = Vec2::new(x, y);
        let vel = Vec2::new(angle.cos() * speed, angle.sin() * speed);

        let color = if state.water_mode {
            Color::WHITE
        } else {
            particle_color(pos, vel, 0.0, base_hue, state)
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
                pos,
                vel,
                base_hue,
                density: 0.0,
            },
        ));
    }

    commands.spawn_batch(batch);
    state.unit_count += count;
}

pub fn step_simulation(
    state: Res<SimState>,
    time: Res<Time>,
    buttons: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    camera_q: Query<(&Camera, &GlobalTransform), Without<OffscreenCamera>>,
    ui_interactions: Query<&Interaction>,
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

    let over_ui = ui_interactions.iter().any(|i| *i != Interaction::None);
    let cursor_force =
        if !over_ui && (buttons.pressed(MouseButton::Left) || buttons.pressed(MouseButton::Right)) {
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

    let radius = state.unit_radius;
    let min_x = -ARENA_HALF_W + radius;
    let max_x = ARENA_HALF_W - radius;
    let min_y = -ARENA_HALF_H + radius;
    let max_y = ARENA_HALF_H - radius;

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

    let target_cell_size = grid_cell_size(radius);
    if (grid.cell_size - target_cell_size).abs() > 0.01 {
        grid.rebuild(ARENA_HALF_W, ARENA_HALF_H, target_cell_size);
    }

    grid.clear();
    for (i, (pos, _)) in data.iter().enumerate() {
        grid.insert(i, *pos);
    }

    let min_dist = radius * 2.0;
    let min_dist_sq = min_dist * min_dist;
    let track_density = !state.water_mode && state.gradient_mode == ParticleGradientMode::Density;
    let density_r_sq = grid.cell_size * grid.cell_size;
    let mut neighbor_counts = if track_density {
        vec![0u16; n]
    } else {
        Vec::new()
    };

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
                    if track_density && dist_sq < density_r_sq {
                        neighbor_counts[i] = neighbor_counts[i].saturating_add(1);
                        neighbor_counts[j] = neighbor_counts[j].saturating_add(1);
                    }
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
    for (idx, ((mut unit, mut transform, mut sprite), (pos, vel))) in
        query.iter_mut().zip(data).enumerate()
    {
        unit.pos = pos;
        unit.vel = vel;
        if track_density {
            unit.density = (neighbor_counts[idx] as f32 / 8.0).clamp(0.0, 1.0);
        }
        transform.translation.x = pos.x;
        transform.translation.y = pos.y;

        if !water_mode {
            sprite.color = particle_color(pos, vel, unit.density, unit.base_hue, &state);
        }
    }
}
