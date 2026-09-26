//! Lowers repaired PSYS authoring documents to particle-effect data.

use std::collections::HashMap;

use super::particle_system_emitter_graph_validation::validate_particle_system_collision_spawn_emitter_graph;
use super::particle_system_emitter_lowering::{
    calculate_particle_system_emitter_capacity, create_stable_particle_system_emitter_id,
    lower_particle_system_emitter_intrinsic_modifiers,
    lower_particle_system_emitter_shape_and_spawn_policy,
};
use super::particle_system_modifier_lowering::lower_particle_system_modifier_node;
use super::particle_system_renderer_lowering::lower_particle_system_renderer_node;
use super::particle_system_source_conversion_error::{
    particle_system_source_conversion_failure, ParticleSystemSourceConversionError,
};
use super::particle_system_source_node_traversal::{
    describe_particle_system_source_node, particle_system_source_children_named,
    particle_system_source_local_name,
};
use super::particle_system_source_scalar_reading::{
    read_particle_system_attribute_bool_or_default, read_required_particle_system_attribute_u32,
};

use crate::assets::source_document::{
    blue_fang_source_document_format::BlueFangSourceDocumentFormat,
    blue_fang_source_document_parsing::parse_blue_fang_source_document,
    ordered_source_document_types::OrderedSourceDocument, path::AssetPath,
};
use openzt2_game_data::{
    particle::authored_particle_system::{
        AuthoredParticleOverflowPolicy as EffectOverflowPolicy,
        AuthoredParticleSystemDocument as EffectDocument,
        AuthoredParticleSystemEmitter as EffectEmitter,
        AuthoredParticleSystemModifier as EffectModifier,
        ParticleEffectSimulationClock as EffectClock,
    },
    AssetId,
};

pub(super) fn lower_authored_particle_system_source_to_effect_document(
    particle_system_asset_path: &str,
    particle_system_source_bytes: &[u8],
) -> Result<EffectDocument, ParticleSystemSourceConversionError> {
    let particle_system_document = parse_blue_fang_source_document(
        AssetPath::new(particle_system_asset_path),
        particle_system_source_bytes,
    )
    .map_err(|source_document_error| {
        particle_system_source_conversion_failure(
            particle_system_asset_path,
            "document",
            source_document_error.to_string(),
        )
    })?;
    if particle_system_document.format != BlueFangSourceDocumentFormat::ParticleSystem {
        return Err(particle_system_source_conversion_failure(
            particle_system_asset_path,
            "document",
            "source is not a particle system",
        ));
    }
    lower_parsed_particle_system_document_to_effect_document(&particle_system_document)
}

fn lower_parsed_particle_system_document_to_effect_document(
    particle_system_document: &OrderedSourceDocument,
) -> Result<EffectDocument, ParticleSystemSourceConversionError> {
    let particle_system_asset_path = particle_system_document.path.key();
    let simulator_nodes = particle_system_document
        .root
        .element_children()
        .filter(|source_node| particle_system_source_local_name(&source_node.name) == "simulator")
        .collect::<Vec<_>>();
    if simulator_nodes.is_empty() {
        return Err(particle_system_source_conversion_failure(
            &particle_system_asset_path,
            "document",
            "PSYS contains no simulators",
        ));
    }
    let mut emitter_id_by_source_handle = HashMap::<u32, Option<AssetId>>::new();
    for simulator_node in &simulator_nodes {
        let simulator_handle =
            read_required_particle_system_attribute_u32(simulator_node, "handle")?;
        for (emitter_ordinal, emitter_node) in
            particle_system_source_children_named(simulator_node, "emitter").enumerate()
        {
            let emitter_handle =
                read_required_particle_system_attribute_u32(emitter_node, "handle")?;
            let emitter_id = create_stable_particle_system_emitter_id(
                &particle_system_asset_path,
                simulator_handle,
                emitter_handle,
                emitter_ordinal,
            );
            emitter_id_by_source_handle
                .entry(emitter_handle)
                .and_modify(|existing| *existing = None)
                .or_insert(Some(emitter_id));
        }
    }
    let resolve_emitter_handle = |emitter_handle| {
        emitter_id_by_source_handle
            .get(&emitter_handle)
            .copied()
            .flatten()
    };
    let mut effect_emitters = Vec::new();
    for simulator_node in simulator_nodes {
        let simulator_handle =
            read_required_particle_system_attribute_u32(simulator_node, "handle")?;
        let emitter_nodes =
            particle_system_source_children_named(simulator_node, "emitter").collect::<Vec<_>>();
        if emitter_nodes.is_empty() {
            continue;
        }
        let renderer_nodes =
            particle_system_source_children_named(simulator_node, "renderer").collect::<Vec<_>>();
        let primary_renderer_node = renderer_nodes.first().copied().ok_or_else(|| {
            particle_system_source_conversion_failure(
                &particle_system_asset_path,
                describe_particle_system_source_node(simulator_node),
                "simulator has no renderer",
            )
        })?;
        let (material_asset_path, renderer_modifier) = lower_particle_system_renderer_node(
            &particle_system_asset_path,
            primary_renderer_node,
        )?;
        for additional_renderer_node in renderer_nodes.iter().skip(1) {
            let additional_renderer = lower_particle_system_renderer_node(
                &particle_system_asset_path,
                additional_renderer_node,
            )?;
            if additional_renderer != (material_asset_path.clone(), renderer_modifier.clone()) {
                return Err(particle_system_source_conversion_failure(
                    &particle_system_asset_path,
                    describe_particle_system_source_node(simulator_node),
                    "simulator has heterogeneous renderers",
                ));
            }
        }
        let shared_effect_modifiers =
            particle_system_source_children_named(simulator_node, "modifier")
                .map(|modifier_node| {
                    lower_particle_system_modifier_node(
                        &particle_system_asset_path,
                        modifier_node,
                        &resolve_emitter_handle,
                    )
                })
                .collect::<Result<Vec<_>, _>>()?;
        let die_when_empty_is_enabled =
            read_particle_system_attribute_bool_or_default(simulator_node, "diewhenempty", false)?;
        for (emitter_ordinal, emitter_node) in emitter_nodes.into_iter().enumerate() {
            let emitter_handle =
                read_required_particle_system_attribute_u32(emitter_node, "handle")?;
            let mut effect_modifiers =
                lower_particle_system_emitter_intrinsic_modifiers(emitter_node)?;
            effect_modifiers.extend(shared_effect_modifiers.iter().cloned());
            effect_modifiers.push(renderer_modifier.clone());
            effect_modifiers.push(EffectModifier::DieWhenEmpty {
                enabled: die_when_empty_is_enabled,
            });
            effect_modifiers.push(EffectModifier::Overflow {
                policy: EffectOverflowPolicy::DropNewest,
            });
            let (emitter_shape, spawn_policy) =
                lower_particle_system_emitter_shape_and_spawn_policy(
                    &particle_system_asset_path,
                    emitter_node,
                    &mut effect_modifiers,
                )?;
            let emitter_capacity = calculate_particle_system_emitter_capacity(
                &particle_system_asset_path,
                simulator_node,
                &effect_modifiers,
                spawn_policy,
            )?;
            effect_emitters.push(EffectEmitter {
                emitter_id: create_stable_particle_system_emitter_id(
                    &particle_system_asset_path,
                    simulator_handle,
                    emitter_handle,
                    emitter_ordinal,
                ),
                capacity: emitter_capacity,
                spawn: spawn_policy,
                clock: EffectClock::Presentation,
                shape: emitter_shape,
                material: material_asset_path.clone(),
                modifiers: effect_modifiers,
            });
        }
    }
    validate_particle_system_collision_spawn_emitter_graph(
        &particle_system_asset_path,
        &effect_emitters,
    )?;
    Ok(EffectDocument {
        effect_id: AssetId::from_virtual_path(&particle_system_asset_path),
        emitters: effect_emitters,
    })
}
