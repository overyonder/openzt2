/// Descriptor bindings supplied by the renderer that consumes the translated shader.
#[derive(Clone, Copy, Debug)]
pub struct D3d9ShaderBindingLayout {
    pub descriptor_set: u32,
    pub vertex_uniform_binding: u32,
    pub pixel_uniform_binding: u32,
    pub first_texture_binding: u32,
    pub texture_binding_count: u32,
    pub first_sampler_binding: u32,
    /// Cube textures use a parallel range: stage N binds at this value plus N.
    pub first_cube_texture_binding: u32,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct D3d9ShaderInputSignatureElement {
    pub semantic_name: String,
    pub semantic_index: u32,
    pub input_register: u32,
}

#[derive(Clone, Debug)]
pub struct SpirvShaderTranslation {
    /// Source D3D register indices in each SPIR-V packed bank (float, int, bool).
    pub uniform_register_upload_order: [Vec<u32>; 3],
    pub spirv_bytecode: Vec<u8>,
    pub input_signature_elements: Vec<D3d9ShaderInputSignatureElement>,
    pub vertex_output_signature_elements: Vec<D3d9ShaderVertexOutputSignatureElement>,
    pub uniform_register_bindings: Vec<D3d9ShaderUniformRegisterBinding>,
}

/// Dependency-reflected vertex varyings after SPIR-V interface linking.
/// Position and point size are builtins and are not included in this list.
#[derive(Clone, Debug)]
pub struct D3d9ShaderVertexOutputSignatureElement {
    pub semantic_name: String,
    pub semantic_index: u32,
    pub location: u32,
    pub component_count: u8,
}

#[derive(Clone, Debug)]
pub struct D3d9ShaderUniformRegisterBinding {
    pub parameter_name: String,
    pub register_set: u32,
    pub first_register: u32,
    pub register_count: u32,
    /// Matrix register packing reported by the compiled shader constant table.
    pub is_row_major_matrix: bool,
}
