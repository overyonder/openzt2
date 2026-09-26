use super::source_element_tree_search::{
    authored_type_family_components, authored_type_family_elements_named, find_descendant,
    find_presentation_component,
};
use crate::assets::source_document::{
    blue_fang_source_numeric_lexeme::parse_blue_fang_source_numeric_lexeme,
    ordered_source_document_types::OrderedSourceDocumentNode,
    resolved_source_record_index::{BindError, RecordView},
    source_document_semantic_name::{
        canonicalize_source_document_record_key, source_document_names_are_semantically_equal,
    },
};
use openzt2_game_data::{
    world_definitions::expanding_columns::ExpandingColumnPresentationDefinition, AssetId,
};
use std::collections::BTreeMap;

pub(super) fn lower_expanding_column_presentation(
    record: &RecordView<'_, '_>,
    top_piece: &str,
    entity_scene_paths: &BTreeMap<String, String>,
) -> Result<ExpandingColumnPresentationDefinition, BindError> {
    let expando = authored_type_family_components(record, "ZTExpandoComponent")
        .into_iter()
        .next()
        .ok_or_else(|| BindError::record(record, "column has no ZTExpandoComponent"))?;
    let piece = |name| {
        find_descendant(expando, name)
            .ok_or_else(|| BindError::record(record, format!("column has no {name} piece")))
    };
    let base = piece("Base")?;
    let repeating = find_descendant(expando, "Expando")
        .and_then(|container| find_descendant(container, "Piece1"))
        .ok_or_else(|| BindError::record(record, "column has no Expando/Piece1"))?;
    let top = find_descendant(expando, "Top")
        .and_then(|container| find_descendant(container, top_piece))
        .ok_or_else(|| BindError::record(record, format!("column has no Top/{top_piece}")))?;
    let height = |piece: &OrderedSourceDocumentNode| {
        piece
            .attribute_named_any(&["height"])
            .and_then(parse_blue_fang_source_numeric_lexeme::<f32>)
            .filter(|value| value.is_finite() && *value > 0.0)
            .ok_or_else(|| BindError::record(record, "column piece height must be positive"))
    };
    let prefab = |piece: &OrderedSourceDocumentNode| {
        let binder_name = piece
            .attribute_named_any(&["binderName"])
            .ok_or_else(|| BindError::record(record, "column piece has no binderName"))?;
        authored_type_family_elements_named(record, "BFNamedBinder")
            .into_iter()
            .filter(|binder| {
                binder
                    .attribute_named_any(&["binderName"])
                    .is_some_and(|name| {
                        source_document_names_are_semantically_equal(name, binder_name)
                    })
            })
            .find_map(find_presentation_component)
            .and_then(|component| component.attribute_named_any(&["modelfile", "actorfile"]))
            .and_then(|path| entity_scene_paths.get(&canonicalize_source_document_record_key(path)))
            .map(|path| AssetId::from_virtual_path(path))
            .ok_or_else(|| {
                BindError::record(
                    record,
                    format!("column binder {binder_name:?} has no resolved scene"),
                )
            })
    };
    let attachment = |piece: &OrderedSourceDocumentNode, attribute: &str| {
        piece
            .attribute_named_any(&[attribute])
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(|name| AssetId::from_key(&name.to_ascii_lowercase()))
            .unwrap_or_default()
    };
    Ok(ExpandingColumnPresentationDefinition {
        base_prefab: prefab(base)?,
        repeating_prefab: prefab(repeating)?,
        top_prefab: prefab(top)?,
        base_attachment: attachment(base, "attachOthersNode"),
        repeating_attachment: attachment(repeating, "attachSelfNode"),
        repeating_next_attachment: attachment(repeating, "attachOthersNode"),
        top_attachment: attachment(top, "attachSelfNode"),
        base_height_metres: height(base)?,
        repeating_height_metres: height(repeating)?,
        top_height_metres: height(top)?,
    })
}
