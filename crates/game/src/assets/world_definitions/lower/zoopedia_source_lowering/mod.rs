use super::world_definition_lowering_tables::WorldDefinitionLoweringTables;
use super::world_definition_source_value_reading_and_conversion::{
    asset, asset_list, id, number_or,
};
use crate::assets::source_document::ordered_source_document_types::{
    OrderedSourceDocument, OrderedSourceDocumentNode,
};
use crate::assets::source_document::resolved_source_record_index::{BindError, RecordView};
use crate::assets::source_document::source_document_semantic_name::canonicalize_source_document_record_key;
use openzt2_game_data::world_definitions::catalogue_and_progression::zoopedia_entry_types::ZoopediaEntry;
use openzt2_game_data::AssetId;
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn bind_zoopedia(
    record: &RecordView<'_, '_>,
    output: &mut WorldDefinitionLoweringTables,
) -> Result<(), BindError> {
    let related = record
        .value(&["related"])
        .map(asset_list)
        .unwrap_or_default();
    output.document.zoopedia.push(ZoopediaEntry {
        id: id(record.key),
        subject: asset(record, &["subject"]),
        title_key: asset(record, &["titleKey", "title"]),
        body_key: asset(record, &["bodyKey", "body"]),
        image: asset(record, &["image"]),
        related,
        order: number_or(record, &["order"], 0)?,
    });
    Ok(())
}

/// Lowers the original BFHelpComponent's additive BFHelpEntry documents into the
/// existing canonical Zoopedia table.  Entry documents are application data
/// despite their historical UI/ path; no source tree survives lowering.
pub(super) fn bind_zoopedia_entries(
    documents: &[OrderedSourceDocument],
    output: &mut WorldDefinitionLoweringTables,
) -> Result<(), BindError> {
    #[derive(Default)]
    struct Entry {
        order: u16,
        order_key: String,
        children: Vec<String>,
        child_set: BTreeSet<String>,
    }

    fn visit(
        document: &OrderedSourceDocument,
        element: &'_ OrderedSourceDocumentNode,
        parent: Option<&str>,
        entries: &mut BTreeMap<String, Entry>,
        next_order: &mut u16,
    ) -> Result<(), BindError> {
        let current =
            if canonicalize_source_document_record_key(element.name.as_str()) == "bfhelpentry" {
                let authored = element
                    .attribute_named_any(&["entry"])
                    .filter(|value| !value.trim().is_empty())
                    .ok_or_else(|| {
                        BindError::at(
                            document,
                            element,
                            "BFHelpEntry has no non-empty entry identity",
                        )
                    })?;
                let key = canonicalize_source_document_record_key(authored);
                if !entries.contains_key(&key) {
                    let order = *next_order;
                    *next_order = next_order.checked_add(1).ok_or_else(|| {
                        BindError::at(document, element, "BFHelpEntry set exceeds u16 ordering")
                    })?;
                    entries.insert(
                        key.clone(),
                        Entry {
                            order,
                            order_key: canonicalize_source_document_record_key(
                                element.attribute_named_any(&["order"]).unwrap_or(authored),
                            ),
                            ..Default::default()
                        },
                    );
                } else if let Some(order) = element.attribute_named_any(&["order"]) {
                    // BFHelpEntry documents are additive: one file may introduce
                    // a node while another occurrence supplies its ordering key.
                    // Preserve that authored key regardless of document traversal
                    // order. Keeping only the first occurrence moves expansion
                    // entries such as the Extinct Research Lab behind ordinary
                    // alphabetic categories.
                    entries
                        .get_mut(&key)
                        .expect("existing Zoopedia entry")
                        .order_key = canonicalize_source_document_record_key(order);
                }
                if let Some(parent) = parent {
                    let parent = entries.get_mut(parent).ok_or_else(|| {
                        BindError::at(document, element, "BFHelpEntry parent was not lowered")
                    })?;
                    if parent.child_set.insert(key.clone()) {
                        parent.children.push(key.clone());
                    }
                }
                Some(key)
            } else {
                parent.map(str::to_owned)
            };
        for child in element.element_children() {
            visit(document, child, current.as_deref(), entries, next_order)?;
        }
        Ok(())
    }

    let mut entries = BTreeMap::<String, Entry>::new();
    let mut next_order = 0_u16;
    for document in documents {
        if document
            .root
            .element_children()
            .any(|child| canonicalize_source_document_record_key(&child.name) == "bfhelpentry")
        {
            visit(
                document,
                &document.root,
                None,
                &mut entries,
                &mut next_order,
            )?;
        }
    }
    if entries.is_empty() {
        return Ok(());
    }

    let order_keys = entries
        .iter()
        .map(|(key, entry)| (key.clone(), entry.order_key.clone()))
        .collect::<BTreeMap<_, _>>();
    for (key, mut entry) in entries {
        entry.children.sort_by(|left, right| {
            order_keys
                .get(left)
                .cmp(&order_keys.get(right))
                .then_with(|| left.cmp(right))
        });
        let related = entry
            .children
            .iter()
            .map(|child| AssetId::from_key(child))
            .collect();
        output.document.zoopedia.push(ZoopediaEntry {
            id: AssetId::from_key(&key),
            subject: AssetId::from_key(&key),
            title_key: AssetId::from_key(&format!("{key}:entry")),
            body_key: AssetId::from_key(&format!("{key}:text")),
            image: AssetId::default(),
            related,
            order: entry.order,
        });
    }
    Ok(())
}
