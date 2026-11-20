struct SortParameters {
    world_from_particle: mat4x4<f32>,
    camera_position: vec4<f32>,
    particle_offsets: vec4<u32>,
    sort_offsets: vec4<u32>,
}

@group(0) @binding(0) var<storage, read> particles: array<u32>;
@group(0) @binding(1) var<storage, read> particle_indices: array<u32>;
@group(0) @binding(2) var<storage, read> metadata: array<u32>;
@group(0) @binding(3) var<storage, read_write> sorted_particles: array<vec2<u32>>;
@group(0) @binding(4) var<uniform> params: SortParameters;

@compute @workgroup_size(64)
fn fill_keys(
    @builtin(global_invocation_id) id: vec3<u32>,
    @builtin(num_workgroups) workgroups: vec3<u32>,
) {
    let slot = id.x + id.y * workgroups.x * 64u;
    if slot >= params.sort_offsets.y {
        return;
    }
    if slot >= metadata[params.particle_offsets.w] {
        sorted_particles[slot] = vec2(0u, 0xFFFFFFFFu);
        return;
    }
    let base = params.particle_offsets.x;
    let ping = metadata[params.sort_offsets.x];
    let index = particle_indices[(base + slot) * 3u + ping];
    let offset = (base + index) * params.particle_offsets.y + params.particle_offsets.z;
    let position = bitcast<vec3<f32>>(vec3(particles[offset], particles[offset + 1u], particles[offset + 2u]));
    let difference = (params.world_from_particle * vec4(position, 1.0)).xyz - params.camera_position.xyz;
    sorted_particles[slot] = vec2(bitcast<u32>(dot(difference, difference)), index);
}

@compute @workgroup_size(64)
fn sort_keys(
    @builtin(global_invocation_id) id: vec3<u32>,
    @builtin(num_workgroups) workgroups: vec3<u32>,
) {
    let slot = id.x + id.y * workgroups.x * 64u;
    let other = slot ^ params.sort_offsets.w;
    if slot >= params.sort_offsets.y || other <= slot {
        return;
    }
    let left = sorted_particles[slot];
    let right = sorted_particles[other];
    let right_is_farther = right.x > left.x || (right.x == left.x && right.y < left.y);
    let descending = (slot & params.sort_offsets.z) == 0u;
    if descending == right_is_farther {
        sorted_particles[slot] = right;
        sorted_particles[other] = left;
    }
}
