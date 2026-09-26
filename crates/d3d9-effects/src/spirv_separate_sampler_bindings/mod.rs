use std::collections::{HashMap, HashSet};

use rspirv::{
    binary::Assemble,
    dr::{Instruction, Operand},
    spirv::{Decoration, Dim, ExecutionModel, Op, StorageClass},
};

use crate::{error::D3d9EffectProcessingError, shader_types::D3d9ShaderBindingLayout};

/// Adapt combined D3D samplers to the separate image/sampler slots in the
/// supplied renderer layout. rspirv owns binary decoding and encoding.
#[allow(
    clippy::too_many_lines,
    reason = "keeps the linked SPIR-V resource conversion and its ID remapping in one auditable operation"
)]
#[allow(
    clippy::redundant_pub_crate,
    reason = "this adapter must remain crate-private despite its private containing module"
)]
pub(super) fn separate_combined_shader_samplers(
    bytes: &[u8],
    binding_layout: &D3d9ShaderBindingLayout,
) -> Result<Vec<u8>, D3d9EffectProcessingError> {
    let mut module = rspirv::dr::load_bytes(bytes).map_err(|error| {
        D3d9EffectProcessingError::D3d9ShaderTranslationFailed(error.to_string())
    })?;
    let original_descriptor_sets = module.annotations.iter().filter_map(|instruction| {
        match instruction.operands.as_slice() {
            [Operand::IdRef(id), Operand::Decoration(Decoration::DescriptorSet), Operand::LiteralBit32(set)]
                if instruction.class.opcode == Op::Decorate => Some((*id, *set)),
            _ => None,
        }
    }).collect::<HashMap<_, _>>();
    for instruction in &mut module.annotations {
        if instruction.class.opcode != Op::Decorate {
            continue;
        }
        if let [Operand::IdRef(id), Operand::Decoration(decoration), Operand::LiteralBit32(value)] =
            instruction.operands.as_mut_slice()
        {
            let Some(original_set) = original_descriptor_sets.get(id) else {
                continue;
            };
            match decoration {
                Decoration::DescriptorSet => *value = binding_layout.descriptor_set,
                Decoration::Binding => {
                    // MojoShader uses sets 1 and 3 for vertex and pixel uniform banks.
                    *value = match original_set {
                        1 => binding_layout.vertex_uniform_binding,
                        3 => binding_layout.pixel_uniform_binding,
                        _ => value
                            .checked_add(binding_layout.first_texture_binding)
                            .ok_or(
                                D3d9EffectProcessingError::NativeDependencyReturnedMalformedOutput,
                            )?,
                    };
                }
                _ => {}
            }
        }
    }
    // Legacy texture instructions and declarations can both register the
    // same interface variable. SPIR-V entry point interfaces list each once.
    for entry in &mut module.entry_points {
        let mut seen = HashSet::new();
        let mut index = 0;
        entry.operands.retain(|operand| {
            index += 1;
            index <= 3
                || match operand {
                    Operand::IdRef(id) => seen.insert(*id),
                    _ => true,
                }
        });
    }
    let mut next_id = module
        .header
        .as_ref()
        .ok_or(D3d9EffectProcessingError::NativeDependencyReturnedMalformedOutput)?
        .bound;
    let mut allocate = || {
        let id = next_id;
        next_id += 1;
        id
    };
    let sampled_types = module
        .types_global_values
        .iter()
        .filter_map(|instruction| {
            if instruction.class.opcode != Op::TypeSampledImage {
                return None;
            }
            match instruction.operands.as_slice() {
                [Operand::IdRef(image)] => Some((instruction.result_id?, *image)),
                _ => None,
            }
        })
        .collect::<HashMap<_, _>>();
    let cube_image_types = module
        .types_global_values
        .iter()
        .filter(|instruction| {
            instruction.class.opcode == Op::TypeImage
                && matches!(
                    instruction.operands.get(1),
                    Some(Operand::Dim(Dim::DimCube))
                )
        })
        .filter_map(|instruction| instruction.result_id)
        .collect::<HashSet<_>>();
    let mut cube_texture_variable_bindings = Vec::new();
    let mut pointers = HashMap::new();
    for instruction in &mut module.types_global_values {
        if instruction.class.opcode != Op::TypePointer {
            continue;
        }
        if let [Operand::StorageClass(StorageClass::UniformConstant), Operand::IdRef(pointee)] =
            instruction.operands.as_mut_slice()
        {
            if let Some(image) = sampled_types.get(pointee) {
                if let Some(id) = instruction.result_id {
                    pointers.insert(id, (*pointee, *image));
                }
                *pointee = *image;
            }
        }
    }
    let mut variables = HashMap::new();
    let sampler_type = allocate();
    let sampler_pointer = allocate();
    let mut new_globals = vec![
        Instruction::new(Op::TypeSampler, None, Some(sampler_type), vec![]),
        Instruction::new(
            Op::TypePointer,
            None,
            Some(sampler_pointer),
            vec![
                Operand::StorageClass(StorageClass::UniformConstant),
                Operand::IdRef(sampler_type),
            ],
        ),
    ];
    for instruction in &module.types_global_values {
        if instruction.class.opcode != Op::Variable {
            continue;
        }
        let Some((combined_image_type, image)) = instruction
            .result_type
            .and_then(|pointer| pointers.get(&pointer))
        else {
            continue;
        };
        let Some(id) = instruction.result_id else {
            continue;
        };
        let sampler = allocate();
        let mut binding = None;
        let mut set = None;
        for annotation in &module.annotations {
            if annotation.class.opcode != Op::Decorate {
                continue;
            }
            if let [Operand::IdRef(target), Operand::Decoration(decoration), Operand::LiteralBit32(value)] =
                annotation.operands.as_slice()
            {
                if *target == id {
                    match decoration {
                        Decoration::Binding => binding = Some(*value),
                        Decoration::DescriptorSet => set = Some(*value),
                        _ => {}
                    }
                }
            }
        }
        let binding = binding
            .filter(|binding| {
                binding
                    .checked_sub(binding_layout.first_texture_binding)
                    .is_some_and(|index| index < binding_layout.texture_binding_count)
            })
            .ok_or_else(|| {
                D3d9EffectProcessingError::D3d9ShaderTranslationFailed(
                    "combined sampler is outside the material texture bindings".into(),
                )
            })?;
        let set = set.ok_or(D3d9EffectProcessingError::NativeDependencyReturnedMalformedOutput)?;
        if cube_image_types.contains(image) {
            cube_texture_variable_bindings.push((
                id,
                binding_layout
                    .first_cube_texture_binding
                    .checked_add(binding - binding_layout.first_texture_binding)
                    .ok_or(D3d9EffectProcessingError::NativeDependencyReturnedMalformedOutput)?,
            ));
        }
        for (decoration, value) in [
            (Decoration::DescriptorSet, set),
            (
                Decoration::Binding,
                binding_layout
                    .first_sampler_binding
                    .checked_add(binding - binding_layout.first_texture_binding)
                    .ok_or(D3d9EffectProcessingError::NativeDependencyReturnedMalformedOutput)?,
            ),
        ] {
            module.annotations.push(Instruction::new(
                Op::Decorate,
                None,
                None,
                vec![
                    Operand::IdRef(sampler),
                    Operand::Decoration(decoration),
                    Operand::LiteralBit32(value),
                ],
            ));
        }
        new_globals.push(Instruction::new(
            Op::Variable,
            Some(sampler_pointer),
            Some(sampler),
            vec![Operand::StorageClass(StorageClass::UniformConstant)],
        ));
        variables.insert(id, (*combined_image_type, *image, sampler));
    }
    module.types_global_values.extend(new_globals);
    for (variable, cube_binding) in cube_texture_variable_bindings {
        for annotation in &mut module.annotations {
            if annotation.class.opcode != Op::Decorate {
                continue;
            }
            if let [Operand::IdRef(target), Operand::Decoration(Decoration::Binding), Operand::LiteralBit32(value)] =
                annotation.operands.as_mut_slice()
            {
                if *target == variable {
                    *value = cube_binding;
                }
            }
        }
    }
    for function in &mut module.functions {
        for block in &mut function.blocks {
            let mut instructions = Vec::with_capacity(block.instructions.len());
            for instruction in std::mem::take(&mut block.instructions) {
                let variable = if instruction.class.opcode == Op::Load {
                    match instruction.operands.first() {
                        Some(Operand::IdRef(id)) => variables.get(id).map(|value| (*id, *value)),
                        _ => None,
                    }
                } else {
                    None
                };
                if let Some((variable, (sampled_type, image_type, sampler))) = variable {
                    let image_value = allocate();
                    let sampler_value = allocate();
                    instructions.push(Instruction::new(
                        Op::Load,
                        Some(image_type),
                        Some(image_value),
                        vec![Operand::IdRef(variable)],
                    ));
                    instructions.push(Instruction::new(
                        Op::Load,
                        Some(sampler_type),
                        Some(sampler_value),
                        vec![Operand::IdRef(sampler)],
                    ));
                    instructions.push(Instruction::new(
                        Op::SampledImage,
                        Some(sampled_type),
                        instruction.result_id,
                        vec![Operand::IdRef(image_value), Operand::IdRef(sampler_value)],
                    ));
                } else {
                    instructions.push(instruction);
                }
            }
            block.instructions = instructions;
        }
    }
    // Shader model 1.x writes its sole pixel result through r0 without an
    // explicit output declaration. Attach the D3D colour-target-zero binding.
    if module.entry_points.iter().any(|entry| {
        matches!(
            entry.operands.first(),
            Some(Operand::ExecutionModel(ExecutionModel::Fragment))
        )
    }) {
        for instruction in &module.types_global_values {
            if instruction.class.opcode != Op::Variable
                || !matches!(
                    instruction.operands.first(),
                    Some(Operand::StorageClass(StorageClass::Output))
                )
            {
                continue;
            }
            let Some(id) = instruction.result_id else {
                continue;
            };
            let bound = module.annotations.iter().any(|annotation| matches!(annotation.operands.as_slice(), [Operand::IdRef(target), Operand::Decoration(Decoration::Location | Decoration::BuiltIn), ..] if *target == id));
            if !bound {
                module.annotations.push(Instruction::new(
                    Op::Decorate,
                    None,
                    None,
                    vec![
                        Operand::IdRef(id),
                        Operand::Decoration(Decoration::Location),
                        Operand::LiteralBit32(0),
                    ],
                ));
            }
        }
    }
    if let Some(header) = &mut module.header {
        header.bound = next_id;
    }
    Ok(module
        .assemble()
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect())
}
