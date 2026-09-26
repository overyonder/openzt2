use std::io;

use openzt2_game_data::behavior::{
    action::target_test::{BehaviorTargetTestAction, BehaviorTargetTestKind},
    action_record::BehaviorAction,
};

use crate::assets::source_document::ordered_source_document_types::OrderedSourceDocumentNode as DataNode;

use super::source_value_reading::{
    read_nonnegative_centimetres_attribute, read_optional_asset_identifier_attribute,
};

pub(super) fn lower_target_test_action_source_node(
    target_test_action_source_node: &DataNode,
) -> io::Result<Option<BehaviorAction>> {
    let target_test_action_kind = match target_test_action_source_node.name.as_str() {
        "ZTBehPlaceTarget" | "ZTBehTestTargetPos" => BehaviorTargetTestKind::Position,
        "ZTBehTargetFence" => BehaviorTargetTestKind::Fence,
        _ => return Ok(None),
    };

    Ok(Some(BehaviorAction::TargetTest(BehaviorTargetTestAction {
        target_test_kind: target_test_action_kind,
        target_definition_asset_id: read_optional_asset_identifier_attribute(
            target_test_action_source_node,
            "targetType",
        )
        .or_else(|| {
            read_optional_asset_identifier_attribute(target_test_action_source_node, "fenceType")
        }),
        success_behavior_set_asset_id: read_optional_asset_identifier_attribute(
            target_test_action_source_node,
            "targetBeh",
        )
        .or_else(|| {
            read_optional_asset_identifier_attribute(target_test_action_source_node, "legalBeh")
        })
        .or_else(|| {
            read_optional_asset_identifier_attribute(target_test_action_source_node, "behSet")
        }),
        failure_behavior_set_asset_id: read_optional_asset_identifier_attribute(
            target_test_action_source_node,
            "illegalBeh",
        ),
        target_radius_cm: read_nonnegative_centimetres_attribute(
            target_test_action_source_node,
            "targetRadius",
        )?
        .max(read_nonnegative_centimetres_attribute(
            target_test_action_source_node,
            "searchDistance",
        )?),
    })))
}
