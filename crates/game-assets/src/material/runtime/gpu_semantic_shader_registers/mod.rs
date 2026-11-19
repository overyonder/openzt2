//! Computes camera- and model-dependent effect registers in the shader itself.
//!
//! A translated D3D9 shader reads its constants from one packed register array.
//! The rewritten shader copies that array into a private one on entry and then
//! overwrites the registers bound to renderer semantics: transforms from Bevy's
//! view and mesh uniforms, and wind, lights and fog distances from the shared
//! environment buffer. One material then serves every draw and every camera.
//!
//! Effects see the original game's z-up world: Bevy's y-up world maps to it
//! through `openzt2_source_from_world`, so wind, lights and authored source
//! geometry such as water keep their original meaning.

use std::fmt::Write;
use std::io;

use super::resolved_effect_transform_bindings::ResolvedEffectTransformSemantic as Semantic;

/// One semantic register range that the shader computes for itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct GpuSemanticRegisters {
    pub semantic: Semantic,
    pub first_register: u32,
    pub register_count: u32,
    pub is_row_major_matrix: bool,
}

/// Where the shader finds the draw's mesh instance index.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum InstanceIndexInput {
    /// The vertex stage's `instance_index` builtin.
    VertexBuiltin,
    /// A flat value the vertex stage forwards at this location.
    FlatLocation(u32),
}

/// The material binding of the shared environment registers.
pub(super) const ENVIRONMENT_REGISTER_BINDING: u32 = 47;
/// Wind's seven registers, then the light rig's seven, then fog distances.
pub(super) const ENVIRONMENT_REGISTER_COUNT: usize = 15;
const LIGHT_RIG_OFFSET: u32 = 7;
const FOG_OFFSET: u32 = 14;

/// The inter-stage location that carries the mesh instance index to pixel
/// shaders whose registers depend on the model.
pub(super) const FORWARDED_INSTANCE_INDEX_LOCATION: u32 = 15;

impl Semantic {
    /// Semantics the renderer supplies, rather than the material's owner.
    pub(super) const fn is_computed_on_gpu(self) -> bool {
        !matches!(self, Self::Other)
    }

    const fn reads_model(self) -> bool {
        matches!(
            self,
            Self::ModelToWorld
                | Self::WorldToModel
                | Self::VectorToWorld
                | Self::ModelToView
                | Self::ModelToProjection
        )
    }
}

pub(super) fn reads_model(registers: &[GpuSemanticRegisters]) -> bool {
    registers
        .iter()
        .any(|register| register.semantic.reads_model())
}

struct TranslatedShaderText {
    text: String,
}

/// Rewrites a translated shader. `float_register_order` lists the D3D float
/// register held at each packed array index. `forward_instance_index` makes a
/// vertex shader also output its instance index for the pixel stage.
pub(super) fn rewrite_translated_shader_with_gpu_semantic_registers(
    spirv: &[u8],
    uniform_binding: u32,
    float_register_order: &[u32],
    registers: &[GpuSemanticRegisters],
    instance_index: Option<InstanceIndexInput>,
    forward_instance_index: bool,
) -> io::Result<String> {
    let mut shader = TranslatedShaderText::from_spirv(spirv)?;
    let model = reads_model(registers);
    let instance_index = match (model || forward_instance_index, instance_index) {
        (false, _) => None,
        (true, Some(input)) => Some(input),
        (true, None) => {
            // A model-dependent pixel shader after a fixed-function vertex stage.
            return Err(invalid("model-dependent registers have no instance index"));
        }
    };
    let writes = register_writes(float_register_order, registers);
    if !writes.is_empty() {
        let (array, field) = shader.privatise_uniform_registers(uniform_binding)?;
        shader
            .text
            .push_str(&semantic_register_function(&array, &field, &writes, model));
        let call = if model {
            format!("    {array} = openzt2_register_source;\n    openzt2_bind_semantic_registers(openzt2_instance_index);\n")
        } else {
            format!(
                "    {array} = openzt2_register_source;\n    openzt2_bind_semantic_registers();\n"
            )
        };
        shader.prepend_to_entry_point(&call)?;
    }
    if let Some(input) = instance_index {
        let parameter = match input {
            InstanceIndexInput::VertexBuiltin => {
                "@builtin(instance_index) openzt2_instance_index: u32".to_owned()
            }
            InstanceIndexInput::FlatLocation(location) => {
                format!("@location({location}) @interpolate(flat) openzt2_instance_index: u32")
            }
        };
        shader.add_entry_point_parameter(&parameter)?;
    }
    if forward_instance_index {
        shader.forward_vertex_instance_index()?;
    }
    shader.text.insert_str(
        0,
        "import bevy_pbr::render::{\n    mesh_functions::{get_world_from_local, get_local_from_world},\n    mesh_view_bindings::view,\n};\n\n",
    );
    Ok(shader.text)
}

/// Apply the D3D9 sampler bias to Naga's translated texture instructions.
pub(super) fn apply_translated_sampler_biases(
    shader: &mut String,
    binding_layout: d3d9_effects::shader_types::D3d9ShaderBindingLayout,
    biases: &[f32; 8],
) -> io::Result<()> {
    let mut names = Vec::new();
    let group = format!("@group({})", binding_layout.descriptor_set);
    for (stage, &bias) in (0_u32..).zip(biases) {
        if bias == 0.0 {
            continue;
        }
        let binding = format!("@binding({})", binding_layout.first_sampler_binding + stage);
        let Some(attributes) = shader.find(&format!("{group} {binding}")) else {
            continue;
        };
        let declaration = &shader[attributes + group.len() + binding.len() + 1..];
        let name = declaration
            .trim_start()
            .strip_prefix("var ")
            .and_then(|variable| variable.split_once(':'))
            .map(|(name, _)| name.trim())
            .ok_or_else(|| invalid("translated sampler declaration has no name"))?;
        names.push((
            name.to_owned(),
            format!("bitcast<f32>(0x{:08x}u)", bias.to_bits()),
        ));
    }
    let mut offset = 0;
    while let Some(index) = shader[offset..].find("textureSample") {
        let start = offset + index;
        let tail = &shader[start..];
        let end_name = tail
            .find(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
            .unwrap_or(tail.len());
        let name = &tail[..end_name];
        let Some(arguments_text) = tail[end_name..].strip_prefix('(') else {
            offset = start + end_name;
            continue;
        };
        offset = start + end_name + 1;
        if !matches!(
            name,
            "textureSample" | "textureSampleBias" | "textureSampleLevel" | "textureSampleGrad"
        ) {
            continue;
        }
        let (argument_end, arguments) = translated_texture_sample_arguments(arguments_text)
            .ok_or_else(|| invalid("translated texture sample has unbalanced arguments"))?;
        let Some((_, bias)) = arguments
            .get(1)
            .and_then(|sampler| names.iter().find(|(name, _)| name == sampler))
        else {
            continue;
        };
        let mut arguments = arguments
            .iter()
            .map(|value| (*value).to_owned())
            .collect::<Vec<_>>();
        let replacement = match name {
            "textureSample" if arguments.len() >= 3 => {
                arguments.insert(3, bias.clone());
                "textureSampleBias"
            }
            "textureSampleBias" | "textureSampleLevel" if arguments.len() >= 4 => {
                arguments[3] = format!("({}) + {bias}", arguments[3]);
                name
            }
            "textureSampleGrad" if arguments.len() >= 5 => {
                for gradient in &mut arguments[3..5] {
                    *gradient = format!("({gradient}) * exp2({bias})");
                }
                name
            }
            _ => {
                return Err(invalid(
                    "translated texture sample has an unsupported argument layout",
                ))
            }
        };
        let replacement_call = format!("{replacement}({})", arguments.join(", "));
        let end = offset + argument_end + 1;
        offset = start + replacement.len() + 1;
        shader.replace_range(start..end, &replacement_call);
    }
    Ok(())
}

fn translated_texture_sample_arguments(text: &str) -> Option<(usize, Vec<&str>)> {
    let mut depth = 0;
    let mut start = 0;
    let mut arguments = Vec::new();
    for (index, character) in text.char_indices() {
        match character {
            '(' | '[' => depth += 1,
            ')' if depth == 0 => {
                arguments.push(text[start..index].trim());
                return Some((index, arguments));
            }
            ')' | ']' => depth -= 1,
            ',' if depth == 0 => {
                arguments.push(text[start..index].trim());
                start = index + 1;
            }
            _ => {}
        }
    }
    None
}

impl TranslatedShaderText {
    fn from_spirv(spirv: &[u8]) -> io::Result<Self> {
        // Read the SPIR-V as wgpu does. The translated shader already writes
        // wgpu's y-up clip space, so naga must not flip it again.
        let module = naga::front::spv::parse_u8_slice(
            spirv,
            &naga::front::spv::Options {
                adjust_coordinate_space: false,
                ..naga::front::spv::Options::default()
            },
        )
        .map_err(io::Error::other)?;
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .map_err(|error| invalid(&error.emit_to_string("")))?;
        let text =
            naga::back::wgsl::write_string(&module, &info, naga::back::wgsl::WriterFlags::empty())
                .map_err(io::Error::other)?;
        Ok(Self { text })
    }

    /// Replaces the packed register uniform with a private copy of the same
    /// type, returning the copy's name and its array field.
    fn privatise_uniform_registers(&mut self, binding: u32) -> io::Result<(String, String)> {
        let attribute = format!("@binding({binding})");
        let lines = self.text.lines().collect::<Vec<_>>();
        let index = lines
            .windows(2)
            .position(|pair| {
                pair[0].trim_start().starts_with("@group(")
                    && pair[0].contains(&attribute)
                    && pair[1].trim_start().starts_with("var<uniform> ")
            })
            .map(|index| index + 1)
            .ok_or_else(|| invalid("translated shader has no register uniform"))?;
        let declaration = lines[index].trim();
        let (name, ty) = declaration
            .trim_start_matches("var<uniform> ")
            .trim_end_matches(';')
            .split_once(':')
            .map(|(name, ty)| (name.trim().to_owned(), ty.trim().to_owned()))
            .ok_or_else(|| invalid("unexpected register uniform declaration"))?;
        let header = format!("struct {ty} {{");
        let field = lines
            .iter()
            .position(|line| line.trim() == header)
            .and_then(|start| lines.get(start + 1))
            .and_then(|line| line.trim().split_once(':'))
            .map(|(field, _)| field.trim().to_owned())
            .ok_or_else(|| invalid("register uniform has no array field"))?;
        let replacement =
            format!("var<uniform> openzt2_register_source: {ty};\nvar<private> {name}: {ty};");
        self.text = lines
            .iter()
            .enumerate()
            .map(|(line, text)| {
                if line == index {
                    replacement.as_str()
                } else {
                    text
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        self.text.push('\n');
        Ok((name, field))
    }

    fn entry_point_signature(&self) -> io::Result<(usize, usize)> {
        let start = ["@vertex", "@fragment"]
            .iter()
            .filter_map(|stage| self.text.find(stage))
            .min()
            .ok_or_else(|| invalid("translated shader has no entry point"))?;
        let open = self.text[start..]
            .find("fn main(")
            .map(|offset| start + offset + "fn main(".len())
            .ok_or_else(|| invalid("translated entry point is not main"))?;
        let body = self.text[open..]
            .find('{')
            .map(|offset| open + offset + 1)
            .ok_or_else(|| invalid("translated entry point has no body"))?;
        Ok((open, body))
    }

    fn add_entry_point_parameter(&mut self, parameter: &str) -> io::Result<()> {
        let (open, _) = self.entry_point_signature()?;
        let separator = if self.text[open..].starts_with(')') {
            ""
        } else {
            ", "
        };
        self.text
            .insert_str(open, &format!("{parameter}{separator}"));
        Ok(())
    }

    fn prepend_to_entry_point(&mut self, statements: &str) -> io::Result<()> {
        let (_, body) = self.entry_point_signature()?;
        self.text.insert_str(body, &format!("\n{statements}"));
        Ok(())
    }

    fn forward_vertex_instance_index(&mut self) -> io::Result<()> {
        let location = FORWARDED_INSTANCE_INDEX_LOCATION;
        let header = "struct VertexOutput {";
        let start = self
            .text
            .find(header)
            .ok_or_else(|| invalid("vertex shader has no output structure"))?;
        let close = self.text[start..]
            .find("\n}")
            .map(|offset| start + offset)
            .ok_or_else(|| invalid("unterminated vertex output structure"))?;
        self.text.insert_str(
            close,
            &format!("\n    @location({location}) @interpolate(flat) openzt2_instance_index: u32,"),
        );
        let (_, body) = self.entry_point_signature()?;
        let construction = self.text[body..]
            .find("return VertexOutput(")
            .map(|offset| body + offset)
            .ok_or_else(|| invalid("vertex shader does not return its output structure"))?;
        let end = self.text[construction..]
            .find(");")
            .map(|offset| construction + offset)
            .ok_or_else(|| invalid("unterminated vertex output construction"))?;
        self.text.insert_str(end, ", openzt2_instance_index");
        Ok(())
    }
}

/// One packed register assignment, as a WGSL expression of the semantic value.
struct RegisterWrite {
    packed_index: usize,
    value: String,
}

fn register_writes(
    float_register_order: &[u32],
    registers: &[GpuSemanticRegisters],
) -> Vec<RegisterWrite> {
    let mut writes = Vec::new();
    for register in registers {
        let values = semantic_value_registers(register.semantic, !register.is_row_major_matrix);
        for (offset, value) in (0..register.register_count).zip(values) {
            if let Some(packed_index) = float_register_order
                .iter()
                .position(|candidate| *candidate == register.first_register + offset)
            {
                writes.push(RegisterWrite {
                    packed_index,
                    value,
                });
            }
        }
    }
    writes
}

/// A semantic's value as consecutive four-component registers: a matrix's
/// columns (or rows, transposed), one vector, or environment registers.
pub(super) fn semantic_value_registers(semantic: Semantic, transpose_matrix: bool) -> Vec<String> {
    if let Some(vector) = semantic_vector(semantic) {
        return vec![vector.to_owned()];
    }
    let environment = |first: u32, count: u32| {
        (first..first + count)
            .map(|index| format!("openzt2_environment[{index}]"))
            .collect()
    };
    match semantic {
        Semantic::Wind => environment(0, LIGHT_RIG_OFFSET),
        Semantic::Light => environment(LIGHT_RIG_OFFSET, LIGHT_RIG_OFFSET),
        // (start, 1 / (end - start), end, far), then the view-direction plane.
        Semantic::Fog => vec![
            format!("vec4<f32>(openzt2_environment[{FOG_OFFSET}].xyz, dot(view.frustum[5].xyz, view.world_position) + view.frustum[5].w)"),
            "vec4<f32>(openzt2_view_forward, dot((openzt2_source_from_world * vec4<f32>(view.world_position, 1.0)).xyz, openzt2_view_forward))".to_owned(),
        ],
        _ => semantic_matrix(semantic).map_or_else(Vec::new, |matrix| {
            let matrix = if transpose_matrix {
                format!("transpose({matrix})")
            } else {
                matrix.to_owned()
            };
            (0..4).map(|column| format!("({matrix})[{column}]")).collect()
        }),
    }
}

/// The local values the semantic expressions in `body` refer to.
pub(super) fn semantic_locals(body: &str) -> String {
    let mut locals = String::new();
    if body.contains("openzt2_world_from_local") {
        locals
            .push_str("    let openzt2_world_from_local = get_world_from_local(instance_index);\n");
    }
    if body.contains("openzt2_local_from_world") {
        locals
            .push_str("    let openzt2_local_from_world = get_local_from_world(instance_index);\n");
    }
    if body.contains("openzt2_view_forward") {
        locals.push_str("    let openzt2_view_forward = (openzt2_source_from_world * vec4<f32>(-normalize(view.world_from_view[2].xyz), 0.0)).xyz;\n");
    }
    locals
}

/// Module-scope declarations the semantic expressions need.
pub(super) fn semantic_declarations(body: &str) -> String {
    let mut declarations = String::from(SOURCE_WORLD_FRAME);
    if body.contains("openzt2_environment") {
        declarations.push_str(&format!(
            "@group({}) @binding({ENVIRONMENT_REGISTER_BINDING}) var<storage, read> openzt2_environment: array<vec4<f32>, {ENVIRONMENT_REGISTER_COUNT}>;\n",
            super::effect_pass_gpu_data::EFFECT_SHADER_BINDING_LAYOUT.descriptor_set,
        ));
    }
    declarations
}

const fn semantic_vector(semantic: Semantic) -> Option<&'static str> {
    Some(match semantic {
        Semantic::CameraPosition => {
            "openzt2_source_from_world * vec4<f32>(view.world_position, 1.0)"
        }
        Semantic::CameraAcross => {
            "openzt2_source_from_world * vec4<f32>(normalize(view.world_from_view[0].xyz), 0.0)"
        }
        Semantic::CameraUp => {
            "openzt2_source_from_world * vec4<f32>(normalize(view.world_from_view[1].xyz), 0.0)"
        }
        _ => return None,
    })
}

/// Column-vector matrices between the model, the original's world, the view
/// and clip space.
const fn semantic_matrix(semantic: Semantic) -> Option<&'static str> {
    Some(match semantic {
        Semantic::Identity => "mat4x4<f32>(vec4<f32>(1.0, 0.0, 0.0, 0.0), vec4<f32>(0.0, 1.0, 0.0, 0.0), vec4<f32>(0.0, 0.0, 1.0, 0.0), vec4<f32>(0.0, 0.0, 0.0, 1.0))",
        Semantic::ModelToWorld => "openzt2_source_from_world * openzt2_world_from_local",
        Semantic::WorldToModel => "openzt2_local_from_world * openzt2_world_from_source",
        Semantic::VectorToWorld => {
            "transpose(openzt2_local_from_world * openzt2_world_from_source)"
        }
        Semantic::VectorWorldToView => "transpose(openzt2_source_from_world * view.world_from_view)",
        Semantic::ModelToView => "view.view_from_world * openzt2_world_from_local",
        Semantic::ModelToProjection => {
            "view.clip_from_view * view.view_from_world * openzt2_world_from_local"
        }
        Semantic::WorldToView => "view.view_from_world * openzt2_world_from_source",
        Semantic::ViewToWorld => "openzt2_source_from_world * view.world_from_view",
        Semantic::ViewToProjection => "view.clip_from_view",
        Semantic::WorldToProjection => {
            "view.clip_from_view * view.view_from_world * openzt2_world_from_source"
        }
        _ => return None,
    })
}

/// Bevy's y-up world from the original's z-up world, and its inverse.
const SOURCE_WORLD_FRAME: &str = "const openzt2_world_from_source = mat4x4<f32>(vec4<f32>(1.0, 0.0, 0.0, 0.0), vec4<f32>(0.0, 0.0, -1.0, 0.0), vec4<f32>(0.0, 1.0, 0.0, 0.0), vec4<f32>(0.0, 0.0, 0.0, 1.0));
const openzt2_source_from_world = mat4x4<f32>(vec4<f32>(1.0, 0.0, 0.0, 0.0), vec4<f32>(0.0, 0.0, 1.0, 0.0), vec4<f32>(0.0, -1.0, 0.0, 0.0), vec4<f32>(0.0, 0.0, 0.0, 1.0));
";

fn semantic_register_function(
    array: &str,
    field: &str,
    writes: &[RegisterWrite],
    model: bool,
) -> String {
    let mut body = String::new();
    for write in writes {
        let _ = writeln!(
            body,
            "    {array}.{field}[{}] = {};",
            write.packed_index, write.value
        );
    }
    let parameter = if model { "instance_index: u32" } else { "" };
    format!(
        "\n{}\nfn openzt2_bind_semantic_registers({parameter}) {{\n{}{body}}}\n",
        semantic_declarations(&body),
        semantic_locals(&body),
    )
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.to_owned())
}

#[cfg(test)]
mod tests;
