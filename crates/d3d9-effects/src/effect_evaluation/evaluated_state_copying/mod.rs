use crate::{
    effect_types::{
        D3d9EffectParameterDescription, D3d9EffectStructMemberDescription, EvaluatedD3d9Effect,
        EvaluatedD3d9EffectTechnique,
    },
    error::D3d9EffectProcessingError,
    native_abi::{
        openzt2_effect_begin_pass, openzt2_effect_end_pass, openzt2_effect_execution_error,
        openzt2_effect_parameter_annotations, openzt2_effect_parameter_class,
        openzt2_effect_parameter_columns, openzt2_effect_parameter_count,
        openzt2_effect_parameter_elements, openzt2_effect_parameter_member,
        openzt2_effect_parameter_member_count, openzt2_effect_parameter_name,
        openzt2_effect_parameter_rows, openzt2_effect_parameter_semantic,
        openzt2_effect_parameter_type, openzt2_effect_pass_count, openzt2_effect_technique_count,
        openzt2_effect_technique_float_annotation, openzt2_effect_technique_is_valid,
        openzt2_effect_technique_name,
    },
};

use super::{
    evaluated_pass_copying::copy_evaluated_d3d9_effect_pass_from_mojoshader,
    mojoshader_effect_allocation::{
        copy_optional_mojoshader_string, copy_required_mojoshader_string,
        MojoShaderEffectAllocationOwner,
    },
    retained_transform_evaluation::RetainedD3d9EffectProgram,
};

#[allow(
    unsafe_code,
    reason = "copies evaluated values from the MojoShader Effects ABI"
)]
pub(super) fn copy_evaluated_d3d9_effect_from_mojoshader(
    effect_allocation: &MojoShaderEffectAllocationOwner,
    retained_program: &RetainedD3d9EffectProgram,
) -> Result<EvaluatedD3d9Effect, D3d9EffectProcessingError> {
    let effect_parameter_count =
        unsafe { openzt2_effect_parameter_count(effect_allocation.native_effect_pointer) };
    let parameter_descriptions = (0..effect_parameter_count)
        .map(|parameter_index| unsafe {
            Ok(D3d9EffectParameterDescription {
                parameter_name: copy_required_mojoshader_string(openzt2_effect_parameter_name(
                    effect_allocation.native_effect_pointer,
                    parameter_index,
                ))?,
                semantic_name: copy_optional_mojoshader_string(openzt2_effect_parameter_semantic(
                    effect_allocation.native_effect_pointer,
                    parameter_index,
                )),
                parameter_class: openzt2_effect_parameter_class(
                    effect_allocation.native_effect_pointer,
                    parameter_index,
                ),
                parameter_type: openzt2_effect_parameter_type(
                    effect_allocation.native_effect_pointer,
                    parameter_index,
                ),
                row_count: openzt2_effect_parameter_rows(
                    effect_allocation.native_effect_pointer,
                    parameter_index,
                ),
                column_count: openzt2_effect_parameter_columns(
                    effect_allocation.native_effect_pointer,
                    parameter_index,
                ),
                array_element_count: openzt2_effect_parameter_elements(
                    effect_allocation.native_effect_pointer,
                    parameter_index,
                ),
                annotation_count: openzt2_effect_parameter_annotations(
                    effect_allocation.native_effect_pointer,
                    parameter_index,
                ),
                struct_members: (0..openzt2_effect_parameter_member_count(
                    effect_allocation.native_effect_pointer,
                    parameter_index,
                ))
                    .map(|member_index| {
                        let mut layout = [0; 6];
                        let member_name =
                            copy_required_mojoshader_string(openzt2_effect_parameter_member(
                                effect_allocation.native_effect_pointer,
                                parameter_index,
                                member_index,
                                layout.as_mut_ptr(),
                            ))?;
                        Ok(D3d9EffectStructMemberDescription {
                            member_name,
                            parameter_class: layout[0],
                            parameter_type: layout[1],
                            row_count: layout[2],
                            column_count: layout[3],
                            array_element_count: layout[4],
                            native_component_offset: layout[5],
                        })
                    })
                    .collect::<Result<_, D3d9EffectProcessingError>>()?,
            })
        })
        .collect::<Result<_, D3d9EffectProcessingError>>()?;
    let effect_technique_count =
        unsafe { openzt2_effect_technique_count(effect_allocation.native_effect_pointer) };
    let evaluated_techniques = (0..effect_technique_count)
        .map(|technique_index| {
            copy_evaluated_d3d9_effect_technique_from_mojoshader(
                effect_allocation,
                retained_program,
                technique_index,
            )
        })
        .collect::<Result<_, _>>()?;
    Ok(EvaluatedD3d9Effect {
        parameter_descriptions,
        evaluated_techniques,
    })
}

#[allow(
    unsafe_code,
    reason = "copies one evaluated technique from the MojoShader ABI"
)]
fn copy_evaluated_d3d9_effect_technique_from_mojoshader(
    effect_allocation: &MojoShaderEffectAllocationOwner,
    retained_program: &RetainedD3d9EffectProgram,
    technique_index: u32,
) -> Result<EvaluatedD3d9EffectTechnique, D3d9EffectProcessingError> {
    let mut quality_annotation = 0.0;
    let quality_annotation_name = c"Quality";
    let is_valid = unsafe {
        openzt2_effect_technique_is_valid(effect_allocation.native_effect_pointer, technique_index)
            != 0
    };
    let technique_pass_count = unsafe {
        openzt2_effect_pass_count(effect_allocation.native_effect_pointer, technique_index)
    };
    let evaluated_passes = (0..technique_pass_count)
        .filter(|_| is_valid)
        .map(|pass_index| {
            let began_pass = unsafe {
                openzt2_effect_begin_pass(
                    effect_allocation.native_effect_pointer,
                    technique_index,
                    pass_index,
                )
            };
            if began_pass == 0 {
                return Err(D3d9EffectProcessingError::CompiledEffectEvaluationFailed(
                    unsafe {
                        copy_required_mojoshader_string(openzt2_effect_execution_error(
                            effect_allocation.native_effect_pointer,
                        ))?
                    },
                ));
            }
            let evaluated_pass = copy_evaluated_d3d9_effect_pass_from_mojoshader(
                effect_allocation,
                retained_program,
                technique_index,
                pass_index,
            );
            unsafe { openzt2_effect_end_pass(effect_allocation.native_effect_pointer) };
            evaluated_pass
        })
        .collect::<Result<_, _>>()?;
    Ok(EvaluatedD3d9EffectTechnique {
        technique_name: unsafe {
            copy_required_mojoshader_string(openzt2_effect_technique_name(
                effect_allocation.native_effect_pointer,
                technique_index,
            ))?
        },
        is_valid,
        quality_annotation: (unsafe {
            openzt2_effect_technique_float_annotation(
                effect_allocation.native_effect_pointer,
                technique_index,
                quality_annotation_name.as_ptr(),
                &raw mut quality_annotation,
            )
        } != 0)
            .then_some(quality_annotation)
            .filter(|quality| quality.is_finite()),
        evaluated_passes,
    })
}
