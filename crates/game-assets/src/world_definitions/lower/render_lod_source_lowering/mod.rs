use std::collections::BTreeMap;

use openzt2_game_data::AssetId;

use crate::source_document::{
    blue_fang_source_numeric_lexeme::parse_blue_fang_source_numeric_lexeme,
    ordered_source_document_types::OrderedSourceDocumentNode,
    resolved_source_record_index::{BindError, RecordView},
    source_document_semantic_name::canonicalize_source_document_record_key,
};

use super::{
    super::world_definition_document_asset_and_demand_loaded_dependency_paths::AuthoredRenderLodDistances,
    source_element_tree_search::{authored_type_family_elements_named, find_descendant},
};

pub(super) fn lower_authored_render_lod_distances(
    record: &RecordView<'_, '_>,
    output: &mut BTreeMap<(AssetId, AssetId), AuthoredRenderLodDistances>,
) -> Result<(), BindError> {
    let definition = AssetId::from_key(record.key);
    let mut by_binder = BTreeMap::<AssetId, Vec<&OrderedSourceDocumentNode>>::new();
    for binder in authored_type_family_elements_named(record, "BFNamedBinder") {
        if let Some(name) = binder.attribute_named_any(&["binderName"]) {
            by_binder
                .entry(AssetId::from_key(&canonicalize_source_document_record_key(
                    name,
                )))
                .or_default()
                .push(binder);
        }
    }
    if by_binder.is_empty() {
        by_binder.insert(AssetId::default(), vec![record.source_document_element()]);
    }
    for (binder, family) in by_binder {
        let values = |attribute| {
            let source = family.iter().find_map(|binder| {
                [
                    "BFActorComponent",
                    "BFSceneGraphComponent",
                    "BFRSceneGraphComponent",
                    "BFSimpleLODComponent",
                    "BFNiBoneLODComponent",
                ]
                .iter()
                .find_map(|name| find_descendant(binder, name)?.attribute_named_any(&[attribute]))
            });
            source
                .into_iter()
                .flat_map(str::split_ascii_whitespace)
                .map(|value| {
                    parse_blue_fang_source_numeric_lexeme::<f32>(value)
                        .filter(|value| value.is_finite())
                        .map(|distance| distance * distance)
                        .filter(|distance| distance.is_finite())
                        .ok_or_else(|| {
                            BindError::record(
                                record,
                                format!("invalid render LOD {attribute} distance"),
                            )
                        })
                })
                .collect::<Result<Vec<_>, _>>()
        };
        let switch_distance_squared = values("switch")?;
        let minimum_switch_distance_squared = values("minSwitch")?;
        if !switch_distance_squared.is_empty() || !minimum_switch_distance_squared.is_empty() {
            output.insert(
                (definition, binder),
                AuthoredRenderLodDistances {
                    switch_distance_squared,
                    minimum_switch_distance_squared,
                },
            );
        }
    }
    Ok(())
}
