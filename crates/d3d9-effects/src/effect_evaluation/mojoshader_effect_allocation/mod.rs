use std::{
    ffi::{c_char, CStr},
    ptr,
};

use crate::{
    effect_types::CompiledD3d9EffectBytecode,
    error::D3d9EffectProcessingError,
    native_abi::{
        openzt2_effect_close, openzt2_effect_open, openzt2_shader_free_message, NativeEffect,
    },
};

pub(super) struct MojoShaderEffectAllocationOwner {
    pub(super) native_effect_pointer: *mut NativeEffect,
}

#[allow(
    unsafe_code,
    reason = "owns the synchronous MojoShader Effects allocation"
)]
impl Drop for MojoShaderEffectAllocationOwner {
    fn drop(&mut self) {
        unsafe { openzt2_effect_close(self.native_effect_pointer) };
    }
}

#[allow(
    unsafe_code,
    reason = "opens and owns one MojoShader Effects allocation"
)]
pub(super) fn open_compiled_d3d9_effect_bytecode_with_mojoshader(
    compiled_effect_bytecode: &CompiledD3d9EffectBytecode,
) -> Result<MojoShaderEffectAllocationOwner, D3d9EffectProcessingError> {
    let mut native_evaluation_diagnostic_pointer = ptr::null_mut();
    let effect_allocation = MojoShaderEffectAllocationOwner {
        native_effect_pointer: unsafe {
            openzt2_effect_open(
                compiled_effect_bytecode.compiled_effect_bytecode().as_ptr(),
                compiled_effect_bytecode.compiled_effect_bytecode().len(),
                &raw mut native_evaluation_diagnostic_pointer,
            )
        },
    };
    if !effect_allocation.native_effect_pointer.is_null() {
        return Ok(effect_allocation);
    }
    let evaluation_diagnostic = if native_evaluation_diagnostic_pointer.is_null() {
        "MojoShader returned no diagnostic".to_owned()
    } else {
        let evaluation_diagnostic = unsafe { CStr::from_ptr(native_evaluation_diagnostic_pointer) }
            .to_string_lossy()
            .into_owned();
        unsafe { openzt2_shader_free_message(native_evaluation_diagnostic_pointer) };
        evaluation_diagnostic
    };
    Err(D3d9EffectProcessingError::CompiledEffectEvaluationFailed(
        evaluation_diagnostic,
    ))
}

#[allow(unsafe_code, reason = "copies a backend shader retained by MojoShader")]
pub(super) unsafe fn copy_mojoshader_retained_shader_bytecode(
    native_effect_pointer: *const NativeEffect,
    get_shader_bytecode: unsafe extern "C" fn(*const NativeEffect, *mut usize) -> *const u8,
) -> Option<Box<[u8]>> {
    let mut shader_bytecode_size = 0;
    let shader_bytecode_pointer =
        unsafe { get_shader_bytecode(native_effect_pointer, &raw mut shader_bytecode_size) };
    (!shader_bytecode_pointer.is_null() && shader_bytecode_size != 0).then(|| unsafe {
        std::slice::from_raw_parts(shader_bytecode_pointer, shader_bytecode_size).into()
    })
}

#[allow(unsafe_code, reason = "copies a NUL-terminated MojoShader string")]
pub(super) unsafe fn copy_required_mojoshader_string(
    native_string_pointer: *const c_char,
) -> Result<String, D3d9EffectProcessingError> {
    (!native_string_pointer.is_null())
        .then(|| unsafe {
            CStr::from_ptr(native_string_pointer)
                .to_string_lossy()
                .into_owned()
        })
        .ok_or(D3d9EffectProcessingError::NativeDependencyReturnedMalformedOutput)
}

#[allow(
    unsafe_code,
    reason = "copies an optional NUL-terminated MojoShader string"
)]
pub(super) unsafe fn copy_optional_mojoshader_string(
    native_string_pointer: *const c_char,
) -> Option<String> {
    (!native_string_pointer.is_null()).then(|| unsafe {
        CStr::from_ptr(native_string_pointer)
            .to_string_lossy()
            .into_owned()
    })
}

#[cfg(test)]
mod tests;
