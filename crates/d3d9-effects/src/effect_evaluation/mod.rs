use std::path::Path;

use crate::{
    effect_compilation::compile_d3d9_effect_source_to_fx2_bytecode,
    effect_types::{D3d9EffectIncludeResolver, D3d9EffectParameterAssignment, EvaluatedD3d9Effect},
    error::D3d9EffectProcessingError,
};

mod evaluated_pass_copying;
mod evaluated_state_copying;
mod mojoshader_effect_allocation;
mod parameter_application;

/// Compiles and evaluates an Effects document through vkd3d-shader and
/// `MojoShader`'s dependency-owned FX2 evaluator.
/// Assignments match FX semantics first, then exact variable names.
///
/// # Errors
///
/// Returns compiler, include, compiled-effect, or parameter errors.
pub fn compile_and_evaluate_d3d9_effect_source(
    effect_path: &Path,
    effect_source: &[u8],
    include_resolver: &mut dyn D3d9EffectIncludeResolver,
    parameter_assignments: &[D3d9EffectParameterAssignment<'_>],
) -> Result<EvaluatedD3d9Effect, D3d9EffectProcessingError> {
    let compiled_effect_bytecode =
        compile_d3d9_effect_source_to_fx2_bytecode(effect_path, effect_source, include_resolver)?;
    let effect_allocation =
        mojoshader_effect_allocation::open_compiled_d3d9_effect_bytecode_with_mojoshader(
            &compiled_effect_bytecode,
        )?;
    parameter_application::apply_d3d9_effect_parameter_assignments_to_mojoshader_effect(
        effect_allocation.native_effect_pointer,
        parameter_assignments,
    )?;
    evaluated_state_copying::copy_evaluated_d3d9_effect_from_mojoshader(&effect_allocation)
}
