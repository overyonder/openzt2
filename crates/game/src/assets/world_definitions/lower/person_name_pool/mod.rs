use super::world_definition_lowering_tables::WorldDefinitionLoweringTables;
use super::world_definition_source_value_reading_and_conversion::{id, required_element};
use crate::assets::source_document::ordered_source_document_types::OrderedSourceDocumentNode;
use crate::assets::source_document::resolved_source_record_index::{BindError, RecordView};
use crate::assets::source_document::source_document_semantic_name::canonicalize_source_document_record_key;
use openzt2_game_data::world_definitions::guest_simulation_definitions::PersonNamePool;

pub(super) fn bind_person_names(
    record: &RecordView<'_, '_>,
    output: &mut WorldDefinitionLoweringTables,
) -> Result<(), BindError> {
    let locale_name = record
        .source_path()
        .split('/')
        .nth(1)
        .unwrap_or("original")
        .to_ascii_lowercase();
    // Only the English name pools are loaded.
    if locale_name != "1033" {
        return Ok(());
    }
    let locale = id(&locale_name);
    for pool in record
        .source_document_element()
        .element_children()
        .filter(|child| canonicalize_source_document_record_key(child.name.as_str()) == "locstring")
    {
        let key_text = required_element(&pool, &["_locID", "locID"])?;
        let key = id(key_text);
        // Archive and locale precedence have already been resolved.
        let pool_id = key;
        let delimiter = pool
            .attribute_named_any(&["delimiter"])
            .unwrap_or(" ")
            .to_owned();
        let first_names = pool
            .element_children()
            .find(|child| canonicalize_source_document_record_key(child.name.as_str()) == "first")
            .into_iter()
            .flat_map(OrderedSourceDocumentNode::element_children)
            .filter(|child| canonicalize_source_document_record_key(child.name.as_str()) == "name")
            .map(|candidate| required_element(&candidate, &["name"]).map(str::to_owned))
            .collect::<Result<Vec<_>, _>>()?;
        let last_names = pool
            .element_children()
            .find(|child| canonicalize_source_document_record_key(child.name.as_str()) == "last")
            .into_iter()
            .flat_map(OrderedSourceDocumentNode::element_children)
            .filter(|child| canonicalize_source_document_record_key(child.name.as_str()) == "name")
            .map(|candidate| required_element(&candidate, &["name"]).map(str::to_owned))
            .collect::<Result<Vec<_>, _>>()?;
        if first_names.is_empty() || last_names.is_empty() {
            return Err(BindError::record(
                record,
                "person-name pool must contain first and last candidates",
            ));
        }
        output.document.person_name_pools.push(PersonNamePool {
            id: pool_id,
            locale,
            key,
            delimiter,
            first_names,
            last_names,
        });
    }
    Ok(())
}
