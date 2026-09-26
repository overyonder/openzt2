use super::uniform_buffer_pages::UniformBufferPageAllocation;
use crate::renderer::WgpuWrapper;
use bevy_utils::define_atomic_id;
use core::ops::{Deref, RangeBounds};
use std::sync::Arc;

define_atomic_id!(BufferId);

#[derive(Clone, Debug)]
pub struct Buffer {
    id: BufferId,
    value: WgpuWrapper<wgpu::Buffer>,
    uniform_allocation: Option<Arc<UniformBufferPageAllocation>>,
}

impl Buffer {
    pub(crate) fn with_uniform_allocation(
        mut self,
        allocation: Arc<UniformBufferPageAllocation>,
    ) -> Self {
        // Logical ranges must remain distinct in Bevy's binding caches even
        // though wgpu tracks a single underlying GPU buffer for the page.
        self.id = BufferId::new();
        self.uniform_allocation = Some(allocation);
        self
    }

    pub(crate) fn binding_offset(&self) -> u64 {
        self.uniform_allocation
            .as_ref()
            .map_or(0, |allocation| allocation.byte_offset)
    }

    pub(crate) fn is_uniform_suballocation(&self) -> bool {
        self.uniform_allocation.is_some()
    }

    /// Returns the size of this logical buffer, excluding neighboring ranges.
    pub fn size(&self) -> u64 {
        self.uniform_allocation.as_ref().map_or_else(
            || self.value.size(),
            |allocation| allocation.byte_size.get(),
        )
    }

    /// Binds this buffer's range while its owner retains the allocation.
    pub fn as_entire_binding(&self) -> wgpu::BindingResource<'_> {
        wgpu::BindingResource::Buffer(self.as_entire_buffer_binding())
    }

    /// Returns a binding that excludes neighboring allocations in the page.
    pub fn as_entire_buffer_binding(&self) -> wgpu::BufferBinding<'_> {
        wgpu::BufferBinding {
            buffer: &self.value,
            offset: self.binding_offset(),
            size: self
                .uniform_allocation
                .as_ref()
                .map(|allocation| allocation.byte_size),
        }
    }

    #[inline]
    pub fn id(&self) -> BufferId {
        self.id
    }

    pub fn slice(&self, bounds: impl RangeBounds<wgpu::BufferAddress>) -> BufferSlice<'_> {
        let Some(allocation) = &self.uniform_allocation else {
            return BufferSlice {
                id: self.id,
                value: self.value.slice(bounds),
            };
        };
        use core::ops::Bound;
        let start = match bounds.start_bound() {
            Bound::Included(&value) => value,
            Bound::Excluded(&value) => value.checked_add(1).expect("buffer slice start overflow"),
            Bound::Unbounded => 0,
        };
        let end = match bounds.end_bound() {
            Bound::Included(&value) => value.checked_add(1).expect("buffer slice end overflow"),
            Bound::Excluded(&value) => value,
            Bound::Unbounded => allocation.byte_size.get(),
        };
        assert!(
            start <= end && end <= allocation.byte_size.get(),
            "buffer slice out of bounds"
        );
        BufferSlice {
            id: self.id,
            value: self
                .value
                .slice(allocation.byte_offset + start..allocation.byte_offset + end),
        }
    }

    #[inline]
    pub fn unmap(&self) {
        self.value.unmap();
    }
}

impl From<wgpu::Buffer> for Buffer {
    fn from(value: wgpu::Buffer) -> Self {
        Buffer {
            id: BufferId::new(),
            value: WgpuWrapper::new(value),
            uniform_allocation: None,
        }
    }
}

impl Deref for Buffer {
    type Target = wgpu::Buffer;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

#[derive(Clone, Debug)]
pub struct BufferSlice<'a> {
    id: BufferId,
    value: wgpu::BufferSlice<'a>,
}

impl<'a> BufferSlice<'a> {
    #[inline]
    pub fn id(&self) -> BufferId {
        self.id
    }
}

impl<'a> Deref for BufferSlice<'a> {
    type Target = wgpu::BufferSlice<'a>;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.value
    }
}
