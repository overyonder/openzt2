use std::ffi::{c_char, c_int, c_void};

#[repr(C)]
#[allow(
    clippy::redundant_pub_crate,
    reason = "native ABI records are crate-private, never part of the external API"
)]
pub(crate) struct NativeShaderResult {
    pub(crate) status: c_int,
    pub(crate) shader: *const c_void,
    pub(crate) message: *mut c_char,
}

#[repr(C)]
#[allow(
    clippy::redundant_pub_crate,
    reason = "native ABI records are crate-private, never part of the external API"
)]
pub(crate) struct NativeEffect {
    pub(crate) _private: [u8; 0],
}

#[repr(C)]
#[allow(
    clippy::redundant_pub_crate,
    reason = "native ABI records are crate-private, never part of the external API"
)]
pub(crate) struct Slice {
    pub(crate) data: *const u8,
    pub(crate) size: usize,
}

type OpenInclude =
    unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char, *mut Slice) -> c_int;
type CloseInclude = unsafe extern "C" fn(*mut c_void, *const u8, usize);

#[repr(C)]
#[allow(
    clippy::redundant_pub_crate,
    reason = "native ABI records are crate-private, never part of the external API"
)]
pub(crate) struct NativeCompiler {
    pub(crate) context: *mut c_void,
    pub(crate) open: OpenInclude,
    pub(crate) close: CloseInclude,
}

#[repr(C)]
#[allow(
    clippy::redundant_pub_crate,
    reason = "native ABI records are crate-private, never part of the external API"
)]
pub(crate) struct NativeResult {
    pub(crate) status: c_int,
    pub(crate) code: *const u8,
    pub(crate) code_size: usize,
    pub(crate) messages: *mut c_char,
}

#[allow(
    unsafe_code,
    reason = "narrow ABI to the dependency-owned vkd3d-shader compiler"
)]
unsafe extern "C" {
    pub(crate) fn openzt2_effect_compile(
        path: *const c_char,
        source: *const u8,
        source_size: usize,
        compiler: *mut NativeCompiler,
    ) -> NativeResult;
    pub(crate) fn openzt2_effect_free_code(code: *const u8, size: usize);
    pub(crate) fn openzt2_effect_free_messages(messages: *mut c_char);
    pub(crate) fn openzt2_shader_translate(bytecode: *const u8, size: usize) -> NativeShaderResult;
    pub(crate) fn openzt2_shader_link(vertex: *const c_void, pixel: *const c_void) -> c_int;
    pub(crate) fn openzt2_shader_assemble(source: *const u8, size: usize) -> NativeShaderResult;
    pub(crate) fn openzt2_shader_code(shader: *const c_void) -> *const u8;
    pub(crate) fn openzt2_shader_code_size(shader: *const c_void) -> usize;
    pub(crate) fn openzt2_assembled_shader_bytecode_size(shader: *const c_void) -> usize;
    pub(crate) fn openzt2_shader_input_count(shader: *const c_void) -> usize;
    pub(crate) fn openzt2_shader_input_usage(shader: *const c_void, index: usize) -> c_int;
    pub(crate) fn openzt2_shader_input_index(shader: *const c_void, index: usize) -> u32;
    pub(crate) fn openzt2_shader_vertex_output_count(shader: *const c_void) -> usize;
    pub(crate) fn openzt2_shader_vertex_output_usage(shader: *const c_void, index: usize) -> c_int;
    pub(crate) fn openzt2_shader_vertex_output_index(shader: *const c_void, index: usize) -> u32;
    pub(crate) fn openzt2_shader_vertex_output_location(
        shader: *const c_void,
        index: usize,
    ) -> c_int;
    pub(crate) fn openzt2_shader_symbol_count(shader: *const c_void) -> usize;
    pub(crate) fn openzt2_shader_packed_uniform_source(
        shader: *const c_void,
        bank: u32,
        index: u32,
    ) -> u32;
    pub(crate) fn openzt2_shader_symbol_name(shader: *const c_void, index: usize) -> *const c_char;
    pub(crate) fn openzt2_shader_symbol_register_set(shader: *const c_void, index: usize) -> u32;
    pub(crate) fn openzt2_shader_symbol_register_index(shader: *const c_void, index: usize) -> u32;
    pub(crate) fn openzt2_shader_symbol_register_count(shader: *const c_void, index: usize) -> u32;
    pub(crate) fn openzt2_shader_symbol_is_row_major_matrix(
        shader: *const c_void,
        index: usize,
    ) -> u32;
    pub(crate) fn openzt2_shader_free(shader: *const c_void);
    pub(crate) fn openzt2_shader_free_message(message: *mut c_char);
    pub(crate) fn openzt2_effect_open(
        bytecode: *const u8,
        size: usize,
        message: *mut *mut c_char,
    ) -> *mut NativeEffect;
    pub(crate) fn openzt2_effect_close(effect: *mut NativeEffect);
    pub(crate) fn openzt2_effect_set_raw(
        effect: *mut NativeEffect,
        name: *const c_char,
        data: *const c_void,
        size: u32,
    );
    pub(crate) fn openzt2_effect_parameter_count(effect: *const NativeEffect) -> u32;
    pub(crate) fn openzt2_effect_parameter_name(
        effect: *const NativeEffect,
        index: u32,
    ) -> *const c_char;
    pub(crate) fn openzt2_effect_parameter_semantic(
        effect: *const NativeEffect,
        index: u32,
    ) -> *const c_char;
    pub(crate) fn openzt2_effect_parameter_class(effect: *const NativeEffect, index: u32) -> u32;
    pub(crate) fn openzt2_effect_parameter_type(effect: *const NativeEffect, index: u32) -> u32;
    pub(crate) fn openzt2_effect_parameter_rows(effect: *const NativeEffect, index: u32) -> u32;
    pub(crate) fn openzt2_effect_parameter_columns(effect: *const NativeEffect, index: u32) -> u32;
    pub(crate) fn openzt2_effect_parameter_elements(effect: *const NativeEffect, index: u32)
        -> u32;
    pub(crate) fn openzt2_effect_parameter_annotations(
        effect: *const NativeEffect,
        index: u32,
    ) -> u32;
    pub(crate) fn openzt2_effect_technique_count(effect: *const NativeEffect) -> u32;
    pub(crate) fn openzt2_effect_technique_name(
        effect: *const NativeEffect,
        technique: u32,
    ) -> *const c_char;
    pub(crate) fn openzt2_effect_technique_is_valid(
        effect: *const NativeEffect,
        technique: u32,
    ) -> c_int;
    pub(crate) fn openzt2_effect_technique_float_annotation(
        effect: *const NativeEffect,
        technique: u32,
        name: *const c_char,
        value: *mut f32,
    ) -> c_int;
    pub(crate) fn openzt2_effect_pass_count(effect: *const NativeEffect, technique: u32) -> u32;
    pub(crate) fn openzt2_effect_pass_name(
        effect: *const NativeEffect,
        technique: u32,
        pass: u32,
    ) -> *const c_char;
    pub(crate) fn openzt2_effect_begin_pass(effect: *mut NativeEffect, technique: u32, pass: u32);
    pub(crate) fn openzt2_effect_end_pass(effect: *mut NativeEffect);
    pub(crate) fn openzt2_effect_state_count(effect: *const NativeEffect) -> u32;
    pub(crate) fn openzt2_effect_state_type(effect: *const NativeEffect, index: u32) -> u32;
    pub(crate) fn openzt2_effect_state_index(effect: *const NativeEffect, index: u32) -> u32;
    pub(crate) fn openzt2_effect_state_value(effect: *const NativeEffect, index: u32) -> u32;
    pub(crate) fn openzt2_effect_state_float_values(
        effect: *const NativeEffect,
        index: u32,
        values: *mut f32,
        count: u32,
    );
    pub(crate) fn openzt2_effect_state_mapping(
        effect: *const NativeEffect,
        index: u32,
    ) -> *const c_char;
    pub(crate) fn openzt2_effect_sampler_state_count(
        effect: *const NativeEffect,
        index: u32,
    ) -> u32;
    pub(crate) fn openzt2_effect_sampler_state_type(
        effect: *const NativeEffect,
        index: u32,
        state: u32,
    ) -> u32;
    pub(crate) fn openzt2_effect_sampler_state_value(
        effect: *const NativeEffect,
        index: u32,
        state: u32,
    ) -> u32;
    pub(crate) fn openzt2_effect_sampler_texture_mapping(
        effect: *const NativeEffect,
        index: u32,
        state: u32,
    ) -> *const c_char;
    pub(crate) fn openzt2_effect_vertex_shader(
        effect: *const NativeEffect,
        size: *mut usize,
    ) -> *const u8;
    pub(crate) fn openzt2_effect_pixel_shader(
        effect: *const NativeEffect,
        size: *mut usize,
    ) -> *const u8;
    pub(crate) fn openzt2_effect_vertex_float(effect: *const NativeEffect) -> *const f32;
    pub(crate) fn openzt2_effect_vertex_int(effect: *const NativeEffect) -> *const i32;
    pub(crate) fn openzt2_effect_vertex_bool(effect: *const NativeEffect) -> *const u8;
    pub(crate) fn openzt2_effect_pixel_float(effect: *const NativeEffect) -> *const f32;
    pub(crate) fn openzt2_effect_pixel_int(effect: *const NativeEffect) -> *const i32;
    pub(crate) fn openzt2_effect_pixel_bool(effect: *const NativeEffect) -> *const u8;
}
