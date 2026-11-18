#import bevy_sprite::mesh2d_vertex_output::VertexOutput

@group(2) @binding(0) var density_texture: texture_2d<f32>;
@group(2) @binding(1) var density_sampler: sampler;

struct WaterParams {
    time: f32,
    threshold: f32,
    foam_width: f32,
    _pad: f32,
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
    let raw_depth = clamp((d - params.threshold) / (0.56 - params.threshold), 0.0, 1.0);
    let depth = pow(raw_depth, 0.85);

    // Strong contrast: light/airy cyan in sparse areas -> deep dark navy in tight clumps
    let sparse_color = vec3<f32>(0.42, 0.88, 0.98);
    let mid_color    = vec3<f32>(0.07, 0.42, 0.82);
    let dense_color  = vec3<f32>(0.01, 0.05, 0.16);

    var color = mix(sparse_color, mid_color, smoothstep(0.0, 0.45, depth));
    color = mix(color, dense_color, smoothstep(0.35, 1.0, depth));

    // Subtle surface-tension highlight right at the outer boundary
    let rim = 1.0 - smoothstep(params.threshold, params.threshold + params.foam_width, d);
    let rim_color = vec3<f32>(0.78, 0.96, 1.0);
    color = mix(color, rim_color, rim * 0.45);

    // Subtle organic shimmer
    let wave = sin(mesh.uv.x * 38.0 + params.time * 1.8)
             * cos(mesh.uv.y * 26.0 - params.time * 1.4) * 0.5 + 0.5;
    color += vec3<f32>(0.02, 0.05, 0.08) * wave * (1.0 - depth * 0.7);

    return vec4<f32>(color, body * 0.96);
}
