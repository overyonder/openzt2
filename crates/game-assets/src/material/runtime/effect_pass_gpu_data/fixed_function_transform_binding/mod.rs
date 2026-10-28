//! Descriptor-resolved inputs and dirty state for retained fixed-function expressions.

use std::io;

use bevy::prelude::*;
use d3d9_effects::{
    effect_evaluation::retained_transform_evaluation::{
        D3d9TransformOutputLayout, RetainedD3d9TransformExpression,
    },
    effect_types::{D3d9EffectParameterAssignment, D3d9EffectParameterDescription},
};

use super::super::resolved_effect_transform_bindings::{
    EffectDrawTransforms, EffectRenderViewTransforms, ResolvedEffectTransformSemantic, FOG_INPUT,
    LIGHT_INPUT, MODEL_INPUT, VIEW_INPUT, WIND_INPUT,
};
use super::EffectPassMaterial;
use crate::material::authored_material_parameter_lowering::{
    MaterialEffectParameterValue, OwnedMaterialEffectParameter,
};

#[derive(Clone, Debug)]
pub(in crate::material::runtime) struct DynamicFixedFunctionTransformBinding {
    stage: usize,
    expression: RetainedD3d9TransformExpression,
    parameters: Vec<ResolvedFixedFunctionTransformParameter>,
    initialized: bool,
    failed: bool,
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
            initialized: false,
            failed: false,
        })
    }

    pub(in crate::material::runtime) fn input_dependencies(&self) -> u8 {
        self.parameters.iter().fold(0, |mask, parameter| {
            mask | parameter.semantic.input_dependencies()
        })
    }
}

// Temporary parameter values borrow environment registers or contain one computed
// draw transform. The material owns only the last submitted values for dirty checks.
enum FixedTransformParameterInput<'a> {
    Matrix([f32; 16]),
    Vector([f32; 4]),
    Registers(&'a [Vec4]),
}

impl FixedTransformParameterInput<'_> {
    fn len(&self) -> usize {
        match self {
            Self::Matrix(_) => 16,
            Self::Vector(_) => 4,
            Self::Registers(values) => values.len() * 4,
        }
    }

    fn component(&self, index: usize) -> f32 {
        match self {
            Self::Matrix(values) => values[index],
            Self::Vector(values) => values[index],
            Self::Registers(values) => values[index / 4][index % 4],
        }
    }

    fn matches(&self, cached: &MaterialEffectParameterValue) -> bool {
        let values = match cached {
            MaterialEffectParameterValue::FloatMatrix(values) => &values[..],
            MaterialEffectParameterValue::FloatVector(values) => &values[..],
            _ => return false,
        };
        values.len() == self.len()
            && values
                .iter()
                .enumerate()
                .all(|(index, value)| value.to_bits() == self.component(index).to_bits())
    }

    fn into_owned(self) -> MaterialEffectParameterValue {
        match self {
            Self::Matrix(values) => MaterialEffectParameterValue::FloatMatrix(Box::new(values)),
            _ => MaterialEffectParameterValue::FloatVector(
                (0..self.len()).map(|index| self.component(index)).collect(),
            ),
        }
    }
}

fn parameter_input<'a>(
    semantic: ResolvedEffectTransformSemantic,
    transforms: &EffectDrawTransforms<'_>,
    wind: Option<&'a [Vec4]>,
    light: Option<&'a [Vec4]>,
    fog: Option<&'a [Vec4]>,
) -> Option<FixedTransformParameterInput<'a>> {
    if let Some(matrix) = transforms.matrix(semantic) {
        // FX parameters use padded rows. Keep the existing draw transform's
        // D3D/Bevy convention; declaration-majority packing belongs to the compiler.
        return Some(FixedTransformParameterInput::Matrix(matrix.to_cols_array()));
    }
    if let Some(vector) = transforms.vector(semantic) {
        return Some(FixedTransformParameterInput::Vector(vector.to_array()));
    }
    let registers = match semantic {
        ResolvedEffectTransformSemantic::Wind => wind,
        ResolvedEffectTransformSemantic::Light => light,
        ResolvedEffectTransformSemantic::Fog => fog,
        _ => None,
    }?;
    Some(FixedTransformParameterInput::Registers(registers))
}

impl EffectPassMaterial {
    /// Updates authored matrix parameters used by retained texture-stage expressions.
    ///
    /// # Errors
    /// Returns an error if the native expression cannot evaluate the supplied value.
    pub fn bind_fixed_function_matrix_effect_semantic(
        &mut self,
        semantic_name: &str,
        matrix: Mat4,
    ) -> io::Result<bool> {
        let mut matched = false;
        for binding in &mut self.fixed_transform_bindings {
            let mut changed = false;
            for parameter in &mut binding.parameters {
                if !parameter
                    .assignment
                    .parameter_name
                    .eq_ignore_ascii_case(semantic_name)
                {
                    continue;
                }
                matched = true;
                changed |= binding.failed;
                let input = FixedTransformParameterInput::Matrix(matrix.to_cols_array());
                if !input.matches(&parameter.assignment.parameter_value) {
                    parameter.assignment.parameter_value = input.into_owned();
                    changed = true;
                }
            }
            if !changed {
                continue;
            }
            let assignments: Vec<_> = binding
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
            self.fixed_transform_evaluations += 1;
            let components = binding.expression.evaluate(&assignments).map_err(|error| {
                binding.failed = true;
                self.fixed_transform_evaluation_failures += 1;
                io::Error::other(error)
            })?;
            binding.failed = false;
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

    #[allow(
        clippy::too_many_arguments,
        reason = "each evaluation consumes the existing draw and environment input owners"
    )]
    pub fn bind_fixed_function_transform_expressions(
        &mut self,
        model_to_world: Mat4,
        view_entity: Entity,
        view: &EffectRenderViewTransforms,
        model_changed: bool,
        view_changed: bool,
        wind: Option<&[Vec4]>,
        light: Option<&[Vec4]>,
        fog: Option<&[Vec4]>,
    ) {
        let changed = u8::from(model_changed) * MODEL_INPUT
            | u8::from(view_changed) * VIEW_INPUT
            | u8::from(wind.is_some()) * WIND_INPUT
            | u8::from(light.is_some()) * LIGHT_INPUT
            | u8::from(fog.is_some()) * FOG_INPUT;
        let transforms = EffectDrawTransforms::new(model_to_world, view);
        let has_reflection_projection = self
            .model_reflection_projections
            .iter()
            .any(Option::is_some);
        if has_reflection_projection {
            // Direction generation is in camera space. Remove model scale before
            // rotating those directions into the texture effect's mesh-local frame.
            let (_, rotation, _) = model_to_world.to_scale_rotation_translation();
            let model_from_world = Mat4::from_quat(rotation.conjugate());
            let view_to_world = Mat4::from_mat3(Mat3::from_mat4(view.view_to_world));
            for (stage, projection) in self.model_reflection_projections.iter().enumerate() {
                let Some(projection) = projection else {
                    continue;
                };
                let next = *projection * model_from_world * view_to_world;
                if self.fixed_transforms.values[stage] != next {
                    self.fixed_transforms.values[stage] = next;
                    self.dirty_fixed_transforms = true;
                }
            }
        }
        for binding in &mut self.fixed_transform_bindings {
            if binding.input_dependencies() == 0
                || (binding.initialized && binding.input_dependencies() & changed == 0)
            {
                continue;
            }
            let mut inputs_changed = !binding.initialized;
            for parameter in &mut binding.parameters {
                if let Some(input) =
                    parameter_input(parameter.semantic, &transforms, wind, light, fog)
                {
                    if !input.matches(&parameter.assignment.parameter_value) {
                        parameter.assignment.parameter_value = input.into_owned();
                        inputs_changed = true;
                    }
                }
            }
            if !inputs_changed {
                continue;
            }
            binding.initialized = true;
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
            self.fixed_transform_evaluations += 1;
            match binding.expression.evaluate(&assignments) {
                Ok(components) => {
                    binding.failed = false;
                    // Use precisely the same packed state mapping as initial pass
                    // accumulation. Native output dimensions are not HLSL dimensions.
                    let next = Mat4::from_cols_array(&components);
                    if self.fixed_transforms.values[binding.stage]
                        .to_cols_array()
                        .map(f32::to_bits)
                        != components.map(f32::to_bits)
                    {
                        self.fixed_transforms.values[binding.stage] = next;
                        self.dirty_fixed_transforms = true;
                    }
                }
                Err(error) => {
                    binding.failed = true;
                    self.fixed_transform_evaluation_failures += 1;
                    // Preserve the last valid buffer, report failure, and retry only
                    // when inputs change. A rejected evaluation is never a settlement.
                    error!(texture_stage = binding.stage, ?view_entity, %error, "fixed-function transform evaluation failed");
                }
            }
        }
        if (has_reflection_projection || !self.fixed_transform_bindings.is_empty())
            && self
                .fixed_transform_bindings
                .iter()
                .filter(|binding| binding.input_dependencies() != 0)
                .all(|binding| binding.initialized && !binding.failed)
        {
            self.fixed_transform_render_view = Some(view_entity);
        } else {
            self.fixed_transform_render_view = None;
        }
    }

    /// Borrow the actual submitted inputs, including their native descriptor key.
    pub fn fixed_transform_parameter_inputs(&self) -> impl Iterator<Item = (usize, &str, &[f32])> {
        self.fixed_transform_bindings
            .iter()
            .filter(|binding| binding.input_dependencies() != 0)
            .flat_map(|binding| {
                binding.parameters.iter().filter_map(move |parameter| {
                    let values = match &parameter.assignment.parameter_value {
                        MaterialEffectParameterValue::FloatMatrix(values) => &values[..],
                        MaterialEffectParameterValue::FloatVector(values) if !values.is_empty() => {
                            &values[..]
                        }
                        _ => return None,
                    };
                    Some((
                        binding.stage,
                        parameter.assignment.parameter_name.as_str(),
                        values,
                    ))
                })
            })
    }

    pub fn fixed_transform_output_layouts(
        &self,
    ) -> impl Iterator<Item = (usize, D3d9TransformOutputLayout)> + '_ {
        self.fixed_transform_bindings
            .iter()
            .filter(|binding| binding.input_dependencies() != 0)
            .map(|binding| (binding.stage, binding.expression.output_layout()))
    }

    pub fn fixed_transform_runtime_measurements(&self) -> (Option<Entity>, u64, u64, u64) {
        (
            self.fixed_transform_render_view,
            self.fixed_transform_evaluations,
            self.fixed_transform_evaluation_failures,
            self.fixed_transform_buffer_writes,
        )
    }

    pub fn fixed_transform_buffer_handle(&self) -> &Handle<bevy::render::storage::ShaderBuffer> {
        &self.uniform_buffer_assets.fixed_transforms
    }

    pub fn fixed_transform_stage_values(&self, stage: usize) -> Option<&Mat4> {
        self.fixed_transforms.values.get(stage)
    }
}
