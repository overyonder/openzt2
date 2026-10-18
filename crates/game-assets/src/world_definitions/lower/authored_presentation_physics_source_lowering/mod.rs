//! Binder-local authored physical presentation settings.

use std::collections::BTreeMap;

use openzt2_game_data::{
    world_definitions::world_objects::WorldObjectPresentationCausticPolicy, AssetId,
};

use crate::source_document::{
    blue_fang_source_numeric_lexeme::parse_blue_fang_source_numeric_lexeme,
    ordered_source_document_types::OrderedSourceDocumentNode,
    resolved_source_record_index::{BindError, RecordView},
    source_document_semantic_name::{
        canonicalize_source_document_record_key, source_document_names_are_semantically_equal,
    },
};

use super::{
    source_element_tree_search::{
        authored_type_family_elements_named, find_presentation_component,
    },
    world_definition_source_value_reading_and_conversion::element_bool,
};

#[derive(Default)]
struct AuthoredBinderPhysics {
    no_caustics: Option<bool>,
    splash_scale: Option<f32>,
    is_decal: bool,
}

pub(super) fn lower_authored_presentation_physics(
    record: &RecordView<'_, '_>,
) -> Result<(Vec<WorldObjectPresentationCausticPolicy>, Option<f32>), BindError> {
    let mut binders = BTreeMap::<String, AuthoredBinderPhysics>::new();
    // Source families arrive most-specific first. An omitted attribute inherits
    // independently of the other attributes on the same component.
    for binder in authored_type_family_elements_named(record, "BFNamedBinder") {
        let Some(name) = binder.attribute_named_any(&["binderName"]) else {
            continue;
        };
        let name = canonicalize_source_document_record_key(name);
        if name.is_empty() {
            continue;
        }
        let physics = binders.entry(name).or_default();
        read_binder_local_physics_children(binder, record, physics)?;
    }
    let direct_sources = std::iter::once(record.source_document_element())
        .chain(
            record
                .type_tokens()
                .into_iter()
                .rev()
                .filter(|name| name != record.key)
                .filter_map(|name| record.find_resolved_source_record_by_reference(&name))
                .map(|family| family.source_document_element()),
        )
        .collect::<Vec<_>>();
    if binders.is_empty()
        && (direct_sources.iter().any(|source| {
            find_presentation_component(source).is_some() || has_direct_decal_presentation(source)
        }) || record
            .value(&["prefab", "model", "visual"])
            .is_some_and(|value| !value.is_empty()))
    {
        let physics = binders.entry("mainobj".to_owned()).or_default();
        for source in direct_sources {
            read_binder_local_physics_children(source, record, physics)?;
        }
    }
    let surface_effect_strength = binders
        .get("mainobj")
        .and_then(|physics| physics.splash_scale);
    let policies = binders
        .into_iter()
        .map(|(name, physics)| WorldObjectPresentationCausticPolicy {
            binder: AssetId::from_key(&name),
            no_caustics: physics.no_caustics.unwrap_or(false),
            is_decal: physics.is_decal,
        })
        .collect();
    Ok((policies, surface_effect_strength))
}

fn has_direct_decal_presentation(node: &OrderedSourceDocumentNode) -> bool {
    node.element_children().any(|child| {
        !source_document_names_are_semantically_equal(&child.name, "BFNamedBinder")
            && (source_document_names_are_semantically_equal(&child.name, "BFDecalComponent")
                || source_document_names_are_semantically_equal(
                    &child.name,
                    "BFTerrainDecalComponent",
                )
                || has_direct_decal_presentation(child))
    })
}

fn read_binder_local_physics_children(
    node: &OrderedSourceDocumentNode,
    record: &RecordView<'_, '_>,
    physics: &mut AuthoredBinderPhysics,
) -> Result<(), BindError> {
    for child in node.element_children() {
        if source_document_names_are_semantically_equal(&child.name, "BFNamedBinder") {
            continue;
        }
        if source_document_names_are_semantically_equal(&child.name, "BFSharedPhysVars") {
            if physics.no_caustics.is_none() && child.attribute_named_any(&["noCaustics"]).is_some()
            {
                physics.no_caustics = Some(element_bool(child, &["noCaustics"], false)?);
            }
            if physics.splash_scale.is_none() {
                physics.splash_scale = child
                    .attribute_named_any(&["splashScale"])
                    .map(|value| {
                        parse_blue_fang_source_numeric_lexeme::<f32>(value)
                            .filter(|value| value.is_finite())
                            .ok_or_else(|| {
                                BindError::record(record, "invalid physical binder splashScale")
                            })
                    })
                    .transpose()?;
            }
        }
        physics.is_decal |=
            source_document_names_are_semantically_equal(&child.name, "BFDecalComponent")
                || source_document_names_are_semantically_equal(
                    &child.name,
                    "BFTerrainDecalComponent",
                );
        read_binder_local_physics_children(child, record, physics)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
