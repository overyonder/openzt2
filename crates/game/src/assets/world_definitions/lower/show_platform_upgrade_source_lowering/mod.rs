//! Lowering of authored show-platform upgrade transactions to canonical policy.

use crate::assets::source_document::ordered_source_document_types::{
    OrderedSourceDocument, OrderedSourceDocumentNode,
};
use crate::assets::source_document::resolved_source_record_index::BindError;
use crate::assets::source_document::source_document_semantic_name::canonicalize_source_document_record_key;
use openzt2_game_data::world_definitions::animal_shows_and_training::ShowPlatformUpgradePolicy;
use openzt2_game_data::AssetId;

pub(super) fn lower_show_platform_upgrade_transactions_to_policy(
    documents: &[OrderedSourceDocument],
) -> Result<Option<ShowPlatformUpgradePolicy>, BindError> {
    fn visit(
        document: &OrderedSourceDocument,
        element: &'_ OrderedSourceDocumentNode,
        canopy: &mut Option<i64>,
        canopy_resale: &mut Option<i64>,
        television: &mut Option<i64>,
        television_resale: &mut Option<i64>,
    ) -> Result<(), BindError> {
        if canonicalize_source_document_record_key(element.name.as_str()) == "zttransaction" {
            let target = match element
                .attribute_named_any(&["name"])
                .map(canonicalize_source_document_record_key)
                .as_deref()
            {
                Some("purchasecanopy") => Some(&mut *canopy),
                Some("sellcanopy") => Some(&mut *canopy_resale),
                Some("purchasetv") => Some(&mut *television),
                Some("selltv") => Some(&mut *television_resale),
                _ => None,
            };
            if let Some(target) = target {
                let source = element.attribute_named_any(&["cost"]).ok_or_else(|| {
                    BindError::at(
                        document,
                        element,
                        "show-platform upgrade transaction is missing cost",
                    )
                })?;
                let units = source.parse::<f64>().map_err(|_| {
                    BindError::at(
                        document,
                        element,
                        format!("invalid show-platform upgrade transaction cost {source}"),
                    )
                })?;
                let cents = units * 100.0;
                if !cents.is_finite() || cents <= 0.0 || cents > i64::MAX as f64 {
                    return Err(BindError::at(
                        document,
                        element,
                        "show-platform upgrade cost exceeds positive stored cents",
                    ));
                }
                *target = Some(cents.round() as i64);
            }
        }
        for child in element.element_children() {
            visit(
                document,
                child,
                canopy,
                canopy_resale,
                television,
                television_resale,
            )?;
        }
        Ok(())
    }

    let mut canopy = None;
    let mut canopy_resale = None;
    let mut television = None;
    let mut television_resale = None;
    let mut source = None;
    for document in documents.iter().filter(|document| {
        document
            .path
            .key()
            .to_ascii_lowercase()
            .ends_with("entities/objects/buildings/ai/showplatform_mm.xml")
    }) {
        source = Some(document);
        visit(
            document,
            &document.root,
            &mut canopy,
            &mut canopy_resale,
            &mut television,
            &mut television_resale,
        )?;
    }
    match (source, canopy, canopy_resale, television, television_resale) {
        (None, None, None, None, None) => Ok(None),
        (
            Some(_),
            Some(canopy_cost_cents),
            Some(canopy_resale_cents),
            Some(television_cost_cents),
            Some(television_resale_cents),
        ) => Ok(Some(ShowPlatformUpgradePolicy {
            canopy_cost_cents,
            canopy_resale_cents,
            canopy_definition: AssetId::from_key("Canopy"),
            canopy_attachment: AssetId::default(),
            television_cost_cents,
            television_resale_cents,
            television_definition: AssetId::from_key("BigScreenTV"),
            television_attachment: AssetId::from_key("node_trick"),
        })),
        (Some(document), _, _, _, _) => Err(BindError {
            virtual_path: document.path.key(),
            span: document.root.span,
            message: "show platform must define both purchasecanopy and purchaseTV transactions"
                .to_owned(),
        }),
        _ => unreachable!("upgrade values require a source document"),
    }
}
