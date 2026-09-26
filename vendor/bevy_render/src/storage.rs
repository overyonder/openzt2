use crate::{
    GpuResourceAppExt, Render, RenderApp, RenderSystems,
    render_asset::{
        AssetExtractionError, PrepareAssetError, RenderAsset, RenderAssetPlugin, prepare_assets,
    },
    render_resource::{Buffer, BufferUsages, uniform_buffer_pages::UniformBufferPageAllocator},
    renderer::{PendingCommandBuffers, RenderDevice, RenderQueue, WgpuWrapper},
};
use bevy_app::{App, Plugin};
use bevy_asset::{Asset, AssetApp, AssetId, RenderAssetUsages};
use bevy_ecs::{
    prelude::*,
    system::{
        SystemParamItem,
        lifetimeless::{SRes, SResMut},
    },
};
use bevy_reflect::{Reflect, prelude::ReflectDefault};
use bevy_utils::default;
use encase::{ShaderType, internal::WriteInto};
use wgpu::util::BufferInitDescriptor;

/// Adds [`ShaderBuffer`] as an asset that is extracted and uploaded to the GPU.
#[derive(Default)]
pub struct StoragePlugin;

impl Plugin for StoragePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(RenderAssetPlugin::<GpuShaderBuffer>::default())
            .init_asset::<ShaderBuffer>()
            .register_asset_reflect::<ShaderBuffer>();
        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app.init_gpu_resource::<ShaderBufferUploadStaging>();
            render_app.init_gpu_resource::<UniformBufferPageAllocator>();
            render_app.add_systems(
                Render,
                queue_shader_buffer_staging_uploads
                    .in_set(RenderSystems::PrepareAssets)
                    .after(prepare_assets::<GpuShaderBuffer>),
            );
            render_app.add_systems(
                Render,
                recall_shader_buffer_staging_uploads.in_set(RenderSystems::Cleanup),
            );
        }
    }
}

/// Reusable staging storage for changes to existing shader buffers.
/// GPU resources are recreated with the render device on recovery.
#[derive(Resource)]
pub struct ShaderBufferUploadStaging(WgpuWrapper<ShaderBufferUploadStagingState>);

struct ShaderBufferUploadStagingState {
    belt: wgpu::util::StagingBelt,
    mapped_chunk: Option<MappedShaderBufferUploadChunk>,
    encoder: Option<wgpu::CommandEncoder>,
    buffer_count: u64,
    byte_count: u64,
}

struct MappedShaderBufferUploadChunk {
    buffer: wgpu::Buffer,
    mapping: wgpu::BufferViewMut,
    buffer_offset: u64,
    next_byte_offset: usize,
}

impl FromWorld for ShaderBufferUploadStaging {
    fn from_world(world: &mut World) -> Self {
        let device = world.resource::<RenderDevice>();
        Self(WgpuWrapper::new(ShaderBufferUploadStagingState {
            belt: wgpu::util::StagingBelt::new(device.wgpu_device().clone(), 1024 * 1024),
            mapped_chunk: None,
            encoder: None,
            buffer_count: 0,
            byte_count: 0,
        }))
    }
}

impl ShaderBufferUploadStaging {
    fn write_shader_buffer_change(&mut self, device: &RenderDevice, target: &Buffer, data: &[u8]) {
        let Some(size) = wgpu::BufferSize::new(data.len() as u64) else {
            return;
        };
        let state = &mut *self.0;
        assert!(data.len().is_multiple_of(wgpu::COPY_BUFFER_ALIGNMENT as usize));
        if state.mapped_chunk.as_ref().is_none_or(|chunk| {
            chunk.mapping.len() - chunk.next_byte_offset < data.len()
        }) {
            // The belt owns mapping/reuse and GPU completion. Keep one mapped
            // view per chunk rather than taking and dropping a view per asset.
            // Drop every view before finish() unmaps the belt's active chunks.
            state.mapped_chunk = None;
            let slice = state.belt.allocate(
                wgpu::BufferSize::new(size.get().max(1024 * 1024)).unwrap(),
                wgpu::BufferSize::new(wgpu::COPY_BUFFER_ALIGNMENT).unwrap(),
            );
            state.mapped_chunk = Some(MappedShaderBufferUploadChunk {
                buffer: slice.buffer().clone(),
                mapping: slice.get_mapped_range_mut(),
                buffer_offset: slice.offset(),
                next_byte_offset: 0,
            });
        }
        let encoder = state.encoder.get_or_insert_with(|| {
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("shader buffer staging uploads"),
            })
        });
        let chunk = state.mapped_chunk.as_mut().unwrap();
        let start = chunk.next_byte_offset;
        chunk.mapping.slice(start..start + data.len()).copy_from_slice(data);
        encoder.copy_buffer_to_buffer(
            &chunk.buffer,
            chunk.buffer_offset + start as u64,
            target,
            target.binding_offset(),
            size.get(),
        );
        chunk.next_byte_offset += data.len();
        state.buffer_count += 1;
        state.byte_count += size.get();
    }
}

fn queue_shader_buffer_staging_uploads(
    mut staging: ResMut<ShaderBufferUploadStaging>,
    mut pending: ResMut<PendingCommandBuffers>,
) {
    let state = &mut *staging.0;
    let Some(encoder) = state.encoder.take() else {
        return;
    };
    #[cfg(feature = "trace")]
    let _span = bevy_log::info_span!(
        "upload_shader_buffer_changes",
        buffers = state.buffer_count,
        bytes = state.byte_count
    )
    .entered();
    state.mapped_chunk = None;
    state.belt.finish();
    // PrepareAssets precedes the render graph: these copies enter its ordered
    // submission before any draw that consumes the updated uniform ranges.
    pending.push([encoder.finish()]);
    state.buffer_count = 0;
    state.byte_count = 0;
}

fn recall_shader_buffer_staging_uploads(mut staging: ResMut<ShaderBufferUploadStaging>) {
    // Cleanup follows render submission. Mapping sooner would invalidate source
    // buffers still referenced by pending copies.
    staging.0.belt.recall();
}

/// A storage buffer that is prepared as a [`RenderAsset`] and uploaded to the GPU.
#[derive(Asset, Reflect, Debug, Clone)]
#[reflect(opaque)]
#[reflect(Default, Debug, Clone)]
pub struct ShaderBuffer {
    /// Optional data used to initialize the buffer.
    pub data: Option<Vec<u8>>,
    /// The buffer description used to create the buffer.
    pub buffer_description: wgpu::BufferDescriptor<'static>,
    /// The asset usage of the storage buffer.
    pub asset_usage: RenderAssetUsages,
    /// Whether this buffer should be copied on the GPU when resized.
    pub copy_on_resize: bool,
    /// Pack this fixed-layout UNIFORM | COPY_DST asset into shared GPU pages.
    /// Bindings must use Bevy Buffer's range-aware as_entire_binding method.
    pub use_uniform_buffer_pages: bool,
}

impl Default for ShaderBuffer {
    fn default() -> Self {
        Self {
            data: None,
            buffer_description: wgpu::BufferDescriptor {
                label: None,
                size: 0,
                usage: BufferUsages::STORAGE | BufferUsages::COPY_SRC | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            },
            asset_usage: RenderAssetUsages::default(),
            copy_on_resize: false,
            use_uniform_buffer_pages: false,
        }
    }
}

impl ShaderBuffer {
    /// Creates a new storage buffer with the given data and asset usage.
    pub fn new(data: &[u8], asset_usage: RenderAssetUsages) -> Self {
        let mut storage = ShaderBuffer {
            data: Some(data.to_vec()),
            ..default()
        };
        storage.asset_usage = asset_usage;
        storage
    }

    /// Creates a new storage buffer with the given size and asset usage.
    pub fn with_size(size: usize, asset_usage: RenderAssetUsages) -> Self {
        let mut storage = ShaderBuffer {
            data: None,
            ..default()
        };
        storage.buffer_description.size = size as u64;
        storage.buffer_description.mapped_at_creation = false;
        storage.asset_usage = asset_usage;
        storage
    }

    /// Sets the data of the storage buffer to the given [`ShaderType`].
    pub fn set_data<T>(&mut self, value: T)
    where
        T: ShaderType + WriteInto,
    {
        let size = value.size().get() as usize;
        let mut wrapper = encase::StorageBuffer::<Vec<u8>>::new(Vec::with_capacity(size));
        wrapper.write(&value).unwrap();
        self.data = Some(wrapper.into_inner());
    }

    /// Resizes the buffer to the new size.
    ///
    /// If CPU data is present, it will be truncated or zero-extended.
    /// Does not preserve GPU data when the descriptor changes.
    pub fn resize(&mut self, size: u64) {
        self.buffer_description.size = size;
        if let Some(ref mut data) = self.data {
            data.resize(size as usize, 0);
        }
    }

    /// Resizes the buffer to the new size, preserving existing data.
    ///
    /// If CPU data is present, it will be truncated or zero-extended.
    /// If no CPU data is present, sets `copy_on_resize` to preserve GPU data.
    pub fn resize_in_place(&mut self, size: u64) {
        self.buffer_description.size = size;
        if let Some(ref mut data) = self.data {
            data.resize(size as usize, 0);
        } else {
            self.copy_on_resize = true;
        }
    }
}

impl<T> From<T> for ShaderBuffer
where
    T: ShaderType + WriteInto,
{
    fn from(value: T) -> Self {
        let size = value.size().get() as usize;
        let mut wrapper = encase::StorageBuffer::<Vec<u8>>::new(Vec::with_capacity(size));
        wrapper.write(&value).unwrap();
        Self::new(wrapper.as_ref(), RenderAssetUsages::default())
    }
}

/// A storage buffer that is prepared as a [`RenderAsset`] and uploaded to the GPU.
pub struct GpuShaderBuffer {
    pub buffer: Buffer,
    pub buffer_descriptor: wgpu::BufferDescriptor<'static>,
    pub had_data: bool,
}

impl RenderAsset for GpuShaderBuffer {
    type SourceAsset = ShaderBuffer;
    type Param = (
        SRes<RenderDevice>,
        SRes<RenderQueue>,
        SResMut<ShaderBufferUploadStaging>,
        SResMut<UniformBufferPageAllocator>,
    );

    fn asset_usage(source_asset: &Self::SourceAsset) -> RenderAssetUsages {
        source_asset.asset_usage
    }

    fn take_gpu_data(
        source: &mut Self::SourceAsset,
        previous_gpu_asset: Option<&Self>,
    ) -> Result<Self::SourceAsset, AssetExtractionError> {
        let data = source.data.take();

        let valid_upload = data.is_some() || previous_gpu_asset.is_none_or(|prev| !prev.had_data);

        valid_upload
            .then(|| Self::SourceAsset {
                data,
                ..source.clone()
            })
            .ok_or(AssetExtractionError::AlreadyExtracted)
    }

    fn prepare_asset(
        source_asset: Self::SourceAsset,
        _: AssetId<Self::SourceAsset>,
        (render_device, render_queue, staging, uniform_pages): &mut SystemParamItem<Self::Param>,
        previous_asset: Option<&Self>,
    ) -> Result<Self, PrepareAssetError<Self::SourceAsset>> {
        let had_data = source_asset.data.is_some();

        // when cpu data is provided, the actual buffer size is determined by the vec length,
        // not the descriptor size
        let actual_size = source_asset
            .data
            .as_ref()
            .map(|d| d.len() as u64)
            .unwrap_or(source_asset.buffer_description.size);

        if source_asset.use_uniform_buffer_pages {
            assert_eq!(
                source_asset.buffer_description.usage,
                BufferUsages::UNIFORM | BufferUsages::COPY_DST
            );
            assert!(
                !source_asset.copy_on_resize,
                "shared uniform ranges require initialized replacement data"
            );
        }

        let buffer = if let Some(prev) = previous_asset
            && prev.buffer_descriptor.size == actual_size
            && prev.buffer.is_uniform_suballocation() == source_asset.use_uniform_buffer_pages
            && prev.buffer_descriptor.usage == source_asset.buffer_description.usage
            && prev.buffer_descriptor.label == source_asset.buffer_description.label
            && (!had_data
                || source_asset
                    .buffer_description
                    .usage
                    .contains(BufferUsages::COPY_DST))
        {
            if let Some(ref data) = source_asset.data {
                staging.write_shader_buffer_change(render_device, &prev.buffer, data);
            }
            prev.buffer.clone()
        } else if source_asset.use_uniform_buffer_pages {
            let buffer = uniform_pages.allocate_uniform_buffer(render_device, actual_size);
            if let Some(ref data) = source_asset.data {
                staging.write_shader_buffer_change(render_device, &buffer, data);
            }
            buffer
        } else if let Some(ref data) = source_asset.data {
            render_device.create_buffer_with_data(&BufferInitDescriptor {
                label: source_asset.buffer_description.label,
                contents: data,
                usage: source_asset.buffer_description.usage,
            })
        } else {
            let new_buffer = render_device.create_buffer(&source_asset.buffer_description);
            if source_asset.copy_on_resize
                && let Some(previous) = previous_asset
                && previous
                    .buffer_descriptor
                    .usage
                    .contains(BufferUsages::COPY_SRC)
                && source_asset
                    .buffer_description
                    .usage
                    .contains(BufferUsages::COPY_DST)
            {
                let copy_size = source_asset
                    .buffer_description
                    .size
                    .min(previous.buffer_descriptor.size);
                let mut encoder =
                    render_device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                        label: Some("copy_buffer_on_resize"),
                    });
                encoder.copy_buffer_to_buffer(&previous.buffer, 0, &new_buffer, 0, copy_size);
                render_queue.submit([encoder.finish()]);
            }
            new_buffer
        };

        Ok(GpuShaderBuffer {
            buffer,
            buffer_descriptor: wgpu::BufferDescriptor {
                size: actual_size,
                ..source_asset.buffer_description
            },
            had_data,
        })
    }
}

#[cfg(test)]
mod shader_buffer_staging_tests {
    use super::*;
    use bevy_ecs::system::RunSystemOnce;

    #[test]
    fn small_uploads_share_a_mapping_and_release_it_before_submission() {
        let (device, queue) = crate::test_utils::create_dummy_device();
        let mut world = World::new();
        world.insert_resource(device.clone());
        world.insert_resource(queue);
        world.init_resource::<PendingCommandBuffers>();
        let mut pages = UniformBufferPageAllocator::from_world(&mut world);
        let first = pages.allocate_uniform_buffer(&device, 4);
        let second = pages.allocate_uniform_buffer(&device, 4);
        let mut staging = ShaderBufferUploadStaging::from_world(&mut world);
        staging.write_shader_buffer_change(&device, &first, &[1, 2, 3, 4]);
        let initial_buffer = staging.0.mapped_chunk.as_ref().unwrap().buffer.clone();
        staging.write_shader_buffer_change(&device, &second, &[5, 6, 7, 8]);
        let chunk = staging.0.mapped_chunk.as_ref().unwrap();
        assert_eq!(chunk.buffer, initial_buffer);
        assert_eq!(chunk.next_byte_offset, 8);
        assert_eq!(staging.0.buffer_count, 2);
        world.insert_resource(staging);
        world.run_system_once(queue_shader_buffer_staging_uploads).unwrap();
        assert_eq!(world.resource::<PendingCommandBuffers>().len(), 1);
        let buffers = world.resource_mut::<PendingCommandBuffers>().take();
        world.resource::<RenderQueue>().submit(buffers);
        world.run_system_once(recall_shader_buffer_staging_uploads).unwrap();
        let staging = world.resource::<ShaderBufferUploadStaging>();
        assert!(staging.0.mapped_chunk.is_none());
        assert!(staging.0.encoder.is_none());
        assert_eq!(staging.0.byte_count, 0);
    }

    #[test]
    fn oversized_uploads_roll_over_without_overwriting_the_previous_chunk() {
        let (device, queue) = crate::test_utils::create_dummy_device();
        let mut world = World::new();
        world.insert_resource(device.clone());
        world.insert_resource(queue);
        world.init_resource::<PendingCommandBuffers>();
        let target = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 1024 * 1024 + 8,
            usage: BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut staging = ShaderBufferUploadStaging::from_world(&mut world);
        staging.write_shader_buffer_change(&device, &target, &[1, 2, 3, 4]);
        let initial_buffer = staging.0.mapped_chunk.as_ref().unwrap().buffer.clone();
        staging.write_shader_buffer_change(&device, &target, &vec![7; 1024 * 1024 + 8]);
        let chunk = staging.0.mapped_chunk.as_ref().unwrap();
        assert_ne!(chunk.buffer, initial_buffer);
        assert_eq!(chunk.next_byte_offset, 1024 * 1024 + 8);
        assert_eq!(chunk.mapping.len(), 1024 * 1024 + 8);
        world.insert_resource(staging);
        world.run_system_once(queue_shader_buffer_staging_uploads).unwrap();
        assert_eq!(world.resource::<PendingCommandBuffers>().len(), 1);
        let buffers = world.resource_mut::<PendingCommandBuffers>().take();
        world.resource::<RenderQueue>().submit(buffers);
        world.run_system_once(recall_shader_buffer_staging_uploads).unwrap();
        assert!(world.resource::<ShaderBufferUploadStaging>().0.mapped_chunk.is_none());
    }
}
