use bevy::prelude::*;
use openzt2_game_data::{ui_document::action::information::UiInformationAction, AssetId};

use crate::plugins::ui::authored_ui_node_projection_components::{UiDocumentOwner, UiNodeId};

use super::super::catalogue_types::TypeListFilter;

pub(in crate::plugins::information) fn apply_authored_catalogue_type_list_filter_action(
    action: &UiInformationAction,
    source_document_entity: Entity,
    authored_ui_nodes: &Query<(Entity, &UiDocumentOwner, &UiNodeId)>,
    commands: &mut Commands,
) {
    match action {
        UiInformationAction::SetResearchFilter {
            list,
            unlocked_only,
        } => {
            insert_catalogue_type_list_filter_on_authored_target(
                source_document_entity,
                AssetId(list.0),
                TypeListFilter {
                    unlocked_only: *unlocked_only,
                    kind: None,
                    field_value: None,
                },
                true,
                authored_ui_nodes,
                commands,
            );
        }
        UiInformationAction::SetCatalogueKindFilter { list, kind } => {
            insert_catalogue_type_list_filter_on_authored_target(
                source_document_entity,
                AssetId(list.0),
                TypeListFilter {
                    unlocked_only: false,
                    kind: Some(AssetId(kind.0)),
                    field_value: None,
                },
                false,
                authored_ui_nodes,
                commands,
            );
        }
        UiInformationAction::ClearTypeFilter { list } => {
            for (ui_node_entity, candidate_document_owner, ui_node_identifier) in authored_ui_nodes
            {
                if candidate_document_owner.0 == source_document_entity
                    && ui_node_identifier.id == AssetId(list.0)
                {
                    commands.entity(ui_node_entity).remove::<TypeListFilter>();
                }
            }
        }
        _ => unreachable!("non-filter action sent to the catalogue type-list filter owner"),
    }
}

fn insert_catalogue_type_list_filter_on_authored_target(
    source_document_entity: Entity,
    target_ui_node_identifier: AssetId,
    type_list_filter: TypeListFilter,
    update_research: bool,
    authored_ui_nodes: &Query<(Entity, &UiDocumentOwner, &UiNodeId)>,
    commands: &mut Commands,
) {
    for (ui_node_entity, candidate_document_owner, ui_node_identifier) in authored_ui_nodes {
        if candidate_document_owner.0 == source_document_entity
            && ui_node_identifier.id == target_ui_node_identifier
        {
            commands
                .entity(ui_node_entity)
                .entry::<TypeListFilter>()
                .and_modify(move |mut filter| {
                    if update_research {
                        filter.unlocked_only = type_list_filter.unlocked_only;
                    } else {
                        filter.kind = type_list_filter.kind;
                    }
                })
                .or_insert(type_list_filter);
        }
    }
}
