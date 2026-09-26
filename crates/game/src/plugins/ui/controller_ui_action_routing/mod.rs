//! Match controller intents to authored UI actions, never key macros.

use super::{
    authored_ui_action_projection_components::UiConstructionActions,
    authored_ui_activation_contracts::UiNodeActivated,
    authored_ui_interaction_enabled_state::UiInteractionEnabled,
    authored_ui_node_projection_components::{UiDocumentOwner, UiDocumentRoot, UiNodeId},
};
use crate::{
    assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset,
    plugins::input::input_types::{ActionRequest, ActionSource, GameAction},
};
use bevy::prelude::*;
use openzt2_game_data::ui_document::action::{construction::UiConstructionAction, UiTrigger};

pub(super) fn route_controller_authored_actions(
    mut requests: MessageReader<ActionRequest>,
    documents: Res<Assets<UiDocumentAsset>>,
    roots: Query<&UiDocumentRoot>,
    nodes: Query<(
        Entity,
        &UiDocumentOwner,
        Option<&UiConstructionActions>,
        Option<&UiNodeId>,
        &InheritedVisibility,
        &UiInteractionEnabled,
    )>,
    mut activations: MessageWriter<UiNodeActivated>,
    tool: Res<crate::plugins::construction::construction_tool_and_placement_policy_types::ConstructionTool>,
) {
    for request in requests
        .read()
        .filter(|request| matches!(request.source, ActionSource::Controller(_)))
    {
        if !matches!(
            request.action,
            GameAction::OverviewMap
                | GameAction::DecreaseBrushSize
                | GameAction::IncreaseBrushSize
                | GameAction::RotateObjectLeft
                | GameAction::RotateObjectRight
        ) {
            continue;
        }
        if matches!(request.action, GameAction::RotateObjectLeft | GameAction::RotateObjectRight)
            && matches!(*tool, crate::plugins::construction::construction_tool_and_placement_policy_types::ConstructionTool::Place(_)) { continue; }
        for (entity, owner, construction, node_id, visible, enabled) in &nodes {
            if !visible.get() || !enabled.0 {
                continue;
            }
            let Some(document) = roots
                .get(owner.0)
                .ok()
                .and_then(|root| documents.get(&root.document))
            else {
                continue;
            };
            let trigger = match request.action {
                GameAction::RotateObjectLeft | GameAction::RotateObjectRight => construction
                    .and_then(|actions| {
                        actions
                            .authored_action_records(document)
                            .find_map(|record| match record.action {
                                UiConstructionAction::RotateSelected { steps }
                                    if steps.signum()
                                        == if request.action == GameAction::RotateObjectLeft {
                                            -1
                                        } else {
                                            1
                                        } =>
                                {
                                    Some(record.trigger.clone())
                                }
                                _ => None,
                            })
                    }),
                GameAction::DecreaseBrushSize | GameAction::IncreaseBrushSize => construction
                    .and_then(|actions| {
                        actions
                            .authored_action_records(document)
                            .find_map(|record| match record.action {
                                UiConstructionAction::SetTerrainCursorSize { direction }
                                    if direction.signum()
                                        == if request.action == GameAction::DecreaseBrushSize {
                                            -1
                                        } else {
                                            1
                                        } =>
                                {
                                    Some(record.trigger.clone())
                                }
                                _ => None,
                            })
                    }),
                GameAction::OverviewMap => node_id
                    .and_then(|id| {
                        document
                            .canonical_ui_document()
                            .nodes
                            .get(id.index as usize)
                    })
                    .filter(|node| node.name.eq_ignore_ascii_case("overviewmap"))
                    .map(|_| UiTrigger::Press),
                _ => None,
            };
            let Some(trigger) = trigger.and_then(|trigger| match trigger {
                UiTrigger::Press => Some(UiTrigger::Press),
                UiTrigger::On => Some(UiTrigger::On),
                UiTrigger::Off => Some(UiTrigger::Off),
                _ => None,
            }) else {
                continue;
            };
            activations.write(UiNodeActivated {
                node: entity,
                source: request.source,
                trigger,
            });
            break;
        }
    }
}
