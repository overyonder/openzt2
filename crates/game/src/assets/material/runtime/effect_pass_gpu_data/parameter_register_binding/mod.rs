//! Effect parameter and transform register updates.

use super::super::resolved_effect_transform_bindings::{
    EffectRenderViewTransforms, ResolvedEffectTransformSemantic, MODEL_INPUT, VIEW_INPUT,
};
use super::{D3d9ProgrammableShaderStage, EffectPassMaterial};
use bevy::prelude::*;

impl EffectPassMaterial {
    pub(crate) fn bind_changed_environment_registers(
        &mut self,
        wind: Option<&[Vec4]>,
        light: Option<&[Vec4]>,
        fog: Option<&[Vec4]>,
    ) {
        for binding in self.dynamic_parameter_register_bindings.iter() {
            let values = match binding.transform_semantic {
                ResolvedEffectTransformSemantic::Wind => wind,
                ResolvedEffectTransformSemantic::Light => light,
                ResolvedEffectTransformSemantic::Fog => fog,
                _ => None,
            };
            let Some(values) = values else {
                continue;
            };
            if binding.register_set != 2 {
                continue;
            }
            let target = match binding.shader_stage {
                D3d9ProgrammableShaderStage::Vertex => &mut self.vertex_floats.values,
                D3d9ProgrammableShaderStage::Pixel => &mut self.pixel_floats.values,
            };
            let first = binding.first_register as usize;
            let count = binding.register_count as usize;
            if first.saturating_add(count) > target.len() {
                continue;
            }
            write_changed_float_registers(
                target,
                first,
                &values[..values.len().min(count)],
                &mut self.dirty_float_register_stages,
                binding.shader_stage,
            );
        }
    }

    pub(crate) fn bind_float_vector_effect_semantic(&mut self, semantic_name: &str, value: Vec4) {
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

    pub(crate) fn bind_float_matrix_effect_semantic(&mut self, semantic_name: &str, matrix: Mat4) {
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

    pub(crate) fn bind_per_draw_transform_semantics(
        &mut self,
        model_to_world: Mat4,
        view: &EffectRenderViewTransforms,
        model_changed: bool,
        view_changed: bool,
    ) {
        let world_to_model = std::cell::OnceCell::new();
        let model_to_view = std::cell::OnceCell::new();
        let model_to_projection = std::cell::OnceCell::new();
        let changed =
            (u8::from(model_changed) * MODEL_INPUT) | (u8::from(view_changed) * VIEW_INPUT);
        for binding in self.dynamic_parameter_register_bindings.iter() {
            use ResolvedEffectTransformSemantic as Semantic;
            if binding.register_set != 2 {
                continue;
            }
            let dependencies = binding.transform_semantic.input_dependencies();
            if dependencies != 0 && dependencies & changed == 0 {
                continue;
            }
            let target = match binding.shader_stage {
                D3d9ProgrammableShaderStage::Vertex => &mut self.vertex_floats.values,
                D3d9ProgrammableShaderStage::Pixel => &mut self.pixel_floats.values,
            };
            let vector = match binding.transform_semantic {
                Semantic::CameraPosition => Some(view.camera_position.extend(1.0)),
                Semantic::CameraAcross => Some(view.camera_across.extend(0.0)),
                Semantic::CameraUp => Some(view.camera_up.extend(0.0)),
                _ => None,
            };
            if let Some(vector) = vector {
                write_changed_float_registers(
                    target,
                    binding.first_register as usize,
                    &[vector],
                    &mut self.dirty_float_register_stages,
                    binding.shader_stage,
                );
                continue;
            }
            let matrix = match binding.transform_semantic {
                Semantic::Identity => Mat4::IDENTITY,
                Semantic::ModelToWorld => model_to_world,
                Semantic::WorldToModel => *world_to_model.get_or_init(|| model_to_world.inverse()),
                Semantic::VectorToWorld => world_to_model
                    .get_or_init(|| model_to_world.inverse())
                    .transpose(),
                Semantic::VectorWorldToView => view.view_to_world.transpose(),
                Semantic::ModelToView => {
                    *model_to_view.get_or_init(|| view.world_to_view * model_to_world)
                }
                Semantic::ModelToProjection => {
                    *model_to_projection.get_or_init(|| view.world_to_projection * model_to_world)
                }
                Semantic::WorldToView => view.world_to_view,
                Semantic::ViewToWorld => view.view_to_world,
                Semantic::ViewToProjection => view.view_to_projection,
                Semantic::WorldToProjection => view.world_to_projection,
                _ => continue,
            };
            if binding.register_set != 2 || binding.register_count < 4 {
                continue;
            }
            let first_register = binding.first_register as usize;
            if first_register > 252 {
                continue;
            }
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
