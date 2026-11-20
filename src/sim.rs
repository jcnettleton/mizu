use bevy::{camera::visibility::RenderLayers, prelude::*, window::PrimaryWindow};
use std::f32::consts::TAU;

use crate::water::{OffscreenCamera, UnitAssets};

pub const ARENA_HALF_W: f32 = 620.0;
pub const ARENA_HALF_H: f32 = 340.0;
pub const UNIT_RADIUS: f32 = 4.0;
pub const CELL_SIZE: f32 = 10.0;
pub const INITIAL_UNITS: usize = 4000;

#[derive(Resource)]
pub struct SimState {
    pub paused: bool,
    pub water_mode: bool,
    pub gravity_on: bool,
    pub force_strength: f32,
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
}

#[derive(Component)]
pub struct Unit {
    pub pos: Vec2,
    pub vel: Vec2,
    pub base_hue: f32,
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

pub fn step_simulation(
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

    let cursor_force = if buttons.pressed(MouseButton::Left) || buttons.pressed(MouseButton::Right)
    {
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
    for ((mut unit, mut transform, mut sprite), (pos, vel)) in
        query.iter_mut().zip(data.into_iter())
    {
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
