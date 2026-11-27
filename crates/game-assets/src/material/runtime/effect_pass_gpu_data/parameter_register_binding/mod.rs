//! Register updates for semantics the material's owner binds.

use super::{D3d9ProgrammableShaderStage, EffectPassMaterial};
use bevy::prelude::*;

impl EffectPassMaterial {
    pub fn bind_float_vector_effect_semantic(&mut self, semantic_name: &str, value: Vec4) {
        for binding in self.dynamic_parameter_register_bindings.iter() {
            if binding.register_set != 2
                || !binding.semantic_name.eq_ignore_ascii_case(semantic_name)
            {
                continue;
            }
            let target = match binding.shader_stage {
                D3d9ProgrammableShaderStage::Vertex => &mut self.vertex_floats.values,
                D3d9ProgrammableShaderStage::Pixel => &mut self.pixel_floats.values,
            };
            write_changed_float_registers(
                target,
                binding.first_register as usize,
                &[value],
                &mut self.dirty_float_register_stages,
                binding.shader_stage,
            );
        }
    }

    pub fn bind_float_matrix_effect_semantic(&mut self, semantic_name: &str, matrix: Mat4) {
        for binding in self.dynamic_parameter_register_bindings.iter() {
            if binding.register_set != 2
                || binding.register_count < 4
                || !binding.semantic_name.eq_ignore_ascii_case(semantic_name)
            {
                continue;
            }
            let first_register = binding.first_register as usize;
            if first_register > 252 {
                continue;
            }
            let target = match binding.shader_stage {
                D3d9ProgrammableShaderStage::Vertex => &mut self.vertex_floats.values,
                D3d9ProgrammableShaderStage::Pixel => &mut self.pixel_floats.values,
            };
            write_changed_float_registers(
                target,
                first_register,
                &if binding.is_row_major_matrix {
                    matrix
                } else {
                    matrix.transpose()
                }
                .to_cols_array_2d()
                .map(Vec4::from_array),
                &mut self.dirty_float_register_stages,
                binding.shader_stage,
            );
        }
    }
}

pub(super) const fn shader_stage_dirty_bit(stage: D3d9ProgrammableShaderStage) -> u8 {
    match stage {
        D3d9ProgrammableShaderStage::Vertex => 1,
        D3d9ProgrammableShaderStage::Pixel => 2,
    }
}

fn write_changed_float_registers(
    target: &mut [Vec4],
    first: usize,
    values: &[Vec4],
    dirty: &mut u8,
    stage: D3d9ProgrammableShaderStage,
) {
    let Some(target) = target.get_mut(first..first.saturating_add(values.len())) else {
        return;
    };
    for (target, value) in target.iter_mut().zip(values) {
        if target.to_array().map(f32::to_bits) != value.to_array().map(f32::to_bits) {
            *target = *value;
            *dirty |= shader_stage_dirty_bit(stage);
        }
    }
}

#[cfg(test)]
mod tests;
