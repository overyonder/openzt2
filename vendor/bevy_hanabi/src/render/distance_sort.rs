//! Per-view sorting of live particle indices without modifying simulation data.

use bevy::asset::uuid_handle;

use super::*;

pub(crate) const DISTANCE_SORT_SHADER: Handle<Shader> =
    uuid_handle!("9e634cc9-7e55-4d0e-b199-05bc11606e47");

pub(super) fn render_layout() -> BindGroupLayoutDescriptor {
    BindGroupLayoutDescriptor::new(
        "hanabi:bgl:distance_sort:draw",
        &BindGroupLayoutEntries::single(
            ShaderStages::VERTEX,
            storage_buffer_read_only::<UVec2>(false),
        ),
    )
}

#[derive(Resource)]
pub(crate) struct ParticleDistanceSortPipeline {
    fill: CachedComputePipelineId,
    sort: CachedComputePipelineId,
    layout: BindGroupLayoutDescriptor,
    render_layout: BindGroupLayout,
}

impl FromWorld for ParticleDistanceSortPipeline {
    fn from_world(world: &mut World) -> Self {
        let layout = BindGroupLayoutDescriptor::new(
            "hanabi:bgl:distance_sort:compute",
            &BindGroupLayoutEntries::sequential(
                ShaderStages::COMPUTE,
                (
                    storage_buffer_read_only_sized(false, None),
                    storage_buffer_read_only_sized(false, None),
                    storage_buffer_read_only_sized(false, None),
                    storage_buffer_sized(false, None),
                    uniform_buffer::<DistanceSortParameters>(true),
                ),
            ),
        );
        let pipelines = world.resource::<PipelineCache>();
        let queue = |entry: &'static str| {
            pipelines.queue_compute_pipeline(ComputePipelineDescriptor {
                label: Some(entry.into()),
                layout: vec![layout.clone()],
                shader: DISTANCE_SORT_SHADER,
                entry_point: Some(entry.into()),
                ..default()
            })
        };
        Self {
            fill: queue("fill_keys"),
            sort: queue("sort_keys"),
            layout,
            render_layout: pipelines.get_bind_group_layout(&render_layout()),
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, ShaderType)]
struct DistanceSortParameters {
    world_from_particle: Mat4,
    camera_position: Vec4,
    particle_offsets: UVec4,
    sort_offsets: UVec4,
}

pub(super) struct ViewParticleSort {
    indices: Buffer,
    pub(super) render_binding: BindGroup,
    slab: SlabId,
    metadata: BufferTableId,
    parameters: DistanceSortParameters,
}

impl ViewParticleSort {
    pub(super) fn new(
        device: &RenderDevice,
        pipeline: &ParticleDistanceSortPipeline,
        view: &ExtractedView,
        batch: &EffectBatch,
        draw: &EffectDrawBatch,
        previous: Option<Self>,
    ) -> Option<Self> {
        let state = batch.effect_data.first()?;
        let capacity = batch.capacity.checked_next_power_of_two()?;
        let position = batch.particle_layout.byte_offset(Attribute::POSITION)? / 4;
        let size = u64::from(capacity) * 8;
        if size > u64::from(device.limits().max_storage_buffer_binding_size) {
            return None;
        }
        let (indices, render_binding) =
            if let Some(previous) = previous.filter(|previous| previous.indices.size() == size) {
                (previous.indices, previous.render_binding)
            } else {
                let indices = device.create_buffer(&BufferDescriptor {
                    label: Some("hanabi:distance_sort:indices"),
                    size,
                    usage: BufferUsages::STORAGE,
                    mapped_at_creation: false,
                });
                let binding = device.create_bind_group(
                    "hanabi:bg:distance_sort:draw",
                    &pipeline.render_layout,
                    &BindGroupEntries::single(indices.as_entire_binding()),
                );
                (indices, binding)
            };
        let world_from_particle = if batch
            .layout_flags
            .contains(LayoutFlags::LOCAL_SPACE_SIMULATION)
        {
            draw.transform.to_matrix()
        } else {
            Mat4::IDENTITY
        };
        Some(Self {
            indices,
            render_binding,
            slab: batch.slab_id,
            metadata: state.metadata_table_id,
            parameters: DistanceSortParameters {
                world_from_particle,
                camera_position: view.world_from_view.translation().extend(0.0),
                particle_offsets: UVec4::new(
                    state.slab_offset,
                    batch.particle_layout.min_binding_size32().get() / 4,
                    position,
                    0,
                ),
                sort_offsets: UVec4::new(0, capacity, 0, 0),
            },
        })
    }
}

pub(super) fn sort_particles_for_views(
    mut context: RenderContext,
    device: Res<RenderDevice>,
    pipeline_cache: Res<PipelineCache>,
    pipeline: Res<ParticleDistanceSortPipeline>,
    draws: Res<AuthoredParticleDrawQueue>,
    effect_cache: Res<EffectCache>,
    effects: Res<EffectsMeta>,
) {
    let Some(fill) = pipeline_cache.get_compute_pipeline(pipeline.fill) else {
        return;
    };
    let Some(sort) = pipeline_cache.get_compute_pipeline(pipeline.sort) else {
        return;
    };
    let Some(metadata) = effects.effect_metadata_buffer.buffer() else {
        return;
    };
    let layout = pipeline_cache.get_bind_group_layout(&pipeline.layout);
    let alignment = device.limits().min_uniform_buffer_offset_alignment as usize;
    let stride = std::mem::size_of::<DistanceSortParameters>().next_multiple_of(alignment);
    for draw in draws.views.values().filter_map(|draw| draw.sort.as_ref()) {
        let Some(slab) = effect_cache.get_slab(&draw.slab) else {
            continue;
        };
        let mut params = draw.parameters;
        let metadata_word =
            draw.metadata.0 * (effects.effect_metadata_buffer.aligned_size() as u32 / 4);
        params.particle_offsets[3] =
            metadata_word + (std::mem::offset_of!(GpuEffectMetadata, alive_count) as u32 / 4);
        params.sort_offsets[0] = metadata_word
            + (std::mem::offset_of!(GpuEffectMetadata, indirect_write_index) as u32 / 4);

        let mut data = Vec::new();
        let mut append_parameters = |params: &DistanceSortParameters| {
            let offset = data.len();
            data.resize(offset + stride, 0);
            let bytes = bytemuck::bytes_of(params);
            data[offset..offset + bytes.len()].copy_from_slice(bytes);
        };
        append_parameters(&params);
        let capacity = params.sort_offsets[1];
        for stage in 1..=capacity.ilog2() {
            params.sort_offsets[2] = 1 << stage;
            for step in (0..stage).rev() {
                params.sort_offsets[3] = 1 << step;
                append_parameters(&params);
            }
        }
        let uniforms = device.create_buffer_with_data(&BufferInitDescriptor {
            label: Some("hanabi:distance_sort:parameters"),
            contents: &data,
            usage: BufferUsages::UNIFORM,
        });
        let bindings = device.create_bind_group(
            "hanabi:bg:distance_sort:compute",
            &layout,
            &BindGroupEntries::sequential((
                slab.as_entire_binding_particle(),
                slab.as_entire_binding_indirect(),
                metadata.as_entire_binding(),
                draw.indices.as_entire_binding(),
                BufferBinding {
                    buffer: &uniforms,
                    offset: 0,
                    size: Some(DistanceSortParameters::min_size()),
                },
            )),
        );
        let mut pass = context
            .command_encoder()
            .begin_compute_pass(&ComputePassDescriptor {
                label: Some("hanabi:distance_sort"),
                ..default()
            });
        let workgroups = capacity.div_ceil(64);
        let workgroups_x = workgroups.min(device.limits().max_compute_workgroups_per_dimension);
        let workgroups_y = workgroups.div_ceil(workgroups_x);
        pass.set_pipeline(fill);
        pass.set_bind_group(0, &bindings, &[0]);
        pass.dispatch_workgroups(workgroups_x, workgroups_y, 1);
        pass.set_pipeline(sort);
        for offset in (stride..data.len()).step_by(stride) {
            pass.set_bind_group(0, &bindings, &[offset as u32]);
            pass.dispatch_workgroups(workgroups_x, workgroups_y, 1);
        }
    }
}

impl ParticleDistanceSortPipeline {
    pub(super) fn ready(&self, cache: &PipelineCache) -> bool {
        cache.get_compute_pipeline(self.fill).is_some()
            && cache.get_compute_pipeline(self.sort).is_some()
    }
}
