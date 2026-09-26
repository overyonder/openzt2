use super::source_element_tree_search::{
    authored_type_family_elements_named, find_descendant, find_presentation_component,
};
use crate::assets::source_document::{
    blue_fang_source_numeric_lexeme::parse_blue_fang_source_numeric_lexeme,
    resolved_source_record_index::{BindError, RecordView},
    source_document_semantic_name::canonicalize_source_document_record_key,
};
use openzt2_game_data::{
    world_definitions::world_objects::WorldObjectNamedPhysicalPresentationDefinition, AssetId,
};
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn lower_named_physical_presentations(
    record: &RecordView<'_, '_>,
    scene_paths: &BTreeMap<String, String>,
) -> Result<Vec<WorldObjectNamedPhysicalPresentationDefinition>, BindError> {
    let mut presentations = Vec::new();
    let mut seen = BTreeSet::new();
    for binder in authored_type_family_elements_named(record, "BFNamedBinder") {
        let Some(name) = binder.attribute_named_any(&["binderName"]) else {
            continue;
        };
        let Some(component) = find_presentation_component(binder) else {
            continue;
        };
        if !seen.insert(canonicalize_source_document_record_key(name)) {
            continue;
        }
        let required = match binder
            .attribute_named_any(&["required"])
            .map(|value| value.trim().to_ascii_lowercase())
            .as_deref()
        {
            None | Some("" | "true" | "1") => true,
            Some("false" | "0") => false,
            _ => {
                return Err(BindError::record(
                    record,
                    format!("physical binder {name:?} has an unsupported required value"),
                ))
            }
        };
        let reference = component
            .attribute_named_any(&["modelfile", "actorfile"])
            .expect("presentation lookup requires a reference");
        let scene = scene_paths.get(&canonicalize_source_document_record_key(reference));
        if scene.is_none() && find_descendant(binder, "BFPhysObj").is_none() {
            if !required {
                continue;
            }
            return Err(BindError::record(
                record,
                format!("physical binder {name:?} references unresolved scene {reference:?}"),
            ));
        }
        let scale = component
            .attribute_named_any(&["scale"])
            .map(|value| {
                parse_blue_fang_source_numeric_lexeme::<f32>(value)
                    .filter(|scale| scale.is_finite() && *scale > 0.0)
                    .ok_or_else(|| {
                        BindError::record(record, "physical binder scale must be positive")
                    })
            })
            .transpose()?
            .unwrap_or(1.0);
        presentations.push(WorldObjectNamedPhysicalPresentationDefinition {
            name: AssetId::from_key(&canonicalize_source_document_record_key(name)),
            prefab: scene.map_or_else(AssetId::default, |scene| AssetId::from_virtual_path(scene)),
            scale,
            required,
        });
    }
    Ok(presentations)
}
