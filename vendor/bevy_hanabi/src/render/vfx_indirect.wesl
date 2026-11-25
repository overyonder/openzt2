import bevy_hanabi::render::vfx_common::{
    ChildInfo, ChildInfoBuffer, DispatchIndirectArgs, EffectMetadata, SimParams, SpawnInputs, Spawner,
    DRAW_INDEXED_INDIRECT_STRIDE,
};

@group(0) @binding(0) var<uniform> sim_params : SimParams;
@group(0) @binding(1) var<storage, read_write> spawn_inputs: SpawnInputs;

@group(1) @binding(0) var<storage, read_write> effect_metadatas : array<EffectMetadata>;
// Tightly packed array of DrawIndexedIndirectArgs[], accessed as u32 array. This can contain
// some DrawIndirectArgs[] instead, but in that case the stride is adjusted so all rows have
// the same size. Since we access the instance_count, which is at the same position in both,
// we ignore their size difference (the non-indexed one is padded).
@group(1) @binding(1) var<storage, read_write> draw_indirect_buffer : array<u32>;

@group(2) @binding(0) var<storage, read_write> spawner_buffer : array<Spawner>;
@group(2) @binding(1) var<storage, read_write> prefix_sum : array<u32>;

@if(HAS_GPU_SPAWN_EVENTS)
@group(3) @binding(0) var<storage, read_write> child_info_buffer : ChildInfoBuffer;

var<workgroup> available_particles: atomic<u32>;

// All init dispatches use the same remaining budget, including GPU child and
// explicit point spawns. Count live particles before any new allocations.
@compute @workgroup_size(64)
fn reset_particle_budget(@builtin(local_invocation_index) lane: u32) {
    if lane == 0u {
        atomicStore(&available_particles, sim_params.particle_budget);
    }
    workgroupBarrier();

    for (var index = lane; index < sim_params.num_effects; index += 64u) {
        let metadata = spawner_buffer[index].effect_metadata_index;
        let alive = atomicLoad(&effect_metadatas[metadata].alive_count);
        var remaining = atomicLoad(&available_particles);
        loop {
            if alive == 0u || remaining == 0u {
                break;
            }
            let next = remaining - min(alive, remaining);
            let reserve = atomicCompareExchangeWeak(&available_particles, remaining, next);
            if reserve.exchanged {
                break;
            }
            remaining = reserve.old_value;
        }
    }
    workgroupBarrier();

    if lane == 0u {
        atomicStore(&spawn_inputs.remaining_budget, atomicLoad(&available_particles));
    }
}

/// Perform per-instance update between the init and update passes.
///
/// This is invoked once per instance, with the global invocation ID indexing the list of spawners,
/// which is re-uploaded each frame so is guaranteed to be a tightly packed array with exactly the
/// number of 
@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) global_invocation_id: vec3<u32>) {
    // Cap at maximum number of effects
    let global_effect_index = global_invocation_id.x;
    if (global_effect_index >= sim_params.num_effects) {
        return;
    }

    let spawner = &spawner_buffer[global_effect_index];
    let effect_metadata_index = (*spawner).effect_metadata_index;

    // Each active child has consumed its own event slot in the init pass.
    @if(HAS_GPU_SPAWN_EVENTS)
    {
        let slot = effect_metadatas[effect_metadata_index].global_child_index;
        if (slot < arrayLength(&child_info_buffer.rows)) {
            child_info_buffer.rows[slot].event_count = 0;
        }
    }

    // Clear the rendering instance count, which will be upgraded by the update pass
    // with the particles actually alive at the end of their update (after aged).
    let draw_indirect_index = (*spawner).draw_indirect_index;
    let dri_base = DRAW_INDEXED_INDIRECT_STRIDE * draw_indirect_index;
    draw_indirect_buffer[dri_base + 1u] = 0u;

    let capacity = effect_metadatas[effect_metadata_index].capacity;
    let alive_count = atomicLoad(&effect_metadatas[effect_metadata_index].alive_count); // atomic not needed
    let dead_count = capacity - alive_count;

    // Prepare to rebuild the prefix sum of active instances by storing in the prefix sum array
    // the actual number of alive particles for each instance. We will build the prefix sum in
    // the next pass (vfx_prefix_sum) once that's done.
    // The prefix sums and spawners are allocated in sync, so are indexed with the same index value.
    prefix_sum[global_effect_index] = alive_count;

    // Update max_update from current value of alive_count, so that the
    // update pass coming next can cap its threads to this value, while also
    // atomically modifying alive_count itself for next frame.
    effect_metadatas[effect_metadata_index].max_update = alive_count;

    // Copy the number of dead particles to a constant location, so that the
    // init pass on next frame can atomically modify alive_count in parallel
    // yet still read its initial value at the beginning of the init pass,
    // and limit the number of particles spawned to the number of dead
    // particles to recycle.
    atomicStore(&effect_metadatas[effect_metadata_index].max_spawn, dead_count);  // atomic not needed

    // Swap ping/pong buffers. The update pass always writes into ping, and both the update
    // pass and the render pass always read from pong.
    let ping = effect_metadatas[effect_metadata_index].indirect_write_index;
    let pong = 1u - ping;
    effect_metadatas[effect_metadata_index].indirect_write_index = pong;

    // Copy the new pong into the spawner buffer, which will be used during rendering
    // to determine where to read particle indices.
    (*spawner).render_indirect_read_index = pong;
}
