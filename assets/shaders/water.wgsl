#import bevy_sprite::mesh2d_vertex_output::VertexOutput

@group(2) @binding(0) var density_texture: texture_2d<f32>;
@group(2) @binding(1) var density_sampler: sampler;

struct WaterParams {
    sparse_color: vec4<f32>,
    mid_color: vec4<f32>,
    dense_color: vec4<f32>,
    rim_color: vec4<f32>,
    time: f32,
    threshold: f32,
    foam_width: f32,
    shimmer: f32,
};

@group(2) @binding(2) var<uniform> params: WaterParams;

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    // Subtle animated surface ripple distortion driven by shimmer
    let ripple_x = sin(mesh.uv.y * 44.0 + params.time * 2.6) * cos(mesh.uv.x * 29.0 - params.time * 1.9);
    let ripple_y = cos(mesh.uv.x * 40.0 - params.time * 2.3) * sin(mesh.uv.y * 31.0 + params.time * 1.7);
    let uv = mesh.uv + vec2<f32>(ripple_x, ripple_y) * (0.0018 * params.shimmer);

    // 9-tap smoothing kernel to melt individual circles into one amalgamous liquid mass
    let step_uv = vec2<f32>(1.5 / 1280.0, 1.5 / 720.0);
    let d_c = textureSample(density_texture, density_sampler, uv).a;
    let d_r = textureSample(density_texture, density_sampler, uv + vec2<f32>( step_uv.x,  0.0)).a;
    let d_l = textureSample(density_texture, density_sampler, uv + vec2<f32>(-step_uv.x,  0.0)).a;
    let d_u = textureSample(density_texture, density_sampler, uv + vec2<f32>( 0.0,  step_uv.y)).a;
    let d_d = textureSample(density_texture, density_sampler, uv + vec2<f32>( 0.0, -step_uv.y)).a;

    var d = d_c * 0.28 + (d_r + d_l + d_u + d_d) * 0.12;
    d += textureSample(density_texture, density_sampler, uv + vec2<f32>( step_uv.x,  step_uv.y)).a * 0.06;
    d += textureSample(density_texture, density_sampler, uv + vec2<f32>(-step_uv.x,  step_uv.y)).a * 0.06;
    d += textureSample(density_texture, density_sampler, uv + vec2<f32>( step_uv.x, -step_uv.y)).a * 0.06;
    d += textureSample(density_texture, density_sampler, uv + vec2<f32>(-step_uv.x, -step_uv.y)).a * 0.06;

    let edge_aa = 0.015;
    let body = smoothstep(params.threshold - edge_aa, params.threshold + edge_aa, d);

    if body <= 0.001 {
        discard;
    }

    // Normalize density above threshold (packed non-overlapping units peak around ~0.55-0.60)
    let raw_depth = clamp((d - params.threshold) / max(0.56 - params.threshold, 0.05), 0.0, 1.0);
    let depth = pow(raw_depth, 0.80);

    var color = mix(params.sparse_color.rgb, params.mid_color.rgb, smoothstep(0.0, 0.42, depth));
    color = mix(color, params.dense_color.rgb, smoothstep(0.32, 1.0, depth));

    // Directional liquid bevel & glossy specular highlight from density slope
    let grad = vec2<f32>(d_r - d_l, d_u - d_d);
    let slope = normalize(vec3<f32>(-grad.x * 18.0, -grad.y * 18.0, 1.0));
    let light_dir = normalize(vec3<f32>(-0.55, 0.72, 0.42));
    let spec = pow(max(dot(slope, light_dir), 0.0), 16.0);
    color += params.rim_color.rgb * spec * 0.42 * (1.0 - depth * 0.35);

    // High-contrast surface-tension foam/rim right at the outer boundary
    if params.foam_width > 0.001 {
        let rim = 1.0 - smoothstep(params.threshold, params.threshold + params.foam_width, d);
        color = mix(color, params.rim_color.rgb, rim * 0.85);
    }

    // Vivid animated caustic shimmer
    let wave = sin(uv.x * 38.0 + params.time * 2.2)
             * cos(uv.y * 26.0 - params.time * 1.7) * 0.5 + 0.5;
    let caustic = pow(wave, 2.2);
    color += params.rim_color.rgb * (0.26 * params.shimmer * caustic * (1.0 - depth * 0.5));

    return vec4<f32>(color, body * 0.96);
}
