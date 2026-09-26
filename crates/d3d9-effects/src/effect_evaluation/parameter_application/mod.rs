use std::ffi::CString;

use crate::{
    effect_types::{D3d9EffectParameterAssignment, D3d9EffectParameterValue},
    error::D3d9EffectProcessingError,
    native_abi::{openzt2_effect_set_raw, NativeEffect},
};

#[allow(
    unsafe_code,
    reason = "copies parameter bytes into the synchronous MojoShader evaluator"
)]
pub(super) fn apply_d3d9_effect_parameter_assignments_to_mojoshader_effect(
    native_effect_pointer: *mut NativeEffect,
    parameter_assignments: &[D3d9EffectParameterAssignment<'_>],
) -> Result<(), D3d9EffectProcessingError> {
    for parameter_assignment in parameter_assignments {
        let native_parameter_name = CString::new(parameter_assignment.parameter_name)
            .map_err(|_| D3d9EffectProcessingError::NativeDependencyReturnedMalformedOutput)?;
        let (parameter_data_bytes, parameter_byte_count) =
            convert_d3d9_effect_parameter_value_to_native_bytes(
                &parameter_assignment.parameter_value,
            )?;
        if let Some(parameter_data_bytes) = parameter_data_bytes {
            unsafe {
                openzt2_effect_set_raw(
                    native_effect_pointer,
                    native_parameter_name.as_ptr(),
                    parameter_data_bytes.as_ptr().cast(),
                    parameter_byte_count,
                );
            }
        }
    }
    Ok(())
}

fn convert_d3d9_effect_parameter_value_to_native_bytes(
    parameter_value: &D3d9EffectParameterValue<'_>,
) -> Result<(Option<Vec<u8>>, u32), D3d9EffectProcessingError> {
    let parameter_data_bytes = match parameter_value {
        D3d9EffectParameterValue::Boolean { boolean_value } => {
            i32::from(*boolean_value).to_ne_bytes().to_vec()
        }
        D3d9EffectParameterValue::Integer { integer_value } => integer_value.to_ne_bytes().to_vec(),
        D3d9EffectParameterValue::FloatingPoint {
            floating_point_value,
        } => floating_point_value.to_ne_bytes().to_vec(),
        D3d9EffectParameterValue::FloatVector { vector_components }
        | D3d9EffectParameterValue::FloatMatrix {
            matrix_components: vector_components,
        } => vector_components
            .iter()
            .flat_map(|vector_component| vector_component.to_ne_bytes())
            .collect(),
        D3d9EffectParameterValue::TextureParameterReference { .. } => return Ok((None, 0)),
    };
    let parameter_byte_count = u32::try_from(parameter_data_bytes.len())
        .map_err(|_| D3d9EffectProcessingError::NativeDependencyReturnedMalformedOutput)?;
    Ok((Some(parameter_data_bytes), parameter_byte_count))
}
