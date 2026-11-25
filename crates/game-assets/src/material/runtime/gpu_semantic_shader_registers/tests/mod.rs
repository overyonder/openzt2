use super::*;

const BINDING_LAYOUT: d3d9_effects::shader_types::D3d9ShaderBindingLayout =
    super::super::effect_pass_gpu_data::EFFECT_SHADER_BINDING_LAYOUT;

/// Stand-ins for Bevy's imports, so the rewritten text validates on its own.
const BEVY_STUBS: &str = "struct View { clip_from_view: mat4x4<f32>, view_from_world: mat4x4<f32>, world_from_view: mat4x4<f32>, world_position: vec3<f32>, frustum: array<vec4<f32>, 6> }
@group(0) @binding(0) var<uniform> view: View;
fn get_world_from_local(instance_index: u32) -> mat4x4<f32> { return mat4x4<f32>(); }
fn get_local_from_world(instance_index: u32) -> mat4x4<f32> { return mat4x4<f32>(); }
";

fn rewrite(
    source: &[u8],
    registers: &[GpuSemanticRegisters],
    instance_index: Option<InstanceIndexInput>,
    forward: bool,
) -> String {
    let bytecode =
        d3d9_effects::shader_translation::assemble_d3d9_shader_assembly_source_to_bytecode(source)
            .unwrap();
    let shader = d3d9_effects::shader_translation::translate_d3d9_shader_bytecode_to_spirv(
        &bytecode,
        &BINDING_LAYOUT,
    )
    .unwrap();
    let text = rewrite_translated_shader_with_gpu_semantic_registers(
        &shader.spirv_bytecode,
        BINDING_LAYOUT.vertex_uniform_binding,
        &shader.uniform_register_upload_order[0],
        registers,
        instance_index,
        forward,
    )
    .unwrap();
    let body = text.split_once("};\n\n").unwrap().1;
    let module = naga::front::wgsl::parse_str(&format!("{BEVY_STUBS}{body}"))
        .unwrap_or_else(|error| panic!("{}\n{body}", error.emit_to_string(body)));
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap();
    text
}

#[test]
fn computes_model_and_view_registers_in_the_shader() {
    let text = rewrite(
        b"vs_2_0\ndcl_position v0\ndcl_texcoord v1\nm4x4 oPos, v0, c4\nadd oT0, v1, c9\nadd oT1, c10, c11\nmov oD0, c0\n",
        &[
            GpuSemanticRegisters {
                semantic: Semantic::ModelToProjection,
                first_register: 4,
                register_count: 4,
                is_row_major_matrix: false,
            },
            GpuSemanticRegisters {
                semantic: Semantic::CameraPosition,
                first_register: 9,
                register_count: 1,
                is_row_major_matrix: false,
            },
            GpuSemanticRegisters {
                semantic: Semantic::Fog,
                first_register: 10,
                register_count: 2,
                is_row_major_matrix: false,
            },
        ],
        Some(InstanceIndexInput::VertexBuiltin),
        true,
    );
    // c0 stays authored; the others come from the view, mesh and environment.
    assert_eq!(text.matches("] = (transpose(").count(), 4);
    assert!(text.contains("vec4<f32>(view.world_position, 1.0)"));
    assert!(text.contains("openzt2_environment[14].xyz"));
    assert!(text.contains("@builtin(instance_index) openzt2_instance_index: u32"));
    assert!(text.contains("@location(15) @interpolate(flat) openzt2_instance_index: u32,"));
}
