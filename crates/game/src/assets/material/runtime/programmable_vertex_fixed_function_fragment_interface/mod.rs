//! Connect dependency-linked vertex varyings to the existing texture-stage shader.

use std::{fmt::Write, io};

use bevy::shader::Shader;
use d3d9_effects::shader_types::SpirvShaderTranslation;

pub(super) fn create_fixed_function_fragment_shader_for_programmable_vertex_outputs(
    vertex: &SpirvShaderTranslation,
    source_path: String,
) -> io::Result<Shader> {
    let mut source = String::from(
        "#import openzt2::fixed_function::{FixedFunctionFragmentInputs, evaluate_fixed_function_texture_stages}\n\
         struct ProgrammableVertexOutputs {\n",
    );
    for output in &vertex.vertex_output_signature_elements {
        let value_type = match output.component_count {
            1 => "f32",
            4 => "vec4<f32>",
            count => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("unsupported linked vertex output component count {count}"),
                ));
            }
        };
        writeln!(
            source,
            "    @location({0}) varying_{0}: {value_type},",
            output.location,
        )
        .map_err(io::Error::other)?;
    }
    // Fragment position also keeps the input structure valid for a shader
    // that only emits the mandatory vertex position builtin.
    source.push_str(
        "    @builtin(position) position: vec4<f32>,\n\
         };\n\
         @fragment fn fragment(input: ProgrammableVertexOutputs) -> @location(0) vec4<f32> {\n\
             var fixed: FixedFunctionFragmentInputs;\n\
             fixed.diffuse = vec4(1.0);\n",
    );
    for output in &vertex.vertex_output_signature_elements {
        match (output.semantic_name.as_str(), output.semantic_index) {
            ("COLOR", 0) => writeln!(
                source,
                "    fixed.diffuse = input.varying_{};",
                output.location,
            ),
            ("TEXCOORD", index @ 0..=7) => writeln!(
                source,
                "    fixed.texture_coordinates[{index}] = input.varying_{};",
                output.location,
            ),
            // The existing fixed-function fragment implementation does not
            // apply secondary specular colour or vertex fog yet. Declare
            // these outputs with their real types without repurposing them.
            ("COLOR", 1) | ("FOG", 0 | 1) => Ok(()),
            (semantic, index) => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("unsupported fixed-function vertex output {semantic}{index}"),
                ));
            }
        }
        .map_err(io::Error::other)?;
    }
    source.push_str("    return evaluate_fixed_function_texture_stages(fixed, true);\n}\n");
    Ok(Shader::from_wgsl(source, source_path))
}
