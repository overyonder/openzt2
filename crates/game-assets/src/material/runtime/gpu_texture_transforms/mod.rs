//! Texture-stage transforms that depend on the camera, the drawn mesh or the
//! environment, computed in the fixed-function shader.
//!
//! An FX state such as `TextureTransform[1] = mul(mul(vecLocalToWorld,
//! vecWorldToView), lutScaleOffset)` compiles to a small program. Its WGSL
//! translation reads renderer semantics from Bevy's view and mesh uniforms
//! and every other input from the material's transform-input buffer, which
//! owners update only when they set a value.

use std::fmt::Write;
use std::io;

use bevy::prelude::*;
use d3d9_effects::effect_evaluation::retained_transform_evaluation::{
    D3d9Preshader, D3d9PreshaderOperand, D3d9PreshaderOperandType, D3d9TransformProgram,
};

use super::gpu_semantic_shader_registers::{
    semantic_declarations, semantic_locals, semantic_value_registers,
};
use super::resolved_effect_transform_bindings::ResolvedEffectTransformSemantic as Semantic;

/// The fixed-function register whose `w` holds the mask of stages the shader computes.
pub(super) const GPU_TRANSFORM_STAGE_REGISTER: usize = 49;

/// The material binding of the transform-input buffer.
pub(super) const TRANSFORM_INPUT_BINDING: u32 = 48;

/// The fixed-function shader, with the default texture transform marked so a
/// material can replace it.
pub(super) const FIXED_FUNCTION_SOURCE: &str = include_str!("../../fixed_function.wesl");
const DEFAULT_TRANSFORM_START: &str = "// openzt2:texture-transform-begin";
const DEFAULT_TRANSFORM_END: &str = "// openzt2:texture-transform-end";

/// One texture stage whose transform the shader computes.
pub(super) enum GpuTextureTransform<'a> {
    /// A compiled FX state. `dependencies` holds each dependency's semantic,
    /// and `first_input` the stage's first register in the input buffer.
    Program {
        program: &'a D3d9TransformProgram,
        dependencies: &'a [Semantic],
        first_input: u32,
    },
    /// Model reflection: directions generated in camera space, rotated into the
    /// mesh's frame, then projected by this authored matrix.
    Reflection(Mat4),
}

/// Whether any dependency comes from the renderer rather than the owner.
pub(super) fn computed_on_gpu(dependencies: &[Semantic]) -> bool {
    dependencies
        .iter()
        .any(|semantic| semantic.is_computed_on_gpu())
}

/// The input registers a compiled state reads, filled with its defaults.
pub(super) fn default_inputs(program: &D3d9TransformProgram) -> Vec<f32> {
    match program {
        D3d9TransformProgram::Parameter { .. } => vec![0.0; 16],
        D3d9TransformProgram::Preshader(preshader) => preshader.registers.to_vec(),
    }
}

/// Where each dependency's components start and how many it has.
pub(super) fn dependency_components(
    program: &D3d9TransformProgram,
    dependency: usize,
) -> Option<(usize, usize)> {
    match program {
        D3d9TransformProgram::Parameter { .. } => (dependency == 0).then_some((0, 16)),
        D3d9TransformProgram::Preshader(preshader) => preshader
            .dependency_registers
            .get(dependency)
            .map(|(first, count)| (*first as usize * 4, *count as usize * 4)),
    }
}

/// The fixed-function shader source with these stages computed in place.
pub(super) fn fixed_function_source_with_texture_transforms(
    stages: &[(usize, GpuTextureTransform<'_>)],
) -> io::Result<String> {
    let module = texture_transform_module(stages)?;
    let start = FIXED_FUNCTION_SOURCE
        .find(DEFAULT_TRANSFORM_START)
        .ok_or_else(|| invalid("fixed-function shader has no texture transform marker"))?;
    let end = FIXED_FUNCTION_SOURCE[start..]
        .find(DEFAULT_TRANSFORM_END)
        .map(|offset| start + offset + DEFAULT_TRANSFORM_END.len())
        .ok_or_else(|| invalid("unterminated texture transform marker"))?;
    Ok(format!(
        "{}{module}{}",
        &FIXED_FUNCTION_SOURCE[..start],
        &FIXED_FUNCTION_SOURCE[end..]
    ))
}

/// `texture_transform` and the declarations and functions it calls.
fn texture_transform_module(stages: &[(usize, GpuTextureTransform<'_>)]) -> io::Result<String> {
    let mut functions = String::new();
    let mut dispatch = String::from(
        "fn texture_transform(stage: u32, instance_index: u32) -> mat4x4<f32> {\n    if ((u32(fixed.values[49].w) & (1u << stage)) == 0u) { return transforms.values[stage]; }\n    switch stage {\n",
    );
    for (stage, transform) in stages {
        let body = match transform {
            GpuTextureTransform::Program {
                program,
                dependencies,
                first_input,
            } => program_body(program, dependencies, *first_input)?,
            GpuTextureTransform::Reflection(projection) => reflection_body(*projection),
        };
        let _ = write!(
            functions,
            "\nfn openzt2_texture_transform_{stage}(instance_index: u32) -> mat4x4<f32> {{\n{}{body}}}\n",
            semantic_locals(&body),
        );
        let _ = writeln!(
            dispatch,
            "        case {stage}u: {{ return openzt2_texture_transform_{stage}(instance_index); }}"
        );
    }
    dispatch.push_str("        default: { return transforms.values[stage]; }\n    }\n}\n");
    Ok(format!(
        "{}@group({}) @binding({TRANSFORM_INPUT_BINDING}) var<storage, read> openzt2_transform_inputs: array<vec4<f32>>;\n{dispatch}{functions}",
        semantic_declarations(&functions),
        super::effect_pass_gpu_data::EFFECT_SHADER_BINDING_LAYOUT.descriptor_set,
    ))
}

fn reflection_body(projection: Mat4) -> String {
    format!(
        "    let world_from_local = get_world_from_local(instance_index);
    let columns = mat3x3<f32>(world_from_local[0].xyz, world_from_local[1].xyz, world_from_local[2].xyz);
    var rotation = mat3x3<f32>(normalize(columns[0]), normalize(columns[1]), normalize(columns[2]));
    if determinant(columns) < 0.0 {{ rotation[0] = -rotation[0]; }}
    let local_from_world = transpose(rotation);
    let model = mat4x4<f32>(vec4<f32>(local_from_world[0], 0.0), vec4<f32>(local_from_world[1], 0.0), vec4<f32>(local_from_world[2], 0.0), vec4<f32>(0.0, 0.0, 0.0, 1.0));
    let camera = mat4x4<f32>(vec4<f32>(view.world_from_view[0].xyz, 0.0), vec4<f32>(view.world_from_view[1].xyz, 0.0), vec4<f32>(view.world_from_view[2].xyz, 0.0), vec4<f32>(0.0, 0.0, 0.0, 1.0));
    return {} * model * camera;
",
        matrix_literal(projection)
    )
}

fn program_body(
    program: &D3d9TransformProgram,
    dependencies: &[Semantic],
    first_input: u32,
) -> io::Result<String> {
    let inputs = default_inputs(program);
    let mut body = format!("    var i: array<f32, {}>;\n", inputs.len().max(1));
    for register in 0..inputs.len().div_ceil(4) {
        let _ = writeln!(
            body,
            "    let input_{register} = openzt2_transform_inputs[{}];",
            first_input as usize + register
        );
        for component in 0..4.min(inputs.len() - register * 4) {
            let _ = writeln!(
                body,
                "    i[{}] = input_{register}[{component}];",
                register * 4 + component
            );
        }
    }
    for (dependency, semantic) in dependencies.iter().enumerate() {
        if !semantic.is_computed_on_gpu() {
            continue;
        }
        let Some((first, count)) = dependency_components(program, dependency) else {
            continue;
        };
        // FX matrix parameters hold a Bevy matrix's columns, as the CPU wrote them.
        for (register, value) in semantic_value_registers(*semantic, false)
            .iter()
            .enumerate()
        {
            for component in 0..4 {
                let index = register * 4 + component;
                if index < count && first + index < inputs.len() {
                    let _ = writeln!(body, "    i[{}] = ({value})[{component}];", first + index);
                }
            }
        }
    }
    body.push_str("    var o: array<f32, 16>;\n");
    match program {
        D3d9TransformProgram::Parameter {
            packed_columns,
            component_count,
        } => {
            for component in 0..*component_count {
                let source = packed_columns.map_or(component, |columns| {
                    component / columns * 4 + component % columns
                });
                let _ = writeln!(body, "    o[{component}] = i[{source}];");
            }
        }
        D3d9TransformProgram::Preshader(preshader) => {
            write_preshader(&mut body, preshader)?;
        }
    }
    body.push_str("    return mat4x4<f32>(");
    let components = (0..16)
        .map(|index| format!("o[{index}]"))
        .collect::<Vec<_>>();
    body.push_str(&components.join(", "));
    body.push_str(");\n");
    Ok(body)
}

/// Translates MojoShader's `MOJOSHADER_runPreshader` instruction by instruction.
fn write_preshader(body: &mut String, preshader: &D3d9Preshader) -> io::Result<()> {
    if preshader.temp_count > 0 {
        let _ = writeln!(body, "    var t: array<f32, {}>;", preshader.temp_count);
    }
    // MOJOSHADER_PRESHADEROP_SCALAR_OPS: the first operand of these is one value.
    const SCALAR_OPS: u32 = 25;
    for (number, instruction) in preshader.instructions.iter().enumerate() {
        let Some((destination, sources)) = instruction.operands.split_last() else {
            return Err(invalid("preshader instruction has no destination"));
        };
        let elements = instruction.element_count.min(4);
        let scalar = instruction.opcode >= SCALAR_OPS;
        let source = |operand: usize, element: u32| -> io::Result<String> {
            let first = scalar && operand == 0;
            operand_value(
                preshader,
                &sources[operand],
                if first { 0 } else { element },
            )
        };
        let operation = |element: u32| -> io::Result<String> {
            let a = || source(0, element);
            let b = || source(1, element);
            Ok(match instruction.opcode {
                1 => a()?,
                2 => format!("-{}", a()?),
                3 => format!("1.0 / {}", a()?),
                4 => format!("fract({})", a()?),
                5 => format!("exp({})", a()?),
                6 => format!("log({})", a()?),
                7 => format!("1.0 / sqrt({})", a()?),
                8 => format!("sin({})", a()?),
                9 => format!("cos({})", a()?),
                10 => format!("asin({})", a()?),
                11 => format!("acos({})", a()?),
                12 => format!("atan({})", a()?),
                13 | 25 => {
                    let (a, b) = (a()?, b()?);
                    format!("select({b}, {a}, {a} < {b})")
                }
                14 | 26 => {
                    let (a, b) = (a()?, b()?);
                    format!("select({b}, {a}, {a} > {b})")
                }
                15 | 27 => format!("select(0.0, 1.0, {} < {})", a()?, b()?),
                16 | 28 => format!("select(0.0, 1.0, {} >= {})", a()?, b()?),
                17 | 29 => format!("{} + {}", a()?, b()?),
                18 | 30 => format!("{} * {}", a()?, b()?),
                19 | 31 => format!("atan2({}, {})", a()?, b()?),
                20 | 32 => format!("{} / {}", a()?, b()?),
                21 => format!("select({}, {}, {} >= 0.0)", source(2, element)?, b()?, a()?),
                23 => (0..elements)
                    .map(|element| Ok(format!("{} * {}", source(0, element)?, source(1, element)?)))
                    .collect::<io::Result<Vec<_>>>()?
                    .join(" + "),
                opcode => {
                    return Err(invalid(&format!(
                        "preshader opcode {opcode} has no MojoShader evaluation"
                    )))
                }
            })
        };
        // Read every element before writing: the destination may be a source.
        for element in 0..elements {
            let _ = writeln!(
                body,
                "    let d{number}_{element} = {};",
                operation(element)?
            );
        }
        for element in 0..elements {
            let target = match destination.operand_type {
                D3d9PreshaderOperandType::Temp => "t",
                D3d9PreshaderOperandType::Output => "o",
                _ => return Err(invalid("preshader writes to an input or literal")),
            };
            let _ = writeln!(
                body,
                "    {target}[{}] = d{number}_{element};",
                destination.index + element
            );
        }
    }
    Ok(())
}

fn operand_value(
    preshader: &D3d9Preshader,
    operand: &D3d9PreshaderOperand,
    element: u32,
) -> io::Result<String> {
    if operand.array_register_count > 0 {
        return Err(invalid("indexed preshader inputs are not supported"));
    }
    let index = operand.index + element;
    Ok(match operand.operand_type {
        D3d9PreshaderOperandType::Input => format!("i[{index}]"),
        D3d9PreshaderOperandType::Output => format!("o[{index}]"),
        D3d9PreshaderOperandType::Temp => format!("t[{index}]"),
        D3d9PreshaderOperandType::Literal => {
            let value = preshader
                .literals
                .get(index as usize)
                .ok_or_else(|| invalid("preshader literal out of range"))?;
            float_literal(*value as f32)
        }
    })
}

fn float_literal(value: f32) -> String {
    format!("bitcast<f32>({}u)", value.to_bits())
}

fn matrix_literal(matrix: Mat4) -> String {
    let components = matrix.to_cols_array().map(float_literal).join(", ");
    format!("mat4x4<f32>({components})")
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.to_owned())
}
