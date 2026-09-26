use std::{
    collections::HashMap,
    ffi::{c_char, c_int, c_void, CStr, CString},
    path::{Path, PathBuf},
    ptr,
};

use crate::{
    effect_types::{CompiledD3d9EffectBytecode, D3d9EffectIncludeResolver},
    error::D3d9EffectProcessingError,
    inline_assembly,
    native_abi::{
        openzt2_effect_compile, openzt2_effect_free_code, openzt2_effect_free_messages,
        NativeCompiler, Slice,
    },
};

struct OpenD3d9EffectIncludeSource {
    resolved_include_path: PathBuf,
    include_source_bytes: Box<[u8]>,
}

struct D3d9EffectCompilationIncludeContext<'a> {
    root_effect_path: &'a Path,
    root_effect_source_pointer: *const u8,
    include_resolver: &'a mut dyn D3d9EffectIncludeResolver,
    open_include_sources_by_data_pointer: HashMap<*const u8, OpenD3d9EffectIncludeSource>,
    include_callback_error: Option<D3d9EffectProcessingError>,
}

#[allow(
    unsafe_code,
    reason = "callback borrows the CompileContext for the synchronous native call"
)]
unsafe extern "C" fn open_d3d9_effect_include_for_vkd3d_shader_callback(
    opaque_compilation_context: *mut c_void,
    requested_include_filename: *const c_char,
    parent_source_data_pointer: *const c_char,
    output_include_source: *mut Slice,
) -> c_int {
    let compilation_context = unsafe {
        &mut *opaque_compilation_context.cast::<D3d9EffectCompilationIncludeContext<'_>>()
    };
    let requested_include_filename = unsafe { CStr::from_ptr(requested_include_filename) };
    let Ok(requested_include_filename) = requested_include_filename.to_str() else {
        compilation_context.include_callback_error =
            Some(D3d9EffectProcessingError::NativeDependencyReturnedMalformedOutput);
        return 0;
    };
    let parent_source_data_pointer = parent_source_data_pointer.cast::<u8>();
    let parent_effect_path = if parent_source_data_pointer.is_null()
        || parent_source_data_pointer == compilation_context.root_effect_source_pointer
    {
        compilation_context.root_effect_path
    } else if let Some(open_include_source) = compilation_context
        .open_include_sources_by_data_pointer
        .get(&parent_source_data_pointer)
    {
        &open_include_source.resolved_include_path
    } else {
        compilation_context.include_callback_error =
            Some(D3d9EffectProcessingError::NativeDependencyReturnedMalformedOutput);
        return 0;
    };
    match compilation_context
        .include_resolver
        .open(parent_effect_path, Path::new(requested_include_filename))
    {
        Ok((resolved_include_path, include_source_bytes)) => {
            let open_include_source = OpenD3d9EffectIncludeSource {
                resolved_include_path,
                include_source_bytes: include_source_bytes.into_boxed_slice(),
            };
            let include_source_data_pointer = open_include_source.include_source_bytes.as_ptr();
            unsafe {
                output_include_source.write(Slice {
                    data: include_source_data_pointer,
                    size: open_include_source.include_source_bytes.len(),
                });
            };
            compilation_context
                .open_include_sources_by_data_pointer
                .insert(include_source_data_pointer, open_include_source);
            1
        }
        Err(include_resolution_error) => {
            compilation_context.include_callback_error = Some(include_resolution_error);
            0
        }
    }
}

#[allow(
    unsafe_code,
    reason = "callback borrows the CompileContext for the synchronous native call"
)]
unsafe extern "C" fn close_d3d9_effect_include_for_vkd3d_shader_callback(
    opaque_compilation_context: *mut c_void,
    include_source_data_pointer: *const u8,
    _: usize,
) {
    let compilation_context = unsafe {
        &mut *opaque_compilation_context.cast::<D3d9EffectCompilationIncludeContext<'_>>()
    };
    compilation_context
        .open_include_sources_by_data_pointer
        .remove(&include_source_data_pointer);
}

/// Compiles one complete Effects source document through vkd3d-shader.
///
/// # Errors
///
/// Returns an include error or the complete vkd3d compiler diagnostics.
#[allow(
    unsafe_code,
    reason = "copies and frees all output from the narrow native compiler ABI"
)]
pub fn compile_d3d9_effect_source_to_fx2_bytecode(
    effect_path: &Path,
    effect_source: &[u8],
    include_resolver: &mut dyn D3d9EffectIncludeResolver,
) -> Result<CompiledD3d9EffectBytecode, D3d9EffectProcessingError> {
    let rewritten_effect_source =
        inline_assembly::rewrite_inline_d3d9_shader_assembly_blocks_for_fx2_compilation(
            effect_source,
        )?;
    let rewritten_effect_source_bytes = rewritten_effect_source.rewritten_effect_source.as_slice();
    let effect_path_text = effect_path.to_string_lossy();
    let native_effect_path = CString::new(effect_path_text.as_bytes())
        .map_err(|_| D3d9EffectProcessingError::NativeDependencyReturnedMalformedOutput)?;
    let mut include_context = D3d9EffectCompilationIncludeContext {
        root_effect_path: effect_path,
        root_effect_source_pointer: rewritten_effect_source_bytes.as_ptr(),
        include_resolver,
        open_include_sources_by_data_pointer: HashMap::new(),
        include_callback_error: None,
    };
    let mut native_effect_compiler = NativeCompiler {
        context: ptr::from_mut(&mut include_context).cast(),
        open: open_d3d9_effect_include_for_vkd3d_shader_callback,
        close: close_d3d9_effect_include_for_vkd3d_shader_callback,
    };
    let native_compilation_result = unsafe {
        openzt2_effect_compile(
            native_effect_path.as_ptr(),
            rewritten_effect_source_bytes.as_ptr(),
            rewritten_effect_source_bytes.len(),
            ptr::from_mut(&mut native_effect_compiler),
        )
    };
    let compilation_diagnostics = if native_compilation_result.messages.is_null() {
        String::new()
    } else {
        let compilation_diagnostics = unsafe { CStr::from_ptr(native_compilation_result.messages) }
            .to_string_lossy()
            .into_owned();
        unsafe { openzt2_effect_free_messages(native_compilation_result.messages) };
        compilation_diagnostics
    };
    if let Some(include_callback_error) = include_context.include_callback_error {
        if !native_compilation_result.code.is_null() {
            unsafe {
                openzt2_effect_free_code(
                    native_compilation_result.code,
                    native_compilation_result.code_size,
                );
            };
        }
        return Err(include_callback_error);
    }
    if native_compilation_result.status != 0 {
        if !native_compilation_result.code.is_null() {
            unsafe {
                openzt2_effect_free_code(
                    native_compilation_result.code,
                    native_compilation_result.code_size,
                );
            };
        }
        return Err(
            D3d9EffectProcessingError::Vkd3dShaderEffectCompilationFailed(compilation_diagnostics),
        );
    }
    if native_compilation_result.code.is_null() {
        return Err(D3d9EffectProcessingError::NativeDependencyReturnedMalformedOutput);
    }
    let compiled_effect_bytecode = unsafe {
        std::slice::from_raw_parts(
            native_compilation_result.code,
            native_compilation_result.code_size,
        )
    }
    .to_vec();
    unsafe {
        openzt2_effect_free_code(
            native_compilation_result.code,
            native_compilation_result.code_size,
        );
    };
    Ok(CompiledD3d9EffectBytecode(
        rewritten_effect_source
            .replace_placeholder_shaders_with_compiled_inline_shader_bytecode(
                compiled_effect_bytecode,
            )?
            .into_boxed_slice(),
    ))
}
