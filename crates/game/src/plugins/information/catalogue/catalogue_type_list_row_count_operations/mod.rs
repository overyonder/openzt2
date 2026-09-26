use bevy::prelude::*;
use openzt2_game_data::AssetId;

use crate::assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::progression::adoption_and_content_availability_types::ScenarioContentAvailability;
use crate::plugins::progression::unlock_types::UnlockedCatalogueDefinitionSet;
use crate::plugins::ui::authored_reusable_list_and_table_runtime_types::SetUiListRowCount;
use crate::plugins::ui::authored_reusable_list_and_table_runtime_types::UiTypeListPolicy;
use crate::plugins::ui::authored_ui_node_projection_components::UiDocumentOwner;
use crate::plugins::ui::authored_ui_node_projection_components::UiDocumentRoot;
use crate::plugins::ui::authored_ui_node_projection_components::UiNodeId;
use crate::plugins::world_spawn::selected_world_identity::SelectedWorldIdentity;
use crate::plugins::world_spawn::world_membership_types::WorldRoot;

use super::{
    super::catalogue_types::TypeListFilter,
    catalogue_type_list_policy_operations::count_catalogue_entries_in_authored_type_list,
    catalogue_world_availability_context::{
        catalogue_world_session_or_scenario_availability_changed,
        resolve_catalogue_world_session_mode_and_scenario_availability,
    },
};
use crate::plugins::ui::authored_ui_action_projection_components::UiPopulateCatalogueTypeListActions;
use crate::plugins::ui::authored_ui_activation_contracts::UiNodeActivated;

pub(in crate::plugins::information) fn populate_activated_authored_catalogue_type_list_row_counts(
    mut ui_node_activations: MessageReader<UiNodeActivated>,
    ui_document_assets: Res<Assets<UiDocumentAsset>>,
    populate_type_list_action_nodes: Query<(&UiPopulateCatalogueTypeListActions, &UiDocumentOwner)>,
    ui_document_roots: Query<&UiDocumentRoot>,
    authored_type_list_nodes: Query<(Entity, &UiDocumentOwner, &UiNodeId, &UiTypeListPolicy)>,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    unlocked_catalogue_definitions: Res<UnlockedCatalogueDefinitionSet>,
    selected_worlds: Query<
        (
            Ref<SelectedWorldIdentity>,
            Option<Ref<ScenarioContentAvailability>>,
        ),
        With<WorldRoot>,
    >,
    type_list_filters: Query<&TypeListFilter>,
    mut row_count_requests: MessageWriter<SetUiListRowCount>,
) {
    let (world_session_mode, scenario_availability) =
        resolve_catalogue_world_session_mode_and_scenario_availability(&selected_worlds);

    for activation in ui_node_activations.read() {
        let Ok((action_records, document_owner)) =
            populate_type_list_action_nodes.get(activation.node)
        else {
            continue;
        };
        let Ok(document_root) = ui_document_roots.get(document_owner.0) else {
            continue;
        };
        let Some(document) = ui_document_assets.get(&document_root.document) else {
            continue;
        };

        for action_record in action_records.authored_action_records(document) {
            if activation.trigger != action_record.trigger {
                continue;
            }
            let target_type_list_node_identifier = AssetId(action_record.target_type_list_node.0);
            let Some(world_definitions) = active_world_definitions.get(&world_definition_assets)
            else {
                continue;
            };

            for (list_entity, candidate_document_owner, node_identifier, type_list_policy) in
                &authored_type_list_nodes
            {
                if candidate_document_owner.0 != document_owner.0
                    || node_identifier.id != target_type_list_node_identifier
                {
                    continue;
                }
                row_count_requests.write(SetUiListRowCount {
                    list: list_entity,
                    count: count_catalogue_entries_in_authored_type_list(
                        type_list_policy,
                        type_list_filters
                            .get(list_entity)
                            .copied()
                            .unwrap_or_default(),
                        world_definitions,
                        world_session_mode,
                        scenario_availability,
                        &unlocked_catalogue_definitions,
                    ),
                });
            }
        }
    }
}

/// Requests exactly the reusable row entities required by every visible
/// authored type list whose policy, filter, definitions, or world changed.
pub(in crate::plugins::information) fn request_visible_catalogue_type_list_row_counts(
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    unlocked_catalogue_definitions: Res<UnlockedCatalogueDefinitionSet>,
    selected_worlds: Query<
        (
            Ref<SelectedWorldIdentity>,
            Option<Ref<ScenarioContentAvailability>>,
        ),
        With<WorldRoot>,
    >,
    type_lists: Query<(
        Entity,
        Ref<UiTypeListPolicy>,
        Option<Ref<TypeListFilter>>,
        Ref<InheritedVisibility>,
    )>,
    mut removed_type_list_filters: RemovedComponents<TypeListFilter>,
    mut row_count_requests: MessageWriter<SetUiListRowCount>,
    mut previous_catalogue_revision: Local<u64>,
) {
    let world_definitions_changed =
        *previous_catalogue_revision != active_world_definitions.catalogue_revision();
    *previous_catalogue_revision = active_world_definitions.catalogue_revision();
    let removed_type_list_filters = removed_type_list_filters.read().collect::<Vec<_>>();
    let Some(world_definitions) = active_world_definitions.get(&world_definition_assets) else {
        return;
    };
    let (world_session_mode, scenario_availability) =
        resolve_catalogue_world_session_mode_and_scenario_availability(&selected_worlds);

    for (list_entity, type_list_policy, type_list_filter, inherited_visibility) in &type_lists {
        let type_list_filter_changed = type_list_filter.as_ref().is_some_and(Ref::is_changed);
        if !inherited_visibility.get()
            || (!type_list_policy.is_added()
                && !type_list_filter_changed
                && !removed_type_list_filters.contains(&list_entity)
                && !inherited_visibility.is_changed()
                && !world_definitions_changed
                && !unlocked_catalogue_definitions.is_changed()
                && !catalogue_world_session_or_scenario_availability_changed(&selected_worlds))
        {
            continue;
        }

        let row_count = count_catalogue_entries_in_authored_type_list(
            &type_list_policy,
            type_list_filter.as_deref().copied().unwrap_or_default(),
            world_definitions,
            world_session_mode,
            scenario_availability,
            &unlocked_catalogue_definitions,
        );
        debug!(
            list = ?list_entity,
            included_kinds = ?type_list_policy.included_kinds,
            excluded_kinds = ?type_list_policy.excluded_kinds,
            row_count,
            "resolved catalogue type-list"
        );
        row_count_requests.write(SetUiListRowCount {
            list: list_entity,
            count: row_count,
        });
    }
}
