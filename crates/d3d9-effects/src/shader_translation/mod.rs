use std::ffi::{c_int, CStr};

use crate::{
    error::D3d9EffectProcessingError,
    native_abi::{
        openzt2_assembled_shader_bytecode_size, openzt2_shader_assemble, openzt2_shader_code,
        openzt2_shader_code_size, openzt2_shader_free, openzt2_shader_free_message,
        openzt2_shader_input_count, openzt2_shader_input_index, openzt2_shader_input_usage,
        openzt2_shader_link, openzt2_shader_packed_uniform_source, openzt2_shader_symbol_count,
        openzt2_shader_symbol_is_row_major_matrix, openzt2_shader_symbol_name,
        openzt2_shader_symbol_register_count, openzt2_shader_symbol_register_index,
        openzt2_shader_symbol_register_set, openzt2_shader_translate,
        openzt2_shader_vertex_output_count, openzt2_shader_vertex_output_index,
        openzt2_shader_vertex_output_location, openzt2_shader_vertex_output_usage,
        NativeShaderResult,
    },
    shader_types::{
        D3d9ShaderBindingLayout, D3d9ShaderInputSignatureElement, D3d9ShaderUniformRegisterBinding,
        D3d9ShaderVertexOutputSignatureElement, SpirvShaderTranslation,
    },
};

/// Translates one D3D9 shader token stream to SPIR-V through `MojoShader`.
///
/// # Errors
///
/// Returns the dependency's diagnostic when the bytecode is invalid or uses
/// an unsupported instruction.
#[allow(
    unsafe_code,
    clippy::cast_possible_truncation,
    reason = "copies the complete result from the MojoShader ABI; its input count is an unsigned int"
)]
pub fn translate_d3d9_shader_bytecode_to_spirv(
    d3d9_shader_bytecode: &[u8],
    binding_layout: &D3d9ShaderBindingLayout,
) -> Result<SpirvShaderTranslation, D3d9EffectProcessingError> {
    translate_d3d9_shader_bytecode_through_mojoshader_native_abi(d3d9_shader_bytecode).and_then(
        |result| copy_spirv_translation_and_release_mojoshader_result(result, binding_layout),
    )
}

#[allow(
    unsafe_code,
    reason = "narrow ownership handoff through the MojoShader translation ABI"
)]
fn translate_d3d9_shader_bytecode_through_mojoshader_native_abi(
    d3d9_shader_bytecode: &[u8],
) -> Result<NativeShaderResult, D3d9EffectProcessingError> {
    let native_shader_result = unsafe {
        openzt2_shader_translate(d3d9_shader_bytecode.as_ptr(), d3d9_shader_bytecode.len())
    };
    if native_shader_result.status != 0 || native_shader_result.shader.is_null() {
        let translation_diagnostic = if native_shader_result.message.is_null() {
            "MojoShader returned no diagnostic".to_owned()
        } else {
            let translation_diagnostic = unsafe { CStr::from_ptr(native_shader_result.message) }
                .to_string_lossy()
                .into_owned();
            unsafe { openzt2_shader_free_message(native_shader_result.message) };
            translation_diagnostic
        };
        return Err(D3d9EffectProcessingError::D3d9ShaderTranslationFailed(
            translation_diagnostic,
        ));
    }

    Ok(native_shader_result)
}

#[allow(
    unsafe_code,
    clippy::cast_possible_truncation,
    clippy::needless_pass_by_value,
    reason = "takes ownership so the MojoShader result cannot be used after it is released"
)]
fn copy_spirv_translation_and_release_mojoshader_result(
    native_shader_result: NativeShaderResult,
    binding_layout: &D3d9ShaderBindingLayout,
) -> Result<SpirvShaderTranslation, D3d9EffectProcessingError> {
    let spirv_bytecode_pointer = unsafe { openzt2_shader_code(native_shader_result.shader) };
    let spirv_bytecode_size = unsafe { openzt2_shader_code_size(native_shader_result.shader) };
    if spirv_bytecode_pointer.is_null() || spirv_bytecode_size == 0 {
        unsafe { openzt2_shader_free(native_shader_result.shader) };
        return Err(D3d9EffectProcessingError::NativeDependencyReturnedMalformedOutput);
    }
    let spirv_bytecode =
        unsafe { std::slice::from_raw_parts(spirv_bytecode_pointer, spirv_bytecode_size) }.to_vec();
    let input_signature_elements =
        (0..unsafe { openzt2_shader_input_count(native_shader_result.shader) })
            .map(|input_register| D3d9ShaderInputSignatureElement {
                semantic_name: d3d9_declaration_usage_semantic_name(unsafe {
                    openzt2_shader_input_usage(native_shader_result.shader, input_register)
                })
                .to_owned(),
                semantic_index: unsafe {
                    openzt2_shader_input_index(native_shader_result.shader, input_register)
                },
                input_register: input_register as u32,
            })
            .collect();
    let vertex_output_signature_elements =
        (0..unsafe { openzt2_shader_vertex_output_count(native_shader_result.shader) })
            .filter_map(|index| {
                let location = u32::try_from(unsafe {
                    openzt2_shader_vertex_output_location(native_shader_result.shader, index)
                })
                .ok()?;
                let usage = unsafe {
                    openzt2_shader_vertex_output_usage(native_shader_result.shader, index)
                };
                Some(D3d9ShaderVertexOutputSignatureElement {
                    semantic_name: d3d9_declaration_usage_semantic_name(usage).to_owned(),
                    semantic_index: unsafe {
                        openzt2_shader_vertex_output_index(native_shader_result.shader, index)
                    },
                    location,
                    // MojoShader's SPIR-V profile emits scalar fog and
                    // float4 texture/colour output registers.
                    component_count: if usage == 11 { 1 } else { 4 },
                })
            })
            .collect();
    let uniform_register_bindings =
        (0..unsafe { openzt2_shader_symbol_count(native_shader_result.shader) })
            .filter_map(|symbol_index| {
                let parameter_name_pointer = unsafe {
                    openzt2_shader_symbol_name(native_shader_result.shader, symbol_index)
                };
                if parameter_name_pointer.is_null() {
                    return None;
                }
                let parameter_name = unsafe { CStr::from_ptr(parameter_name_pointer) }
                    .to_string_lossy()
                    .into_owned();
                let register_set = unsafe {
                    openzt2_shader_symbol_register_set(native_shader_result.shader, symbol_index)
                };
                (register_set < 3).then(|| D3d9ShaderUniformRegisterBinding {
                    is_row_major_matrix: unsafe {
                        openzt2_shader_symbol_is_row_major_matrix(
                            native_shader_result.shader,
                            symbol_index,
                        ) != 0
                    },
                    parameter_name,
                    register_set,
                    first_register: unsafe {
                        openzt2_shader_symbol_register_index(
                            native_shader_result.shader,
                            symbol_index,
                        )
                    },
                    register_count: unsafe {
                        openzt2_shader_symbol_register_count(
                            native_shader_result.shader,
                            symbol_index,
                        )
                    },
                })
            })
            .collect();
    let uniform_register_upload_order =
        copy_packed_uniform_register_upload_order(native_shader_result.shader);
    unsafe { openzt2_shader_free(native_shader_result.shader) };
    let spirv_bytecode = crate::spirv_separate_sampler_bindings::separate_combined_shader_samplers(
        &spirv_bytecode,
        binding_layout,
    )?;
    Ok(SpirvShaderTranslation {
        uniform_register_upload_order,
        spirv_bytecode,
        input_signature_elements,
        vertex_output_signature_elements,
        uniform_register_bindings,
    })
}

#[allow(
    unsafe_code,
    reason = "copies register metadata before the native shader is released"
)]
fn copy_packed_uniform_register_upload_order(shader: *const std::ffi::c_void) -> [Vec<u32>; 3] {
    [0, 1, 2].map(|bank| {
        (0..256)
            .map(|index| unsafe { openzt2_shader_packed_uniform_source(shader, bank, index) })
            .take_while(|index| *index != u32::MAX)
            .collect()
    })
}

/// Translates and links one D3D9 vertex/pixel shader pair.
///
/// # Errors
///
/// Returns a translator diagnostic or a link error for incompatible signatures.
#[allow(
    unsafe_code,
    reason = "links and releases two results through the MojoShader ABI"
)]
pub fn translate_and_link_d3d9_vertex_and_pixel_shader_bytecode_to_spirv(
    vertex_shader_bytecode: &[u8],
    pixel_shader_bytecode: &[u8],
    binding_layout: &D3d9ShaderBindingLayout,
) -> Result<(SpirvShaderTranslation, SpirvShaderTranslation), D3d9EffectProcessingError> {
    let native_vertex_shader_result =
        translate_d3d9_shader_bytecode_through_mojoshader_native_abi(vertex_shader_bytecode)?;
    let native_pixel_shader_result =
        translate_d3d9_shader_bytecode_through_mojoshader_native_abi(pixel_shader_bytecode)
            .inspect_err(|_| unsafe {
                openzt2_shader_free(native_vertex_shader_result.shader);
            })?;
    if unsafe {
        openzt2_shader_link(
            native_vertex_shader_result.shader,
            native_pixel_shader_result.shader,
        )
    } != 0
    {
        unsafe {
            openzt2_shader_free(native_vertex_shader_result.shader);
            openzt2_shader_free(native_pixel_shader_result.shader);
        }
        return Err(D3d9EffectProcessingError::D3d9ShaderTranslationFailed(
            "MojoShader could not link the shader pair".to_owned(),
        ));
    }
    let translated_vertex_shader = copy_spirv_translation_and_release_mojoshader_result(
        native_vertex_shader_result,
        binding_layout,
    )
    .inspect_err(|_| unsafe {
        openzt2_shader_free(native_pixel_shader_result.shader);
    })?;
    Ok((
        translated_vertex_shader,
        copy_spirv_translation_and_release_mojoshader_result(
            native_pixel_shader_result,
            binding_layout,
        )?,
    ))
}

/// Assembles one legacy D3D shader assembly program through `MojoShader`.
///
/// # Errors
///
/// Returns the assembler diagnostic when the source is invalid.
#[allow(
    unsafe_code,
    reason = "copies the complete result from the MojoShader ABI"
)]
pub fn assemble_d3d9_shader_assembly_source_to_bytecode(
    shader_assembly_source: &[u8],
) -> Result<Box<[u8]>, D3d9EffectProcessingError> {
    let native_assembly_result = unsafe {
        openzt2_shader_assemble(
            shader_assembly_source.as_ptr(),
            shader_assembly_source.len(),
        )
    };
    if native_assembly_result.status != 0 || native_assembly_result.shader.is_null() {
        let assembly_diagnostic = if native_assembly_result.message.is_null() {
            "MojoShader returned no diagnostic".to_owned()
        } else {
            let assembly_diagnostic = unsafe { CStr::from_ptr(native_assembly_result.message) }
                .to_string_lossy()
                .into_owned();
            unsafe { openzt2_shader_free_message(native_assembly_result.message) };
            assembly_diagnostic
        };
        return Err(D3d9EffectProcessingError::D3d9ShaderTranslationFailed(
            assembly_diagnostic,
        ));
    }
    let assembled_bytecode_pointer = unsafe { openzt2_shader_code(native_assembly_result.shader) };
    let assembled_bytecode_size =
        unsafe { openzt2_assembled_shader_bytecode_size(native_assembly_result.shader) };
    if assembled_bytecode_pointer.is_null() || assembled_bytecode_size == 0 {
        unsafe { openzt2_shader_free(native_assembly_result.shader) };
        return Err(D3d9EffectProcessingError::NativeDependencyReturnedMalformedOutput);
    }
    let assembled_shader_bytecode =
        unsafe { std::slice::from_raw_parts(assembled_bytecode_pointer, assembled_bytecode_size) }
            .into();
    unsafe { openzt2_shader_free(native_assembly_result.shader) };
    Ok(assembled_shader_bytecode)
}

const fn d3d9_declaration_usage_semantic_name(declaration_usage: c_int) -> &'static str {
    match declaration_usage {
        0 => "POSITION",
        1 => "BLENDWEIGHT",
        2 => "BLENDINDICES",
        3 => "NORMAL",
        4 => "PSIZE",
        5 => "TEXCOORD",
        6 => "TANGENT",
        7 => "BINORMAL",
        10 => "COLOR",
        11 => "FOG",
        _ => "UNKNOWN",
    }
}
