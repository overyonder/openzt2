//! Packing and uploading effect shader registers.

use super::{
    parameter_register_binding::shader_stage_dirty_bit, D3d9BooleanShaderRegisters,
    D3d9FixedFunctionWorldLightContexts, D3d9FloatShaderRegisters, D3d9IntegerShaderRegisters,
    D3d9PackedShaderRegisters, D3d9ProgrammableShaderStage, EffectPassMaterial,
    EffectPassUniformBufferAssets,
};
use bevy::{
    asset::RenderAssetUsages,
    platform::collections::AlignedVec,
    prelude::*,
    render::{
        render_resource::{encase::UniformBuffer, BufferUsages, ShaderType},
        storage::{ShaderBuffer, ShaderBufferData},
    },
};

impl EffectPassMaterial {
    pub fn recreate_uniform_buffers_for_runtime_material(
        &mut self,
        buffers: &mut Assets<ShaderBuffer>,
    ) {
        self.uniform_buffer_assets = EffectPassUniformBufferAssets {
            fixed_function: buffers.add(shader_buffer_from_uniform_value(&self.fixed_function)),
            vertex_floats: buffers.add(shader_buffer_from_packed_registers(
                &pack_programmable_shader_registers(
                    &self.vertex_floats,
                    &self.vertex_integers,
                    &self.vertex_booleans,
                    self.vertex_uniform_upload_order.as_deref(),
                ),
            )),
            vertex_integers: buffers.add(shader_buffer_from_uniform_value(&self.vertex_integers)),
            vertex_booleans: buffers.add(shader_buffer_from_uniform_value(&self.vertex_booleans)),
            pixel_floats: buffers.add(shader_buffer_from_packed_registers(
                &pack_programmable_shader_registers(
                    &self.pixel_floats,
                    &self.pixel_integers,
                    &self.pixel_booleans,
                    self.pixel_uniform_upload_order.as_deref(),
                ),
            )),
            pixel_integers: buffers.add(shader_buffer_from_uniform_value(&self.pixel_integers)),
            pixel_booleans: buffers.add(shader_buffer_from_uniform_value(&self.pixel_booleans)),
            fixed_transforms: buffers.add(shader_buffer_from_uniform_value(&self.fixed_transforms)),
            transform_inputs: buffers.add(ShaderBuffer::from(self.transform_inputs.clone())),
        };
        self.dirty_float_register_stages = 0;
        self.dirty_fixed_transforms = false;
        self.dirty_fixed_function = false;
        self.dirty_transform_inputs = false;
    }

    pub fn write_changed_fixed_function_registers_to_persistent_buffer(
        &mut self,
        buffers: &mut Assets<ShaderBuffer>,
    ) {
        if self.dirty_fixed_function
            && buffers.contains(self.uniform_buffer_assets.fixed_function.id())
        {
            write_uniform_value_to_shader_buffer(
                &self.uniform_buffer_assets.fixed_function,
                &self.fixed_function,
                buffers,
            );
            self.dirty_fixed_function = false;
        }
    }

    pub fn write_programmable_float_registers_to_persistent_buffers(
        &mut self,
        buffers: &mut Assets<ShaderBuffer>,
    ) {
        for (stage, handle, values, order) in [
            (
                D3d9ProgrammableShaderStage::Vertex,
                &self.uniform_buffer_assets.vertex_floats,
                &self.vertex_floats,
                self.vertex_uniform_upload_order.as_deref(),
            ),
            (
                D3d9ProgrammableShaderStage::Pixel,
                &self.uniform_buffer_assets.pixel_floats,
                &self.pixel_floats,
                self.pixel_uniform_upload_order.as_deref(),
            ),
        ] {
            if self.dirty_float_register_stages & shader_stage_dirty_bit(stage) != 0
                && self
                    .dynamic_parameter_register_bindings
                    .iter()
                    .any(|binding| binding.shader_stage == stage && binding.register_set == 2)
            {
                let (integers, booleans) = match stage {
                    D3d9ProgrammableShaderStage::Vertex => {
                        (&self.vertex_integers, &self.vertex_booleans)
                    }
                    D3d9ProgrammableShaderStage::Pixel => {
                        (&self.pixel_integers, &self.pixel_booleans)
                    }
                };
                write_packed_registers_to_shader_buffer(
                    handle, values, integers, booleans, order, buffers,
                );
            }
        }
        self.dirty_float_register_stages = 0;
    }

    pub fn write_fixed_function_transforms_to_persistent_buffer(
        &mut self,
        buffers: &mut Assets<ShaderBuffer>,
    ) {
        if buffers.contains(self.uniform_buffer_assets.fixed_transforms.id()) {
            write_uniform_value_to_shader_buffer(
                &self.uniform_buffer_assets.fixed_transforms,
                &self.fixed_transforms,
                buffers,
            );
            self.dirty_fixed_transforms = false;
        }
    }

    pub fn write_changed_fixed_function_transforms_to_persistent_buffer(
        &mut self,
        buffers: &mut Assets<ShaderBuffer>,
    ) {
        if self.dirty_fixed_transforms {
            self.write_fixed_function_transforms_to_persistent_buffer(buffers);
        }
        if self.dirty_transform_inputs {
            if let Some(mut buffer) = buffers.get_mut(&self.uniform_buffer_assets.transform_inputs)
            {
                if let Some(values) = buffer.cast_slice_mut::<Vec4>() {
                    values.copy_from_slice(&self.transform_inputs);
                    self.dirty_transform_inputs = false;
                }
            }
        }
    }
}

pub(in crate::material::runtime) fn pack_programmable_shader_registers(
    values: &D3d9FloatShaderRegisters,
    integers: &D3d9IntegerShaderRegisters,
    booleans: &D3d9BooleanShaderRegisters,
    order: Option<&[Vec<u32>; 3]>,
) -> D3d9PackedShaderRegisters {
    let mut packed = D3d9PackedShaderRegisters {
        values: [Vec4::ZERO; 288],
        active_register_count: order.map_or(256, |banks| {
            banks.iter().map(Vec::len).sum::<usize>().max(1)
        }),
    };
    for (destination, source) in
        packed
            .values
            .iter_mut()
            .zip(programmable_shader_registers_in_upload_order(
                values, integers, booleans, order,
            ))
    {
        *destination = source;
    }
    packed
}

fn programmable_shader_registers_in_upload_order<'a>(
    values: &'a D3d9FloatShaderRegisters,
    integers: &'a D3d9IntegerShaderRegisters,
    booleans: &'a D3d9BooleanShaderRegisters,
    order: Option<&'a [Vec<u32>; 3]>,
) -> impl ExactSizeIterator<Item = Vec4> + 'a {
    let count = order.map_or(256, |banks| {
        banks.iter().map(Vec::len).sum::<usize>().max(1)
    });
    (0..count).map(move |index| {
        let Some(banks) = order else {
            return values.values[index];
        };
        if index < banks[0].len() {
            return values.values[banks[0][index] as usize];
        }
        let integer_index = index - banks[0].len();
        if integer_index < banks[1].len() {
            return Vec4::from_array(
                integers.values[banks[1][integer_index] as usize]
                    .to_array()
                    .map(|value| f32::from_bits(value as u32)),
            );
        }
        let boolean_index = integer_index - banks[1].len();
        banks[2].get(boolean_index).map_or(Vec4::ZERO, |source| {
            let source = *source as usize;
            Vec4::new(
                f32::from_bits(booleans.values[source / 4][source % 4]),
                0.0,
                0.0,
                0.0,
            )
        })
    })
}

pub(in crate::material::runtime) fn shader_buffer_from_packed_registers(
    value: &D3d9PackedShaderRegisters,
) -> ShaderBuffer {
    let mut bytes = bevy::render::render_resource::encase::StorageBuffer::new(Vec::<u8>::new());
    bytes
        .write(&value.values[..value.active_register_count])
        .expect("vec4 registers have a fixed 16-byte layout");
    let mut buffer = ShaderBuffer::new(bytes.into_inner(), RenderAssetUsages::default());
    buffer.buffer_usage = BufferUsages::UNIFORM | BufferUsages::COPY_DST;
    buffer.use_uniform_buffer_pages = true;
    buffer
}

fn write_packed_registers_to_shader_buffer(
    handle: &Handle<ShaderBuffer>,
    values: &D3d9FloatShaderRegisters,
    integers: &D3d9IntegerShaderRegisters,
    booleans: &D3d9BooleanShaderRegisters,
    order: Option<&[Vec<u32>; 3]>,
    buffers: &mut Assets<ShaderBuffer>,
) {
    if let Some(mut buffer) = buffers.get_mut(handle) {
        let registers =
            programmable_shader_registers_in_upload_order(values, integers, booleans, order);
        let bytes = shader_upload_bytes(&mut buffer, registers.len() * 16);
        // Registers are 16-byte vec4 values; overwrite the retained staging buffer.
        for (destination, register) in bytes.chunks_exact_mut(16).zip(registers) {
            for (component, value) in destination.chunks_exact_mut(4).zip(register.to_array()) {
                component.copy_from_slice(&value.to_bits().to_le_bytes());
            }
        }
    }
}

pub(in crate::material::runtime) fn shader_buffer_from_uniform_value<T>(value: &T) -> ShaderBuffer
where
    T: ShaderType + bevy::render::render_resource::encase::internal::WriteInto,
{
    let bytes = shader_uniform_bytes(value);
    let mut buffer = ShaderBuffer::new(bytes, RenderAssetUsages::default());
    buffer.buffer_usage = BufferUsages::UNIFORM | BufferUsages::COPY_DST;
    buffer.use_uniform_buffer_pages = true;
    buffer
}

fn shader_uniform_bytes<T>(value: &T) -> Vec<u8>
where
    T: ShaderType + bevy::render::render_resource::encase::internal::WriteInto,
{
    let mut buffer = UniformBuffer::new(Vec::new());
    buffer
        .write(value)
        .expect("effect pass register types have valid uniform layouts");
    buffer.into_inner()
}

fn write_uniform_value_to_shader_buffer<T>(
    handle: &Handle<ShaderBuffer>,
    value: &T,
    buffers: &mut Assets<ShaderBuffer>,
) where
    T: ShaderType + bevy::render::render_resource::encase::internal::WriteInto,
{
    if let Some(mut buffer) = buffers.get_mut(handle) {
        // Reuse pending staging bytes until Bevy extracts the buffer.
        let mut staging = UniformBuffer::new(shader_upload_bytes(
            &mut buffer,
            value.size().get() as usize,
        ));
        staging
            .write(value)
            .expect("effect pass register types have valid uniform layouts");
    }
}

fn shader_upload_bytes(buffer: &mut ShaderBuffer, len: usize) -> &mut [u8] {
    let mut pending = match std::mem::take(&mut buffer.data) {
        ShaderBufferData::Initialized { data, .. } => data,
        ShaderBufferData::Uninitialized(_) => AlignedVec::new(1),
    };
    pending.resize(len, 0);
    buffer.data = ShaderBufferData::Initialized {
        data: pending,
        buffer_size: len as u64,
    };
    buffer
        .cast_slice_mut::<u8>()
        .expect("shader upload initialized its byte array")
}

impl super::EffectEnvironmentRegisters {
    /// Writes the registers in the shader's storage-buffer layout.
    pub fn write_shader_buffer(&self, buffer: &mut ShaderBuffer) {
        let bytes = shader_upload_bytes(buffer, self.size().get() as usize);
        bevy::render::render_resource::encase::StorageBuffer::new(bytes)
            .write(self)
            .expect("environment registers have a fixed storage layout");
    }
}

impl D3d9FixedFunctionWorldLightContexts {
    /// Writes the active light registers in the shader's storage-buffer layout.
    pub fn write_shader_buffer(&self, buffer: &mut ShaderBuffer) {
        let bytes = shader_upload_bytes(buffer, self.size().get() as usize);
        bevy::render::render_resource::encase::StorageBuffer::new(bytes)
            .write(self)
            .expect("world light registers have a fixed storage layout");
    }
}

#[cfg(test)]
mod tests;
