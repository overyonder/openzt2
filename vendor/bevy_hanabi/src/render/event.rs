use std::{num::NonZeroU64, ops::Range};

use bevy::{
    ecs::{
        lifecycle::Remove,
        observer::On,
        query::{Changed, Or, With, Without},
        system::{Commands, Query},
    },
    log::{error, trace, warn},
    prelude::{Component, Entity, ResMut, Resource},
    render::{
        render_resource::{
            binding_types::storage_buffer, BindGroup, BindGroupEntries, BindGroupLayout,
            BindGroupLayoutEntries, Buffer, ShaderSize as _, ShaderType,
        },
        renderer::{RenderDevice, RenderQueue},
    },
};
use bytemuck::{Pod, Zeroable};
use thiserror::Error;
#[cfg(debug_assertions)]
use wgpu::util::BufferInitDescriptor;
use wgpu::{
    BufferDescriptor, BufferUsages, CommandEncoder, CommandEncoderDescriptor, ShaderStages,
};

use super::{
    aligned_buffer_vec::HybridAlignedBufferVec, effect_cache::SlabState, gpu_buffer::GpuBuffer,
    BufferBindingSource, EffectBindGroups, ExtractedEffect, GpuDispatchIndirectArgs,
};
use crate::{
    render::{effect_cache::SlabId, ChildEffectOf},
    ParticleLayout,
};

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct EventSlice {
    slice: Range<u32>,
}

/// GPU buffer storing the spawn events emitted by parent effects for their
/// children.
///
/// The event buffer contains for each effect the number of particles to spawn
/// this frame. That number is incremented by another effect when it
/// emits a spawn event, and reset to zero on next frame after the indirect init
/// pass spawned new particles, and before the new update pass of the
/// source effect optionally emits more spawn events.
///
/// GPU spawn events are never accumulated over frames; if a source emits too
/// many events and the target effect cannot spawn that many particles, for
/// example because it reached its capacity, then the extra events are
/// discarded. This is consistent with the CPU behavior of
/// [`EffectSpawner::spawn_count`].
///
/// Note that the number of allocated events in the buffer slice associated with
/// a child effect instance is not recorded here; instead it's stored in
/// [`GpuChildInfo::event_count`]. This buffer only stores the events
/// themselves.
pub struct EventBuffer {
    /// GPU buffer storing the spawn events.
    buffer: Buffer,
    /// Buffer capacity, in words (4 bytes).
    capacity: u32,
    /// Allocated (used) buffer size, in words (4 bytes).
    size: u32,
    /// Slices into the GPU buffer where event sub-allocations for each effect
    /// are located. Slices are stored ordered by location in the buffer, for
    /// convenience of allocation.
    slices: Vec<EventSlice>,
}

impl EventBuffer {
    /// Create a new event buffer to store the spawn events of the specified
    /// child effect.
    pub fn new(buffer: Buffer, capacity: u32) -> Self {
        Self {
            buffer,
            capacity,
            size: 0,
            slices: vec![],
        }
    }

    /// Get a reference to the underlying GPU buffer.
    pub fn buffer(&self) -> &Buffer {
        &self.buffer
    }

    /// Allocate a new slice for a child effect.
    pub fn allocate(&mut self, size: u32, alignment: u32) -> Option<EventSlice> {
        if size > self.capacity - self.size {
            return None;
        }

        if self.slices.is_empty() {
            let slice = EventSlice { slice: 0..size };
            self.slices.push(slice.clone());
            self.size += size;
            return Some(slice);
        }

        let mut start = 0;
        for (idx, es) in self.slices.iter().enumerate() {
            let avail_size = es.slice.start - start;
            if size > avail_size {
                start = es.slice.end.next_multiple_of(alignment);
                continue;
            }

            let slice = EventSlice {
                slice: start..start + size,
            };
            self.slices.insert(idx, slice.clone());
            self.size += size;
            return Some(slice);
        }

        if start + size <= self.capacity {
            let slice = EventSlice {
                slice: start..start + size,
            };
            self.slices.push(slice.clone());
            self.size += size;
            Some(slice)
        } else {
            None
        }
    }

    /// Free the slice of a consumer effect once that effect is deallocated.
    pub fn free(&mut self, slice: &EventSlice) -> SlabState {
        // Note: could use binary search, but likely not enough elements to be
        // worth it
        if let Some(idx) = self.slices.iter().position(|es| es == slice) {
            let removed = self.slices.remove(idx);
            self.size -= removed.slice.len() as u32;
        }
        if self.slices.is_empty() {
            SlabState::Free
        } else {
            SlabState::Used
        }
    }
}

/// Data about the child effect(s) of this effect. This component is only
/// present on an effect instance if that effect is the parent effect for at
/// least one child effect.
#[derive(Debug, PartialEq, Component)]
pub(crate) struct CachedParentInfo {
    /// Render world entities of the child effects, and their associated event
    /// buffer binding source.
    pub children: Vec<(Entity, BufferBindingSource)>,
    /// Indices in bytes into the global [`EffectCache::child_infos_buffer`] of
    /// the [`GpuChildInfo`]s for all the child effects of this parent effect.
    /// The child effects are always allocated as a single contiguous block,
    /// which needs to be mapped into a shader binding point.
    pub byte_range: Range<u32>,
}

/// Data about this effect as a child of another effect.
///
/// This component is only present on an effect instance if that effect is the
/// child effect for another effect (that is, this effect has a parent effect).
#[derive(Debug, Clone, PartialEq, Component)]
pub(crate) struct CachedChildInfo {
    /// ID of the slab storing the parent effect.
    pub parent_slab_id: SlabId,
    /// Offset into the slab of the parent's particles.
    pub parent_slab_offset: u32,
    /// Parent's particle layout.
    pub parent_particle_layout: ParticleLayout,
    /// Parent's buffer.
    pub parent_buffer_binding_source: BufferBindingSource,
    /// Index of this child effect into its parent's [`GpuChildInfo`] array.
    /// This starts at zero for the first child of each effect, and is only
    /// unique per parent only, not globally.
    pub local_child_index: u32,
    /// Global index of this child effect into the shared global
    /// [`EventCache::child_infos_buffer`] array. This is a unique index across
    /// all effects.
    pub global_child_index: u32,
    /// Index of the [`GpuDispatchIndirectArgs`] entry into the
    /// [`EventCache::init_indirect_dispatch_buffer`] array.
    pub init_indirect_dispatch_index: u32,
}

impl CachedChildInfo {
    pub fn is_locally_equal(&self, other: &CachedChildInfo) -> bool {
        self.parent_slab_id == other.parent_slab_id
        && self.parent_slab_offset == other.parent_slab_offset
        && self.parent_particle_layout == other.parent_particle_layout
        && self.parent_buffer_binding_source == other.parent_buffer_binding_source
        && self.local_child_index == other.local_child_index
        // skip global_child_index here!
        && self.init_indirect_dispatch_index == other.init_indirect_dispatch_index
    }
}

/// GPU representation of the child info data structure storing some data for a
/// child effect. The associated CPU representation is [`CachedEffectEvents`].
#[repr(C)]
#[derive(Debug, Default, Clone, Copy, Pod, Zeroable, ShaderType)]
pub struct GpuChildInfo {
    /// Index of the [`GpuDispatchIndirectArgs`] inside the
    /// [`EventCache::init_indirect_dispatch_buffer`] used to dispatch the init
    /// pass of this child effect.
    pub init_indirect_dispatch_index: u32,
    /// Number of events currently stored inside the [`EventBuffer`] slice
    /// associated with this child effect. This is updated atomically by the
    /// GPU while stored in the [`EventCache::child_infos_buffer`].
    pub event_count: i32,
}

/// Cached allocation info for the GPU events of a child effect.
///
/// This component is automitically inserted by [`allocate_events()`] when a
/// child effect uses GPU events.
#[derive(Debug, Clone, Component)]
pub struct CachedEffectEvents {
    /// Index of the [`EventBuffer`] inside the [`EventCache::buffers`]
    /// collection where a slice of events is allocated, for this effect to
    /// consume.
    pub buffer_index: u32,
    /// Range, in items (4 bytes), where the events are stored inside the
    /// [`EventBuffer`]. This determines the capacity, in event count, for this
    /// effect. The number of used events is stored on the GPU in
    /// [`GpuChildInfo::event_count`].
    pub range: Range<u32>,
    /// Index of the [`GpuDispatchIndirectArgs`] inside the
    /// [`EventCache::init_indirect_dispatch_buffer`].
    pub init_indirect_dispatch_index: u32,
}

impl CachedEffectEvents {
    /// Capacity of this allocation, in number of GPU events. The number of used
    /// events is stored on the GPU in [`GpuChildInfo::event_count`].
    #[allow(dead_code)]
    pub fn capacity(&self) -> u32 {
        self.range.len() as u32
    }
}

/// Allocate storage for GPU events for all child effects.
///
/// This system manages allocating storage for GPU events of child effects, and
/// spawning the [`CachedEffectEvents`] storing that allocation.
pub(crate) fn allocate_events(
    mut commands: Commands,
    mut event_cache: ResMut<EventCache>,
    mut effect_bind_groups: ResMut<EffectBindGroups>,
    mut child_effects: Query<
        (Entity, &ExtractedEffect, Option<&mut CachedEffectEvents>),
        (
            With<ChildEffectOf>,
            Or<(Changed<ExtractedEffect>, Without<CachedEffectEvents>)>,
        ),
    >,
    former_children: Query<Entity, (With<CachedEffectEvents>, Without<ChildEffectOf>)>,
) {
    event_cache.clear_previous_frame_resizes();

    let maximum_events =
        u32::try_from(event_cache.device.limits().max_storage_buffer_binding_size / 4)
            .unwrap_or(u32::MAX);
    for (entity, child, cached) in &mut child_effects {
        let requested = child
            .incoming_event_capacity
            .unwrap_or(child.capacity)
            .max(1);
        let capacity = requested.min(maximum_events);
        if capacity < requested {
            warn!(
                ?entity,
                requested,
                capacity,
                "Particle spawn events exceed the device storage binding limit"
            );
        }
        if let Some(mut cached) = cached {
            if cached.capacity() >= capacity {
                continue;
            }
            match event_cache.free(&cached) {
                Ok(SlabState::Used) => {}
                Ok(_) => {
                    effect_bind_groups.init_metadata_bind_groups.clear();
                    effect_bind_groups.update_metadata_bind_groups.clear();
                }
                Err(error) => error!("Failed to deallocate events of {entity}: {error:?}"),
            }
            *cached = event_cache.allocate(capacity);
        } else {
            commands
                .entity(entity)
                .insert(event_cache.allocate(capacity));
        }
    }

    for entity in &former_children {
        commands.entity(entity).remove::<CachedEffectEvents>();
    }
}

/// Observer raised when the [`CachedEffectEvents`] component is removed,
/// which indicates that the effect doesn't use GPU events anymore.
pub(crate) fn on_remove_cached_effect_events(
    trigger: On<Remove<CachedEffectEvents>>,
    query: Query<(Entity, &CachedEffectEvents)>,
    mut event_cache: ResMut<EventCache>,
    mut effect_bind_groups: ResMut<EffectBindGroups>,
) {
    #[cfg(feature = "trace")]
    let _span = bevy::log::info_span!("on_remove_cached_effect_events").entered();
    trace!("on_remove_cached_effect_events");

    if let Ok((entity, cached_effect_events)) = query.get(trigger.event().entity) {
        match event_cache.free(cached_effect_events) {
            Err(err) => {
                error!("Error while freeing cached events for effect {entity:?}: {err:?}");
            }
            Ok(buffer_state) => {
                if buffer_state != SlabState::Used {
                    // Clear bind groups associated with the old buffer
                    effect_bind_groups.init_metadata_bind_groups.clear();
                    effect_bind_groups.update_metadata_bind_groups.clear();
                }
            }
        }
    };
}

pub(crate) fn on_remove_cached_parent_info(
    trigger: On<Remove<CachedParentInfo>>,
    parents: Query<&CachedParentInfo>,
    mut cache: ResMut<EventCache>,
) {
    if let Ok(parent) = parents.get(trigger.entity) {
        cache.child_infos_buffer.remove(parent.byte_range.clone());
    }
}

/// Error code for [`EventCache::free()`].
#[derive(Debug, Error)]
pub enum CachedEventsError {
    /// The given buffer index is invalid. The [`EventCache`] doesn't contain
    /// any buffer with such index.
    #[error("Invalid buffer index #{0}.")]
    InvalidBufferIndex(u32),
    /// The given buffer index corresponds to a [`EventCache`] buffer which
    /// was already deallocated.
    #[error("Buffer at index #{0} was deallocated.")]
    BufferDeallocated(u32),
}

/// Cache for effect events.
#[derive(Resource)]
pub struct EventCache {
    /// Render device to allocate GPU resources as needed.
    device: RenderDevice,
    /// Single shared GPU buffer storing all the [`GpuChildInfo`] structs
    /// for all the parent effects.
    child_infos_buffer: HybridAlignedBufferVec,
    /// CPU-initialized records the next upload may overwrite.
    new_child_info_ranges: Vec<Range<u32>>,
    /// Old and new byte offsets of event counts for surviving children.
    moved_event_counts: Vec<(u64, u64)>,
    /// Counts saved before their old records are overwritten.
    staged_event_counts: Option<Buffer>,
    /// Collection of event buffers managed by this cache. Some buffers might
    /// be `None` if the entry is not used. Since the buffers are referenced
    /// by index, we cannot move them once they're allocated.
    buffers: Vec<Option<EventBuffer>>,
    /// Single shared GPU buffer storing all the [`GpuDispatchIndirectArgs`]
    /// structs for all the indirect init passes. Any effect allocating storage
    /// for GPU events also get an entry into this buffer, to allow consuming
    /// the events from an init pass indirectly dispatched (GPU-driven).
    // FIXME - merge with the update pass one, we don't need 2 buffers storing the same type; on
    // the other hand if we sync the allocations with GpuChildInfo we can guarantee a perfect
    // batching for the init fill dispatch pass (single dispatch for all instances at once).
    init_indirect_dispatch_buffer: GpuBuffer<GpuDispatchIndirectArgs>,
    /// Bind group layout for the indirect dispatch pass, which clears the GPU
    /// event counts ([`GpuChildInfo::event_count`]).
    indirect_child_info_buffer_bind_group_layout: BindGroupLayout,
    /// Bind group for the indirect dispatch pass, which clears the GPU event
    /// counts ([`GpuChildInfo::event_count`]).
    indirect_child_info_buffer_bind_group: Option<BindGroup>,
}

impl EventCache {
    /// Create a new event cache.
    pub fn new(device: RenderDevice) -> Self {
        let init_indirect_dispatch_buffer = GpuBuffer::new(
            BufferUsages::STORAGE | BufferUsages::INDIRECT,
            Some("hanabi:buffer:init_indirect_dispatch".to_string()),
        );

        let child_infos_bind_group_layout = device.create_bind_group_layout(
            "hanabi:bgl:indirect:child_infos@3",
            &BindGroupLayoutEntries::single(
                ShaderStages::COMPUTE,
                storage_buffer::<GpuChildInfo>(false),
            ),
        );

        Self {
            device,
            child_infos_buffer: HybridAlignedBufferVec::new(
                BufferUsages::STORAGE | BufferUsages::COPY_SRC,
                NonZeroU64::new(4).unwrap(),
                Some("hanabi:buffer:child_infos".to_string()),
            ),
            new_child_info_ranges: Vec::new(),
            moved_event_counts: Vec::new(),
            staged_event_counts: None,
            buffers: vec![],
            init_indirect_dispatch_buffer,
            indirect_child_info_buffer_bind_group_layout: child_infos_bind_group_layout,
            // Can't create until the buffer is ready
            indirect_child_info_buffer_bind_group: None,
        }
    }

    #[allow(dead_code)]
    #[inline]
    pub fn buffers(&self) -> &[Option<EventBuffer>] {
        &self.buffers
    }

    #[allow(dead_code)]
    #[inline]
    pub fn buffers_mut(&mut self) -> &mut [Option<EventBuffer>] {
        &mut self.buffers
    }

    #[inline]
    pub fn get_buffer(&self, index: u32) -> Option<&Buffer> {
        self.buffers
            .get(index as usize)
            .and_then(|opt_eb| opt_eb.as_ref().map(|eb| eb.buffer()))
    }

    #[inline]
    pub fn child_infos_buffer(&self) -> Option<&Buffer> {
        self.child_infos_buffer.buffer()
    }

    /// Allocate a memory block to store the given number of GPU events.
    ///
    /// The allocation always succeeds, allocating a new GPU event buffer if
    /// none of the existing ones can store the requested number of events.
    ///
    /// # Returns
    ///
    /// The [`CachedEffectEvents`] component representing the allocation.
    ///
    /// # Panics
    ///
    /// Panics if the number of events `num_events` is zero.
    pub fn allocate(&mut self, num_events: u32) -> CachedEffectEvents {
        assert!(num_events > 0);

        // Allocate an entry into the indirect dispatch buffer
        // The value pushed is a dummy; see allocate_frame_buffers().
        let init_indirect_dispatch_index = self.init_indirect_dispatch_buffer.allocate();
        let align = self.device.limits().min_storage_buffer_offset_alignment;
        let event_alignment = (align / 4).max(1);

        // Try to find an allocated GPU buffer with enough capacity
        let mut empty_index = None;
        for (buffer_index, buffer) in self.buffers.iter_mut().enumerate() {
            let Some(buffer) = buffer.as_mut() else {
                // Remember the first empty slot in case we need to allocate a
                // new GPU buffer
                if empty_index.is_none() {
                    empty_index = Some(buffer_index);
                }
                continue;
            };

            // Try to allocate a slice into the buffer
            if let Some(event_slice) = buffer.allocate(num_events, event_alignment) {
                trace!("Allocate new slice in event buffer #{buffer_index} for {num_events} events: range={event_slice:?}");
                return CachedEffectEvents {
                    buffer_index: buffer_index as u32,
                    init_indirect_dispatch_index,
                    range: event_slice.slice,
                };
            }
        }

        // Cannot find any suitable GPU event buffer; allocate a new one

        // Compute the slot where to store the new buffer
        let buffer_index = empty_index.unwrap_or(self.buffers.len());

        // Create the GPU bfufer
        let label = format!("hanabi:buffer:event_buffer{buffer_index}");
        let capacity = num_events.max(16 * 1024); // min capacity
        let byte_size = (capacity as u64 * 4).next_multiple_of(align as u64);
        let capacity = (byte_size / 4) as u32;
        // In debug, fill the buffer with some debug marker
        #[cfg(debug_assertions)]
        let buffer = {
            let mut contents: Vec<u32> = Vec::with_capacity(capacity as usize);
            contents.resize(capacity as usize, 0xDEADBEEF);
            self.device.create_buffer_with_data(&BufferInitDescriptor {
                label: Some(&label[..]),
                usage: BufferUsages::COPY_DST | BufferUsages::STORAGE,
                contents: bytemuck::cast_slice(contents.as_slice()),
            })
        };
        // In release, don't initialize the buffer for performance
        #[cfg(not(debug_assertions))]
        let buffer = self.device.create_buffer(&BufferDescriptor {
            label: Some(&label[..]),
            size: byte_size,
            usage: BufferUsages::COPY_DST | BufferUsages::STORAGE,
            mapped_at_creation: false,
        });
        trace!("Created new event buffer #{buffer_index} '{label}' with {byte_size} bytes ({capacity} events; align={align}B)");
        let mut buffer = EventBuffer::new(buffer, capacity);

        // Allocate a slice from the new event buffer
        let event_slice = buffer.allocate(num_events, event_alignment).expect("Failed to allocate event slice inside new buffer specifically created for this allocation.");
        trace!("Allocate new slice in event buffer #{buffer_index} for {num_events} events: range={event_slice:?}");

        // Store the event buffer at the selected slot
        if buffer_index >= self.buffers.len() {
            self.buffers.push(Some(buffer));
        } else {
            debug_assert!(self.buffers[buffer_index].is_none());
            self.buffers[buffer_index] = Some(buffer);
        }

        CachedEffectEvents {
            buffer_index: buffer_index as u32,
            init_indirect_dispatch_index,
            range: event_slice.slice,
        }
    }

    /// Deallocated and remove an event block allocation from the cache.
    pub fn free(
        &mut self,
        cached_effect_events: &CachedEffectEvents,
    ) -> Result<SlabState, CachedEventsError> {
        trace!(
            "Removing cached event {:?} from cache.",
            cached_effect_events
        );

        self.init_indirect_dispatch_buffer
            .free(cached_effect_events.init_indirect_dispatch_index);

        let entry = self
            .buffers
            .get_mut(cached_effect_events.buffer_index as usize)
            .ok_or(CachedEventsError::InvalidBufferIndex(
                cached_effect_events.buffer_index,
            ))?;
        let buffer = entry.as_mut().ok_or(CachedEventsError::BufferDeallocated(
            cached_effect_events.buffer_index,
        ))?;
        if buffer.free(&EventSlice {
            slice: cached_effect_events.range.clone(),
        }) == SlabState::Free
        {
            let buffer = entry.take().unwrap();
            buffer.buffer.destroy();
            Ok(SlabState::Free)
        } else {
            Ok(SlabState::Used)
        }
    }

    /// Allocate a new block of [`GpuChildInfo`] structures for a list of
    /// children.
    pub fn allocate_child_infos(
        &mut self,
        parent_entity: Entity,
        children: Vec<(Entity, BufferBindingSource)>,
        child_infos: &[GpuChildInfo],
    ) -> CachedParentInfo {
        assert_eq!(children.len(), child_infos.len());
        assert!(!children.is_empty());

        let byte_range = self.child_infos_buffer.push_many(child_infos);
        self.new_child_info_ranges.push(byte_range.clone());
        assert_eq!(byte_range.start as usize % size_of::<GpuChildInfo>(), 0);
        trace!(
            "Parent {:?}: newly allocated ChildInfo[] array at +{}",
            parent_entity,
            byte_range.start
        );

        CachedParentInfo {
            children,
            byte_range,
        }
    }

    /// Re-allocate a block of [`GpuChildInfo`] structures for a modified list
    /// of children.
    pub fn reallocate_child_infos(
        &mut self,
        parent_entity: Entity,
        children: Vec<(Entity, BufferBindingSource)>,
        child_infos: &[GpuChildInfo],
        cached_parent_info: &mut CachedParentInfo,
    ) {
        trace!(
            "Parent {:?}: De-allocating old ChildInfo[] entry at range {:?}",
            parent_entity,
            cached_parent_info.byte_range
        );
        self.child_infos_buffer
            .remove(cached_parent_info.byte_range.clone());

        let byte_range = self.child_infos_buffer.push_many(child_infos);
        assert_eq!(
            byte_range.start as usize % GpuChildInfo::SHADER_SIZE.get() as usize,
            0
        );
        trace!(
            "Parent {:?}: Allocated new ChildInfo[] entry at byte range {:?}",
            parent_entity,
            byte_range
        );

        let row_size = GpuChildInfo::SHADER_SIZE.get();
        let count_offset = std::mem::offset_of!(GpuChildInfo, event_count) as u64;
        for (new_index, child) in children.iter().enumerate() {
            if let Some(old_index) = cached_parent_info
                .children
                .iter()
                .position(|old| old == child)
            {
                self.moved_event_counts.push((
                    u64::from(cached_parent_info.byte_range.start)
                        + old_index as u64 * row_size
                        + count_offset,
                    u64::from(byte_range.start) + new_index as u64 * row_size + count_offset,
                ));
            }
        }
        self.new_child_info_ranges.push(byte_range.clone());
        cached_parent_info.children = children;
        cached_parent_info.byte_range = byte_range;
    }

    /// Re-/allocate any buffer for the current frame.
    pub fn prepare_buffers(
        &mut self,
        render_device: &RenderDevice,
        render_queue: &RenderQueue,
        // FIXME
        _effect_bind_groups: &mut ResMut<EffectBindGroups>,
    ) {
        // This buffer is only ever used in the bind groups of a
        // `GpuBufferOperations`, which manages its bind groups
        // automatically each frame. So there's no invalidation to do
        // here on re-allocation.
        self.init_indirect_dispatch_buffer
            .prepare_buffers(render_device);

        if let Some(previous) = self
            .child_infos_buffer
            .buffer()
            .filter(|_| !self.moved_event_counts.is_empty())
        {
            let staging = render_device.create_buffer(&BufferDescriptor {
                label: Some("hanabi:relocate_child_event_counts"),
                size: self.moved_event_counts.len() as u64 * 4,
                usage: BufferUsages::COPY_SRC | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let mut encoder = render_device.create_command_encoder(&CommandEncoderDescriptor {
                label: Some("hanabi:save_child_event_counts"),
            });
            for (index, &(old, _)) in self.moved_event_counts.iter().enumerate() {
                encoder.copy_buffer_to_buffer(previous, old, &staging, index as u64 * 4, 4);
            }
            render_queue.submit([encoder.finish()]);
            self.staged_event_counts = Some(staging);
        }
        self.child_infos_buffer.write_ranges(
            render_device,
            render_queue,
            &self.new_child_info_ranges,
        );
        self.new_child_info_ranges.clear();
    }

    /// Schedule any pending buffer copy.
    ///
    /// This is necessary when a buffer is reallocated, to copy the old content.
    /// This must be called once per frame after the buffers have been
    /// reallocated with `prepare_buffers()`.
    #[inline]
    pub fn write_buffers(&self, command_encoder: &mut CommandEncoder) {
        self.init_indirect_dispatch_buffer
            .write_buffers(command_encoder);
        if let (Some(staging), Some(child_infos)) =
            (&self.staged_event_counts, self.child_infos_buffer.buffer())
        {
            for (index, &(_, new)) in self.moved_event_counts.iter().enumerate() {
                command_encoder.copy_buffer_to_buffer(
                    staging,
                    index as u64 * 4,
                    child_infos,
                    new,
                    4,
                );
            }
        }
    }

    /// Destroy old copies of buffers reallocated last frame and copied to a new
    /// buffer.
    ///
    /// This must be called once per frame after any content was effectively
    /// copied from an old to a new buffer. This means that, due to Bevy's
    /// limitations, this must be called on the next frame, as we don't have
    /// write access to anything nor any hint as to when copies are done until
    /// the next frame rendering actually starts.
    #[inline]
    pub fn clear_previous_frame_resizes(&mut self) {
        self.init_indirect_dispatch_buffer
            .clear_previous_frame_resizes();
        self.staged_event_counts = None;
        self.moved_event_counts.clear();
    }

    #[inline]
    pub fn init_indirect_dispatch_buffer(&self) -> Option<&Buffer> {
        self.init_indirect_dispatch_buffer.buffer()
    }

    #[inline]
    pub fn child_infos(&self) -> &HybridAlignedBufferVec {
        &self.child_infos_buffer
    }

    pub fn ensure_indirect_child_info_buffer_bind_group(
        &mut self,
        device: &RenderDevice,
    ) -> Option<&BindGroup> {
        let buffer = self.child_infos_buffer()?;
        // TODO - stop re-creating each frame...
        self.indirect_child_info_buffer_bind_group = Some(device.create_bind_group(
            "hanabi:bg:indirect:child_infos@3",
            &self.indirect_child_info_buffer_bind_group_layout,
            &BindGroupEntries::single(buffer.as_entire_binding()),
        ));
        self.indirect_child_info_buffer_bind_group.as_ref()
    }

    pub fn indirect_child_info_buffer_bind_group(&self) -> Option<&BindGroup> {
        self.indirect_child_info_buffer_bind_group.as_ref()
    }
}
