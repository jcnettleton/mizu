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
    // 9-tap smoothing kernel to melt individual circles into one amalgamous mass
    let step_uv = vec2<f32>(1.5 / 1280.0, 1.5 / 720.0);
    var d = textureSample(density_texture, density_sampler, mesh.uv).a * 0.28;
    d += textureSample(density_texture, density_sampler, mesh.uv + vec2<f32>( step_uv.x,  0.0)).a * 0.12;
    d += textureSample(density_texture, density_sampler, mesh.uv + vec2<f32>(-step_uv.x,  0.0)).a * 0.12;
    d += textureSample(density_texture, density_sampler, mesh.uv + vec2<f32>( 0.0,  step_uv.y)).a * 0.12;
    d += textureSample(density_texture, density_sampler, mesh.uv + vec2<f32>( 0.0, -step_uv.y)).a * 0.12;
    d += textureSample(density_texture, density_sampler, mesh.uv + vec2<f32>( step_uv.x,  step_uv.y)).a * 0.06;
    d += textureSample(density_texture, density_sampler, mesh.uv + vec2<f32>(-step_uv.x,  step_uv.y)).a * 0.06;
    d += textureSample(density_texture, density_sampler, mesh.uv + vec2<f32>( step_uv.x, -step_uv.y)).a * 0.06;
    d += textureSample(density_texture, density_sampler, mesh.uv + vec2<f32>(-step_uv.x, -step_uv.y)).a * 0.06;

    let edge_aa = 0.025;
    let body = smoothstep(params.threshold - edge_aa, params.threshold + edge_aa, d);

    if body <= 0.001 {
        discard;
    }

    // Normalize density above threshold (packed non-overlapping units peak around ~0.55-0.60)
    let raw_depth = clamp((d - params.threshold) / max(0.56 - params.threshold, 0.05), 0.0, 1.0);
    let depth = pow(raw_depth, 0.85);

    var color = mix(params.sparse_color.rgb, params.mid_color.rgb, smoothstep(0.0, 0.45, depth));
    color = mix(color, params.dense_color.rgb, smoothstep(0.35, 1.0, depth));

    // Subtle surface-tension highlight right at the outer boundary
    if params.foam_width > 0.001 {
        let rim = 1.0 - smoothstep(params.threshold, params.threshold + params.foam_width, d);
        color = mix(color, params.rim_color.rgb, rim * 0.45);
    }

    // Subtle organic shimmer
    let wave = sin(mesh.uv.x * 38.0 + params.time * 1.8)
             * cos(mesh.uv.y * 26.0 - params.time * 1.4) * 0.5 + 0.5;
    color += vec3<f32>(0.02, 0.05, 0.08) * params.shimmer * wave * (1.0 - depth * 0.7);

    return vec4<f32>(color, body * 0.96);
}
