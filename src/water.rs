use bevy::{
    asset::RenderAssetUsages,
    camera::{visibility::RenderLayers, RenderTarget},
    prelude::*,
    reflect::TypePath,
    render::render_resource::{
        AsBindGroup, Extent3d, ShaderType, TextureDimension, TextureFormat, TextureUsages,
    },
    shader::ShaderRef,
    sprite_render::{AlphaMode2d, Material2d},
};

use crate::sim::{ARENA_HALF_H, ARENA_HALF_W, SimState};

pub const RT_WIDTH: u32 = 1280;
pub const RT_HEIGHT: u32 = 720;

#[derive(Resource)]
pub struct UnitAssets {
    pub circle_texture: Handle<Image>,
    pub blob_texture: Handle<Image>,
    pub water_material: Handle<WaterMaterial>,
}

#[derive(Component)]
pub struct WaterQuad;

#[derive(Component)]
pub struct OffscreenCamera;

#[derive(ShaderType, Debug, Clone, Copy)]
pub struct WaterParams {
    pub time: f32,
    pub threshold: f32,
    pub foam_width: f32,
    pub _pad: f32,
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct WaterMaterial {
    #[texture(0)]
    #[sampler(1)]
    pub density_texture: Handle<Image>,
    #[uniform(2)]
    pub params: WaterParams,
}

impl Material2d for WaterMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/water.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }
}

pub fn create_circle_image(size: u32) -> Image {
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

pub fn create_blob_image(size: u32) -> Image {
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

pub fn setup_scene(
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
            custom_size: Some(Vec2::new(
                ARENA_HALF_W * 2.0 + 16.0,
                ARENA_HALF_H * 2.0 + 16.0,
            )),
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

    crate::sim::spawn_units(&mut commands, &assets, &mut state, crate::sim::INITIAL_UNITS);
    commands.insert_resource(assets);
}

pub fn update_water_time(
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
