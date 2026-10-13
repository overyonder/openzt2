use std::io;

use openzt2_game_data::{
    behavior::{
        action::place_target::{BehaviorPlaceTargetAction, BehaviorPlacedTargetType},
        action_record::BehaviorAction,
    },
    AssetId,
};

use crate::source_document::ordered_source_document_types::OrderedSourceDocumentNode;

use super::{
    invalid_behavior_source_data, source_value_reading::read_optional_trimmed_string_attribute,
};

/// Lowers the container form of `ZTBehPlaceTarget`. The older direct form,
/// where the action itself walks within `targetRadius` and plays
/// `targetAnim`, has no mapped search or locomotion policy and is rejected.
pub(super) fn lower_place_target_action(
    node: &OrderedSourceDocumentNode,
) -> io::Result<Option<BehaviorAction>> {
    if node.name != "ZTBehPlaceTarget" {
        return Ok(None);
    }
    if node
        .attributes
        .iter()
        .any(|attribute| !matches!(attribute.name(), "container" | "targetType" | "targetBeh"))
        || node.element_children().next().is_some()
    {
        return Err(invalid_behavior_source_data(
            "ZTBehPlaceTarget has unmapped placement policy",
        ));
    }
    let required = |name| {
        read_optional_trimmed_string_attribute(node, name).ok_or_else(|| {
            invalid_behavior_source_data(format!("ZTBehPlaceTarget requires {name}"))
        })
    };
    let placed_type = match required("targetType")?.to_ascii_lowercase().as_str() {
        "s_keepereattype" => BehaviorPlacedTargetType::KeeperFood,
        "s_keeperdrinktype" => BehaviorPlacedTargetType::KeeperDrink,
        _ => {
            return Err(invalid_behavior_source_data(
                "ZTBehPlaceTarget targetType names no supported target attribute",
            ))
        }
    };
    Ok(Some(BehaviorAction::PlaceTarget(
        BehaviorPlaceTargetAction {
            placed_type,
            holding_container: AssetId::from_key(&required("container")?.to_ascii_lowercase()),
            placement_behavior_set: AssetId::from_key(&required("targetBeh")?.to_ascii_lowercase()),
        },
    )))
}
