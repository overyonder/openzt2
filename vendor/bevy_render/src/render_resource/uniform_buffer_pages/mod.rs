//! Aligned, reference-counted uniform ranges in shared GPU buffer pages.

use std::sync::{Arc, Mutex, Weak};

use bevy_ecs::prelude::*;
use offset_allocator::{Allocation, Allocator};

use crate::{render_resource::Buffer, renderer::RenderDevice};

const UNIFORM_PAGE_BYTES: u32 = 1024 * 1024;

/// Packs opted-in shader uniforms without changing their asset identities.
/// Empty pages are released once their last allocation/binding is dropped.
#[derive(Resource)]
pub struct UniformBufferPageAllocator {
    pages: Vec<Weak<UniformBufferPage>>,
    alignment: u32,
}

#[derive(Debug)]
struct UniformBufferPage {
    buffer: Buffer,
    slots: Mutex<Allocator>,
}

pub(crate) struct UniformBufferPageAllocation {
    page: Arc<UniformBufferPage>,
    allocation: Allocation,
    pub(crate) byte_offset: u64,
    pub(crate) byte_size: wgpu::BufferSize,
}

impl std::fmt::Debug for UniformBufferPageAllocation {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("UniformBufferPageAllocation")
            .field("byte_offset", &self.byte_offset)
            .field("byte_size", &self.byte_size)
            .finish_non_exhaustive()
    }
}

impl Drop for UniformBufferPageAllocation {
    fn drop(&mut self) {
        self.page
            .slots
            .lock()
            .expect("uniform page allocator poisoned")
            .free(self.allocation);
    }
}

impl FromWorld for UniformBufferPageAllocator {
    fn from_world(world: &mut World) -> Self {
        Self {
            pages: Vec::new(),
            alignment: world
                .resource::<RenderDevice>()
                .limits()
                .min_uniform_buffer_offset_alignment,
        }
    }
}

impl UniformBufferPageAllocator {
    pub(crate) fn allocate_uniform_buffer(&mut self, device: &RenderDevice, size: u64) -> Buffer {
        let byte_size = wgpu::BufferSize::new(size).expect("uniform ranges cannot be empty");
        assert!(size <= u64::from(device.limits().max_uniform_buffer_binding_size));
        let units = u32::try_from(size.div_ceil(u64::from(self.alignment)))
            .expect("uniform size fits the device limits");
        for existing in &self.pages {
            let Some(page) = existing.upgrade() else {
                continue;
            };
            let allocation = page
                .slots
                .lock()
                .expect("uniform page allocator poisoned")
                .allocate(units);
            if let Some(allocation) = allocation {
                return self.bind_uniform_allocation(page, allocation, byte_size);
            }
        }
        self.pages.retain(|page| page.strong_count() != 0);
        let page_units = UNIFORM_PAGE_BYTES.div_ceil(self.alignment).max(units);
        let page = Arc::new(UniformBufferPage {
            buffer: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("shared shader uniform page"),
                size: u64::from(page_units) * u64::from(self.alignment),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            slots: Mutex::new(Allocator::with_max_allocs(page_units, page_units + 1)),
        });
        let allocation = page
            .slots
            .lock()
            .expect("uniform page allocator poisoned")
            .allocate(units)
            .expect("new uniform page fits its first allocation");
        self.pages.push(Arc::downgrade(&page));
        self.bind_uniform_allocation(page, allocation, byte_size)
    }

    fn bind_uniform_allocation(
        &self,
        page: Arc<UniformBufferPage>,
        allocation: Allocation,
        byte_size: wgpu::BufferSize,
    ) -> Buffer {
        let buffer = page.buffer.clone();
        buffer.with_uniform_allocation(Arc::new(UniformBufferPageAllocation {
            byte_offset: u64::from(allocation.offset) * u64::from(self.alignment),
            byte_size,
            page,
            allocation,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::create_dummy_device;

    #[test]
    fn uniform_ranges_share_a_page_and_keep_binding_ownership() {
        let (device, _) = create_dummy_device();
        let mut world = World::new();
        world.insert_resource(device.clone());
        let mut allocator = UniformBufferPageAllocator::from_world(&mut world);
        let first = allocator.allocate_uniform_buffer(&device, 64);
        let first_offset = first.binding_offset();
        let retained_binding = first.clone();
        let second = allocator.allocate_uniform_buffer(&device, 4096);
        assert_eq!(allocator.pages.len(), 1);
        assert_ne!(first.id(), second.id());
        assert_ne!(first_offset, second.binding_offset());
        assert_eq!(second.binding_offset() % u64::from(allocator.alignment), 0);
        match second.as_entire_binding() {
            wgpu::BindingResource::Buffer(binding) => {
                assert_eq!(binding.offset, second.binding_offset());
                assert_eq!(binding.size.unwrap().get(), 4096);
            }
            _ => panic!("expected a buffer binding"),
        }
        drop(first);
        let third = allocator.allocate_uniform_buffer(&device, 64);
        assert_ne!(third.binding_offset(), first_offset);
        drop(retained_binding);
        let reused = allocator.allocate_uniform_buffer(&device, 64);
        assert_eq!(reused.binding_offset(), first_offset);
        drop((second, third, reused));
        assert!(allocator.pages[0].upgrade().is_none());
    }
}
