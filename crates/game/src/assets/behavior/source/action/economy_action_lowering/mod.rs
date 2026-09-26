//! Named economic behavior actions use the economy owner's transaction definitions.

use std::io;

use openzt2_game_data::{behavior::action_record::BehaviorAction, AssetId};

use super::{
    invalid_behavior_source_data,
    source_value_reading::{read_boolean_attribute_or_false, read_optional_float_attribute},
};
use crate::assets::source_document::ordered_source_document_types::OrderedSourceDocumentNode;

pub(super) fn lower_economy_action(
    node: &OrderedSourceDocumentNode,
) -> io::Result<Option<BehaviorAction>> {
    if node.name != "ZTBehEconomy" {
        return Ok(None);
    }
    if node.attributes.iter().any(|attribute| {
        !matches!(
            attribute.name(),
            "transactionName" | "costOverride" | "transactionTrackerKey" | "general"
        )
    }) || node.element_children().next().is_some()
    {
        return Err(invalid_behavior_source_data(
            "ZTBehEconomy has unmapped explicit entity context",
        ));
    }
    let transaction = node
        .attribute("transactionName")
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| invalid_behavior_source_data("ZTBehEconomy has no transactionName"))?;
    let cost_override = read_optional_float_attribute(node, "costOverride")?;
    if cost_override.is_some_and(|value| !value.is_finite()) {
        return Err(invalid_behavior_source_data(
            "ZTBehEconomy has nonfinite costOverride",
        ));
    }
    Ok(Some(BehaviorAction::Economy {
        transaction: AssetId::from_key(&transaction.trim().to_ascii_lowercase()),
        cost_override: cost_override.filter(|value| *value >= 0.0),
        tracker: node
            .attribute("transactionTrackerKey")
            .filter(|value| !value.trim().is_empty())
            .map(|value| AssetId::from_key(&value.trim().to_ascii_lowercase())),
        general: read_boolean_attribute_or_false(node, "general")?,
    }))
}
