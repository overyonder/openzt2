use bevy::{
    ecs::{relationship::RelationshipTarget, system::SystemParam},
    prelude::*,
};
use openzt2_game_data::{
    world_definitions::catalogue_and_progression::catalogue_definition_types::CatalogueEntry,
    AssetId,
};

use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use openzt2_world_state::game_session_types::WorldSessionMode;
use openzt2_zoo_state::progression::adoption_and_content_availability_types::ScenarioContentAvailability;
use openzt2_zoo_state::progression::catalogue_entry_availability::catalogue_entry_is_available;
use openzt2_zoo_state::progression::catalogue_entry_availability::catalogue_entry_is_visible;
use openzt2_zoo_state::progression::unlock_types::UnlockedCatalogueDefinitionSet;
use openzt2_interface_state::ui::authored_reusable_list_and_table_runtime_types::{
    CatalogueTypeListMemoryPolicy, UiTypeListPolicy,
};

use openzt2_interface_state::information::catalogue_types::{
    ScenarioCatalogueRestriction, ScenarioCatalogueRestrictions, TypeListFilter,
};
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;

#[derive(SystemParam)]
pub(crate) struct CatalogueRestrictions<'w, 's> {
    roots: Query<'w, 's, Ref<'static, ScenarioCatalogueRestrictions>, With<WorldRoot>>,
    restrictions: Query<'w, 's, Ref<'static, ScenarioCatalogueRestriction>>,
}

impl CatalogueRestrictions<'_, '_> {
    pub(super) fn changed(&self) -> bool {
        self.roots.iter().any(|root| {
            root.is_changed()
                || self
                    .restrictions
                    .iter_many(root.iter())
                    .flatten()
                    .any(|restriction| restriction.is_changed())
        })
    }

    pub(super) fn allows(&self, list: Option<AssetId>, definition: AssetId) -> bool {
        let Ok(root) = self.roots.single() else {
            return true;
        };
        let (mut global, mut scoped) = (None, None);
        for restriction in self.restrictions.iter_many(root.iter()).flatten() {
            if restriction.definition != definition {
                continue;
            }
            match restriction.list {
                None => global = Some(restriction.enabled),
                Some(key) if list == Some(key) => scoped = Some(restriction.enabled),
                _ => {}
            }
        }
        scoped.or(global).unwrap_or(true)
    }
}

// Availability checks need the list policy, world restrictions and unlock state.
#[allow(clippy::too_many_arguments)]
pub(crate) fn catalogue_entry_is_included_by_authored_type_list(
    type_list_policy: &UiTypeListPolicy,
    type_list_filter: TypeListFilter,
    catalogue_index: usize,
    catalogue_entry: &CatalogueEntry,
    world_definitions: WorldDefinitionsView<'_>,
    world_session_mode: Option<WorldSessionMode>,
    scenario_availability: Option<&ScenarioContentAvailability>,
    unlocked_catalogue_definitions: &UnlockedCatalogueDefinitionSet,
) -> bool {
    catalogue_entry_matches_authored_type_and_kind_filters(
        type_list_policy,
        type_list_filter,
        catalogue_entry,
    ) && catalogue_entry_is_visible(
        world_definitions,
        catalogue_index,
        catalogue_entry,
        world_session_mode,
        scenario_availability,
        unlocked_catalogue_definitions,
    ) && (!type_list_filter.unlocked_only
        || catalogue_entry_is_available(
            world_definitions,
            catalogue_index,
            catalogue_entry,
            world_session_mode,
            scenario_availability,
            unlocked_catalogue_definitions,
        ))
}

fn catalogue_entry_contains_any_authored_kind(
    catalogue_entry: &CatalogueEntry,
    wanted_kind_identifiers: &[AssetId],
) -> bool {
    wanted_kind_identifiers
        .iter()
        .any(|wanted_kind_identifier| {
            AssetId(catalogue_entry.kind.0) == *wanted_kind_identifier
                || catalogue_entry
                    .kinds
                    .iter()
                    .any(|kind| AssetId(kind.0) == *wanted_kind_identifier)
        })
}

fn catalogue_entry_matches_authored_type_and_kind_filters(
    type_list_policy: &UiTypeListPolicy,
    type_list_filter: TypeListFilter,
    catalogue_entry: &CatalogueEntry,
) -> bool {
    let contains_any_kind = |wanted_kind_identifiers: &[AssetId]| {
        catalogue_entry_contains_any_authored_kind(catalogue_entry, wanted_kind_identifiers)
    };

    (type_list_policy.included_kinds.is_empty()
        || contains_any_kind(&type_list_policy.included_kinds))
        && !contains_any_kind(&type_list_policy.excluded_kinds)
        && type_list_filter
            .kind
            .is_none_or(|kind| contains_any_kind(std::slice::from_ref(&kind)))
        && type_list_filter.field_value.is_none_or(|(field, value)| {
            catalogue_entry.filter_values.iter().any(|candidate| {
                AssetId::from_key(&candidate.field) == field
                    && AssetId::from_key(&candidate.value.to_ascii_lowercase()) == value
            })
        })
}

/// Returns whether a catalogue entry may be selected by an activated TypeList.
fn catalogue_entry_is_selectable_for_type_list(
    type_list_policy: &UiTypeListPolicy,
    type_list_filter: TypeListFilter,
    catalogue_index: usize,
    catalogue_entry: &CatalogueEntry,
    world_definitions: WorldDefinitionsView<'_>,
    world_session_mode: Option<WorldSessionMode>,
    scenario_availability: Option<&ScenarioContentAvailability>,
    unlocked_catalogue_definitions: &UnlockedCatalogueDefinitionSet,
) -> bool {
    catalogue_entry_is_included_by_authored_type_list(
        type_list_policy,
        type_list_filter,
        catalogue_index,
        catalogue_entry,
        world_definitions,
        world_session_mode,
        scenario_availability,
        unlocked_catalogue_definitions,
    ) && catalogue_entry_is_available(
        world_definitions,
        catalogue_index,
        catalogue_entry,
        world_session_mode,
        scenario_availability,
        unlocked_catalogue_definitions,
    )
}

/// Native activation first retains a still-valid selected child; when no such
/// child exists, the activation body scans the populated children in order.
pub(crate) fn choose_remembered_or_first_included_catalogue_entry<'a>(
    type_list_policy: &UiTypeListPolicy,
    type_list_filter: TypeListFilter,
    remembered_definition: Option<AssetId>,
    memory: Option<&CatalogueTypeListMemoryPolicy>,
    world_definitions: WorldDefinitionsView<'a>,
    world_session_mode: Option<WorldSessionMode>,
    scenario_availability: Option<&ScenarioContentAvailability>,
    unlocked_catalogue_definitions: &UnlockedCatalogueDefinitionSet,
    scenario_allows: impl Fn(&CatalogueEntry) -> bool,
) -> Option<(usize, &'a CatalogueEntry)> {
    if let Some(remembered_definition) = remembered_definition {
        if let Some(selected) = world_definitions
            .catalogue_in_authored_purchase_order()
            .find(|&(catalogue_index, catalogue_entry)| {
                AssetId(catalogue_entry.definition.0) == remembered_definition
                    && memory.is_none_or(|memory| memory.remembers(catalogue_entry))
                    && scenario_allows(catalogue_entry)
                    && catalogue_entry_is_selectable_for_type_list(
                        type_list_policy,
                        type_list_filter,
                        catalogue_index,
                        catalogue_entry,
                        world_definitions,
                        world_session_mode,
                        scenario_availability,
                        unlocked_catalogue_definitions,
                    )
            })
        {
            return Some(selected);
        }
    }

    world_definitions
        .catalogue_in_authored_purchase_order()
        .find(|&(catalogue_index, catalogue_entry)| {
            scenario_allows(catalogue_entry)
                && catalogue_entry_is_selectable_for_type_list(
                    type_list_policy,
                    type_list_filter,
                    catalogue_index,
                    catalogue_entry,
                    world_definitions,
                    world_session_mode,
                    scenario_availability,
                    unlocked_catalogue_definitions,
                )
        })
}
