//! Texture-stage transforms computed from FX parameters.
//!
//! A stage that reads a renderer semantic runs in the shader, and its other
//! inputs live in the material's transform-input buffer. A stage that reads
//! only owner-set parameters is evaluated here whenever an owner sets one.

use std::io;

use bevy::prelude::*;
use d3d9_effects::{
    effect_evaluation::retained_transform_evaluation::{
        D3d9TransformProgram, RetainedD3d9TransformExpression,
    },
    effect_types::{D3d9EffectParameterAssignment, D3d9EffectParameterDescription},
};

use super::super::gpu_texture_transforms::dependency_components;
use super::super::resolved_effect_transform_bindings::ResolvedEffectTransformSemantic;
use super::EffectPassMaterial;
use crate::material::authored_material_parameter_lowering::{
    MaterialEffectParameterValue, OwnedMaterialEffectParameter,
};

#[derive(Clone, Debug)]
pub(in crate::material::runtime) struct DynamicFixedFunctionTransformBinding {
    stage: usize,
    expression: RetainedD3d9TransformExpression,
    parameters: Vec<ResolvedFixedFunctionTransformParameter>,
    /// The stage's first register in the transform-input buffer, when the
    /// shader computes it.
    gpu_first_input: Option<u32>,
}

#[derive(Clone, Debug)]
struct ResolvedFixedFunctionTransformParameter {
    semantic: ResolvedEffectTransformSemantic,
    assignment: OwnedMaterialEffectParameter,
}

impl DynamicFixedFunctionTransformBinding {
    pub(in crate::material::runtime) fn stage(&self) -> usize {
        self.stage
    }

    pub(in crate::material::runtime) fn resolve(
        stage: usize,
        expression: &RetainedD3d9TransformExpression,
        descriptions: &[D3d9EffectParameterDescription],
    ) -> io::Result<Self> {
        let parameters = expression
            .parameter_indices()
            .iter()
            .map(|index| {
                let description = descriptions.get(*index as usize).ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        "fixed-transform dependency has no parameter descriptor",
                    )
                })?;
                let name = description
                    .semantic_name
                    .as_deref()
                    .unwrap_or(&description.parameter_name);
                let semantic = ResolvedEffectTransformSemantic::from_authored_name(name);
                // Resolve to the actual native assignment receiver at load time. The
                // setter prefers semantics to names; do not depend on unknown-name behavior.
                let receiver = descriptions
                    .iter()
                    .find(|candidate| candidate.semantic_name.as_deref() == Some(name))
                    .or_else(|| {
                        descriptions
                            .iter()
                            .find(|candidate| candidate.parameter_name == name)
                    });
                if !receiver.is_some_and(|receiver| std::ptr::eq(receiver, description)) {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("ambiguous fixed-transform parameter receiver {name}"),
                    ));
                }
                Ok(ResolvedFixedFunctionTransformParameter {
                    semantic,
                    assignment: OwnedMaterialEffectParameter {
                        parameter_name: name.to_owned(),
                        parameter_value: MaterialEffectParameterValue::FloatVector(Box::default()),
                    },
                })
            })
            .collect::<io::Result<Vec<_>>>()?;
        Ok(Self {
            stage,
            expression: expression.clone(),
            parameters,
            gpu_first_input: None,
        })
    }

    /// Each dependency's semantic, in the program's dependency order.
    pub(in crate::material::runtime) fn semantics(&self) -> Vec<ResolvedEffectTransformSemantic> {
        self.parameters
            .iter()
            .map(|parameter| parameter.semantic)
            .collect()
    }

    pub(in crate::material::runtime) fn program(&self) -> &D3d9TransformProgram {
        self.expression.compiled_program()
    }

    pub(in crate::material::runtime) fn compute_on_gpu(&mut self, first_input: u32) {
        self.gpu_first_input = Some(first_input);
    }
}

impl EffectPassMaterial {
    /// Sets an owner-bound matrix parameter of the texture-stage expressions.
    ///
    /// # Errors
    /// Returns an error if the native expression cannot evaluate the supplied value.
    pub fn bind_fixed_function_matrix_effect_semantic(
        &mut self,
        semantic_name: &str,
        matrix: Mat4,
    ) -> io::Result<bool> {
        let components = matrix.to_cols_array();
        let mut matched = false;
        for binding in &mut self.fixed_transform_bindings {
            let mut changed = false;
            for (dependency, parameter) in binding.parameters.iter_mut().enumerate() {
                if !parameter
                    .assignment
                    .parameter_name
                    .eq_ignore_ascii_case(semantic_name)
                {
                    continue;
                }
                matched = true;
                if let Some(first_input) = binding.gpu_first_input {
                    let Some((start, count)) =
                        dependency_components(binding.expression.compiled_program(), dependency)
                    else {
                        continue;
                    };
                    let first = first_input as usize * 4 + start;
                    for (offset, value) in components.iter().take(count).enumerate() {
                        let target =
                            &mut self.transform_inputs[(first + offset) / 4][(first + offset) % 4];
                        if target.to_bits() != value.to_bits() {
                            *target = *value;
                            self.dirty_transform_inputs = true;
                        }
                    }
                    continue;
                }
                let unchanged = matches!(
                    &parameter.assignment.parameter_value,
                    MaterialEffectParameterValue::FloatMatrix(values)
                        if values.map(f32::to_bits) == components.map(f32::to_bits)
                );
                if !unchanged {
                    parameter.assignment.parameter_value =
                        MaterialEffectParameterValue::FloatMatrix(Box::new(components));
                    changed = true;
                }
            }
            if !changed {
                continue;
            }
            let assignments: Vec<D3d9EffectParameterAssignment<'_>> = binding
                .parameters
                .iter()
                .filter(|parameter| {
                    !matches!(
                        &parameter.assignment.parameter_value,
                        MaterialEffectParameterValue::FloatVector(values) if values.is_empty()
                    )
                })
                .map(|parameter| {
                    parameter
                        .assignment
                        .borrowed_d3d9_effect_parameter_assignment()
                })
                .collect();
            let components = binding
                .expression
                .evaluate(&assignments)
                .map_err(io::Error::other)?;
            if self.fixed_transforms.values[binding.stage]
                .to_cols_array()
                .map(f32::to_bits)
                != components.map(f32::to_bits)
            {
                self.fixed_transforms.values[binding.stage] = Mat4::from_cols_array(&components);
                self.dirty_fixed_transforms = true;
            }
        }
        Ok(matched)
    }

    pub fn fixed_transform_buffer_handle(&self) -> &Handle<bevy::render::storage::ShaderBuffer> {
        &self.uniform_buffer_assets.fixed_transforms
    }

    pub fn fixed_transform_stage_values(&self, stage: usize) -> Option<&Mat4> {
        self.fixed_transforms.values.get(stage)
    }
}
