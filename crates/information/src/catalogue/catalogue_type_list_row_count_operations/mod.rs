use bevy::prelude::*;
use openzt2_game_data::AssetId;

use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_interface_state::information::catalogue_types::TypeListFilter;
use openzt2_interface_state::ui::authored_reusable_list_and_table_runtime_types::{RequestedUiListRowCount, UiTypeListPolicy};
use openzt2_world_state::world_spawn::selected_world_identity::SelectedWorldIdentity;
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;
use openzt2_zoo_state::progression::adoption_and_content_availability_types::ScenarioContentAvailability;
use openzt2_zoo_state::progression::unlock_types::UnlockedCatalogueDefinitionSet;

use super::catalogue_type_list_policy_operations::catalogue_entry_is_included_by_authored_type_list;
use super::catalogue_world_availability_context::{
    catalogue_world_session_or_scenario_availability_changed,
    resolve_catalogue_world_session_mode_and_scenario_availability,
};

#[derive(Component, Default, PartialEq, Eq)]
pub(crate) struct OrderedCatalogueEntries(pub(super) Vec<AssetId>);

pub(crate) fn initialize_authored_catalogue_type_list_state(
    lists: Query<
        (Entity, Has<TypeListFilter>),
        (With<UiTypeListPolicy>, Without<OrderedCatalogueEntries>),
    >,
    mut commands: Commands,
) {
    for (list, has_filter) in &lists {
        let mut list = commands.entity(list);
        list.insert(OrderedCatalogueEntries::default());
        if !has_filter {
            list.insert(TypeListFilter::default());
        }
    }
}

pub(crate) fn reconcile_authored_catalogue_type_lists(
    assets: Res<Assets<WorldDefinitionAsset>>,
    definitions: Res<WorldDefinitions>,
    unlocks: Single<Ref<UnlockedCatalogueDefinitionSet>>,
    worlds: Query<
        (
            Ref<SelectedWorldIdentity>,
            Option<Ref<ScenarioContentAvailability>>,
        ),
        With<WorldRoot>,
    >,
    mut lists: Query<(
        Ref<UiTypeListPolicy>,
        Ref<TypeListFilter>,
        &mut OrderedCatalogueEntries,
        Option<&mut RequestedUiListRowCount>,
    )>,
) {
    let unlocks = unlocks.into_inner();
    let inputs_changed = assets.is_changed()
        || definitions.is_changed()
        || unlocks.is_changed()
        || catalogue_world_session_or_scenario_availability_changed(&worlds);
    let Some(catalogue) = definitions.get(&assets) else {
        return;
    };
    let (mode, scenario) = resolve_catalogue_world_session_mode_and_scenario_availability(&worlds);
    for (policy, filter, mut entries, mut row_count) in &mut lists {
        if !inputs_changed
            && !policy.is_changed()
            && !filter.is_changed()
            && !entries.is_added()
            && !row_count.as_ref().is_some_and(|count| count.is_added())
        {
            continue;
        }
        let included = catalogue
            .catalogue_in_authored_purchase_order()
            .filter(|(index, entry)| {
                catalogue_entry_is_included_by_authored_type_list(
                    &policy, *filter, *index, entry, catalogue, mode, scenario, &unlocks,
                )
            })
            .map(|(_, entry)| entry.definition)
            .take(usize::from(u16::MAX))
            .collect::<Vec<_>>();
        if let Some(count) = row_count.as_mut() {
            count.set_if_neq(RequestedUiListRowCount(u16::try_from(included.len()).ok()));
        }
        entries.set_if_neq(OrderedCatalogueEntries(included));
    }
}
