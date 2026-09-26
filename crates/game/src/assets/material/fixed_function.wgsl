#define_import_path openzt2::fixed_function

#import bevy_pbr::{
    mesh_functions::{get_world_from_local, mesh_normal_local_to_world, mesh_position_local_to_clip}
}
#import bevy_render::color_operations::srgb_to_linear
#ifdef SKINNED
#import bevy_pbr::skinning::{skin_model, skin_normals}
#endif

struct VertexInput {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) primary_texture_coordinates: vec2<f32>,
    @location(3) secondary_texture_coordinates: vec2<f32>,
    @location(5) color: vec4<f32>,
#ifdef SKINNED
    @location(6) joint_indices: vec4<u32>,
    @location(7) joint_weights: vec4<f32>,
#endif
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) primary_texture_coordinates: vec2<f32>,
    @location(1) secondary_texture_coordinates: vec2<f32>,
    @location(2) normal: vec3<f32>,
    @location(3) color: vec4<f32>,
    @location(4) lit_diffuse: vec4<f32>,
};

struct FixedFunctionFragmentInputs {
    texture_coordinates: array<vec4<f32>, 8>,
    normal: vec3<f32>,
    diffuse: vec4<f32>,
};

@vertex
fn vertex(input: VertexInput) -> VertexOutput {
#ifdef SKINNED
    let world_from_local = skin_model(input.joint_indices, input.joint_weights, input.instance_index);
#else
    let world_from_local = get_world_from_local(input.instance_index);
#endif
    var output: VertexOutput;
    output.position = mesh_position_local_to_clip(world_from_local, vec4(input.position, 1.0));
    output.primary_texture_coordinates = input.primary_texture_coordinates;
    output.secondary_texture_coordinates = input.secondary_texture_coordinates;
#ifdef SKINNED
    output.normal = skin_normals(world_from_local, input.normal);
#else
    output.normal = mesh_normal_local_to_world(input.normal, input.instance_index);
#endif
    output.color = input.color;
    output.lit_diffuse = fixed_function_lit_diffuse(output.normal, output.color);
    return output;
}

@group(#{MATERIAL_BIND_GROUP}) @binding(2) var texture0: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var texture1: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(4) var texture2: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(5) var texture3: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(6) var texture4: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(7) var texture5: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(8) var texture6: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(9) var texture7: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(10) var sampler0: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(11) var sampler1: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(12) var sampler2: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(13) var sampler3: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(14) var sampler4: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(15) var sampler5: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(16) var sampler6: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(17) var sampler7: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(38) var cube_texture0: texture_cube<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(39) var cube_texture1: texture_cube<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(40) var cube_texture2: texture_cube<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(41) var cube_texture3: texture_cube<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(42) var cube_texture4: texture_cube<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(43) var cube_texture5: texture_cube<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(44) var cube_texture6: texture_cube<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(45) var cube_texture7: texture_cube<f32>;
// Bit N is set when stage N has a cube texture bound.
@group(#{MATERIAL_BIND_GROUP}) @binding(46) var<uniform> cube_texture_stages: vec4<u32>;

struct FixedFunctionState { values: array<vec4<f32>, 48> };
struct FixedTransforms { values: array<mat4x4<f32>, 8> };
struct FixedFunctionWorldLights {
    ambient_color_and_directional_light_count: vec4<f32>,
    directional_light_directions: array<vec4<f32>, 8>,
    directional_light_colors: array<vec4<f32>, 8>,
};
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var<uniform> fixed: FixedFunctionState;
@group(#{MATERIAL_BIND_GROUP}) @binding(36) var<uniform> transforms: FixedTransforms;
@group(#{MATERIAL_BIND_GROUP}) @binding(37) var<storage, read> world_light_contexts: array<FixedFunctionWorldLights>;

const D3D9_DIRECTIONAL_LIGHT_REFERENCE_ILLUMINANCE: f32 = 100000.0;

fn state(stage: u32, offset: u32) -> u32 {
    // MojoShader exposes the 18 compact FX texture-stage states in source order.
    let component = offset % 4u;
    return u32(fixed.values[stage * 5u + offset / 4u][component]);
}

fn coordinates(stage: u32, input: FixedFunctionFragmentInputs, programmable_vertex: bool) -> vec4<f32> {
    let index = state(stage, 13u);
    var value = input.texture_coordinates[index & 0xffffu];
    // A programmable vertex shader has already produced its texture outputs.
    // Fixed-function coordinate generation and transforms do not run again.
    if programmable_vertex { return value; }
    switch index & 0xffff0000u {
        case 0x00010000u: { value = vec4(input.normal, 1.0); }
        case 0x00030000u: { value = vec4(reflect(vec3(0.0, 0.0, -1.0), input.normal), 1.0); }
        default: {}
    }
    let flags = state(stage, 16u);
    if ((flags & 0xffu) != 0u) { value = transforms.values[stage] * value; }
    if ((flags & 0x100u) != 0u && value.w != 0.0) { value /= value.w; }
    return value;
}

fn sample_stage(stage: u32, stage_coordinates: vec4<f32>) -> vec4<f32> {
    // D3D9 samples a stage by the type of the bound texture: cube textures
    // use three coordinate components and 2D textures use two.
    if ((cube_texture_stages.x & (1u << stage)) != 0u) {
        let direction = stage_coordinates.xyz;
        switch stage {
            case 0u: { return textureSample(cube_texture0, sampler0, direction); }
            case 1u: { return textureSample(cube_texture1, sampler1, direction); }
            case 2u: { return textureSample(cube_texture2, sampler2, direction); }
            case 3u: { return textureSample(cube_texture3, sampler3, direction); }
            case 4u: { return textureSample(cube_texture4, sampler4, direction); }
            case 5u: { return textureSample(cube_texture5, sampler5, direction); }
            case 6u: { return textureSample(cube_texture6, sampler6, direction); }
            default: { return textureSample(cube_texture7, sampler7, direction); }
        }
    }
    let uv = stage_coordinates.xy;
    switch stage {
        case 0u: { return textureSample(texture0, sampler0, uv); }
        case 1u: { return textureSample(texture1, sampler1, uv); }
        case 2u: { return textureSample(texture2, sampler2, uv); }
        case 3u: { return textureSample(texture3, sampler3, uv); }
        case 4u: { return textureSample(texture4, sampler4, uv); }
        case 5u: { return textureSample(texture5, sampler5, uv); }
        case 6u: { return textureSample(texture6, sampler6, uv); }
        default: { return textureSample(texture7, sampler7, uv); }
    }
}

fn argument(encoded: u32, texture: vec4<f32>, diffuse: vec4<f32>, current: vec4<f32>) -> vec4<f32> {
    var value = vec4(0.0);
    switch encoded & 0xfu {
        case 0u: { value = diffuse; }
        case 1u: { value = current; }
        case 2u: { value = texture; }
        case 3u: { value = vec4(1.0); }
        case 4u: { value = vec4(0.0); }
        default: {}
    }
    if ((encoded & 0x10u) != 0u) { value = vec4(1.0) - value; }
    if ((encoded & 0x20u) != 0u) { value = vec4(value.a); }
    return value;
}

fn operation(kind: u32, first: vec4<f32>, second: vec4<f32>, texture: vec4<f32>, current: vec4<f32>) -> vec4<f32> {
    switch kind {
        case 1u: { return vec4(0.0); }
        case 2u: { return first; }
        case 3u: { return second; }
        case 4u: { return first * second; }
        case 5u: { return first * second * 2.0; }
        case 6u: { return first * second * 4.0; }
        case 7u: { return first + second; }
        case 8u: { return first + second - vec4(0.5); }
        case 9u: { return (first + second - vec4(0.5)) * 2.0; }
        case 10u: { return first - second; }
        case 13u: { return mix(second, first, texture.a); }
        case 14u: { return mix(second, first, first.a); }
        case 15u: { return mix(second, first, texture.r); }
        case 16u: { return mix(second, first, texture.g); }
        case 17u: { return mix(second, first, texture.b); }
        case 18u: { return mix(second, first, texture.a); }
        case 24u: { return first * second + current; }
        case 25u: { return first * current + second; }
        case 26u: { return first * second + vec4(1.0) - current; }
        default: { return vec4(0.0); }
    }
}

fn compare(kind: u32, first: f32, second: f32) -> bool {
    switch kind {
        case 1u: { return false; }
        case 2u: { return first < second; }
        case 3u: { return first == second; }
        case 4u: { return first <= second; }
        case 5u: { return first > second; }
        case 6u: { return first != second; }
        case 7u: { return first >= second; }
        default: { return true; }
    }
}

fn fixed_function_material_source(source: u32, material: vec4<f32>, vertex_color: vec4<f32>) -> vec4<f32> {
    switch source {
        case 0u: { return material; }
        // The shipped BFB and NIF layouts provide D3DCOLOR0 but no
        // D3DCOLOR1. COLOR2 therefore has the D3D default of white.
        case 1u: { return vertex_color; }
        default: { return vec4(1.0); }
    }
}

fn fixed_function_lit_diffuse(normal: vec3<f32>, vertex_color: vec4<f32>) -> vec4<f32> {
    let lighting = fixed.values[46];
    if (u32(lighting.x) == 0u) {
        return vertex_color;
    }

    let diffuse = fixed_function_material_source(u32(lighting.y), fixed.values[40], vertex_color);
    let ambient = fixed_function_material_source(u32(lighting.z), fixed.values[41], vertex_color);
    let emissive = fixed_function_material_source(u32(lighting.w), fixed.values[43], vertex_color);
    let world_lights = world_light_contexts[u32(fixed.values[47].x)];
    let world_normal = normalize(normal);
    var color = emissive.rgb
        + ambient.rgb * world_lights.ambient_color_and_directional_light_count.rgb;
    let directional_light_count =
        u32(world_lights.ambient_color_and_directional_light_count.w);
    for (var index = 0u; index < directional_light_count; index += 1u) {
        color += diffuse.rgb
            * (world_lights.directional_light_colors[index].rgb
                / D3D9_DIRECTIONAL_LIGHT_REFERENCE_ILLUMINANCE)
            * max(dot(world_normal, world_lights.directional_light_directions[index].xyz), 0.0);
    }
    return vec4(clamp(color, vec3(0.0), vec3(1.0)), diffuse.a);
}

@fragment
fn fragment(input: VertexOutput) -> @location(0) vec4<f32> {
    var fragment_inputs: FixedFunctionFragmentInputs;
    fragment_inputs.texture_coordinates[0] = vec4(input.primary_texture_coordinates, 0.0, 1.0);
    fragment_inputs.texture_coordinates[1] = vec4(input.secondary_texture_coordinates, 0.0, 1.0);
    fragment_inputs.normal = input.normal;
    fragment_inputs.diffuse = input.lit_diffuse;
    return evaluate_fixed_function_texture_stages(fragment_inputs, false);
}

fn evaluate_fixed_function_texture_stages(input: FixedFunctionFragmentInputs, programmable_vertex: bool) -> vec4<f32> {
    let diffuse = input.diffuse;
    var current = diffuse;
    for (var stage = 0u; stage < 8u; stage++) {
        let color_op = state(stage, 0u);
        if (color_op == 1u) { break; }
        let texture = sample_stage(stage, coordinates(stage, input, programmable_vertex));
        let first = argument(state(stage, 1u), texture, diffuse, current);
        let second = argument(state(stage, 2u), texture, diffuse, current);
        var result = operation(color_op, first, second, texture, current);
        // RGB combiners must not overwrite opacity. The shipped BaseGlow
        // effect disables later alpha stages to retain the base texture alpha.
        result.a = current.a;
        let alpha_op = state(stage, 3u);
        if (alpha_op != 1u) {
            let alpha_first = argument(state(stage, 4u), texture, diffuse, current);
            let alpha_second = argument(state(stage, 5u), texture, diffuse, current);
            result.a = operation(alpha_op, alpha_first, alpha_second, texture, current).a;
        }
        current = clamp(result, vec4(0.0), vec4(1.0));
    }
    let alpha_test = fixed.values[45];
    if (u32(alpha_test.x) != 0u && !compare(u32(alpha_test.z), current.a, alpha_test.y / 255.0)) {
        discard;
    }
    // D3D9 fixed-function stages operated directly on the authored colour
    // bytes and wrote that result to the display. Bevy's render target expects
    // linear fragment output, so convert the completed legacy colour once;
    // decoding individual texture stages would change multi-texture math such
    // as Gamebryo detail-map MODULATE2X.
    return vec4(srgb_to_linear(current.rgb), current.a);
}
