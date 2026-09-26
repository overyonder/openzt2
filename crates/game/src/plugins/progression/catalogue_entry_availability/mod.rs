use openzt2_game_data::world_definitions::catalogue_and_progression::{
    catalogue_definition_types::{CatalogueEntry, CatalogueFilterFlags},
    research_and_unlock_definition_types::UnlockRequirement,
};
use openzt2_game_data::AssetId;

use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use crate::game_session_types::WorldSessionMode;

use super::{
    adoption_and_content_availability_types::ScenarioContentAvailability,
    catalogue_unlock_set_operations::catalogue_definition_is_unlocked_at_index,
    unlock_types::UnlockedCatalogueDefinitionSet,
};

/// Resolves authored catalogue availability from the selected game mode and
/// progression facts. An entry without an authored unlock requirement is
/// available: the unlock bitset records exceptions, not catalogue membership.
pub(crate) fn catalogue_entry_is_available(
    catalog: WorldDefinitionsView<'_>,
    index: usize,
    entry: &CatalogueEntry,
    mode: Option<WorldSessionMode>,
    scenario: Option<&ScenarioContentAvailability>,
    unlocks: &UnlockedCatalogueDefinitionSet,
) -> bool {
    let definition = AssetId(entry.definition.0);
    let freeform = mode == Some(WorldSessionMode::Freeform);
    let scenario_available = scenario.is_some_and(|availability| availability.contains(definition));

    let mut has_requirement = false;
    let mut always = false;
    let mut has_profile_requirement = false;
    for unlock in catalog.unlock_definitions_targeting_catalogue_definition(definition) {
        has_requirement = true;
        if matches!(&unlock.requirement, UnlockRequirement::Always) {
            always = true;
        }
        if matches!(&unlock.requirement, UnlockRequirement::Profile(_)) {
            has_profile_requirement = true;
        }
    }

    availability_from_facts(
        freeform,
        scenario_available,
        has_requirement,
        always,
        has_profile_requirement,
        u32::try_from(index)
            .ok()
            .is_some_and(|index| catalogue_definition_is_unlocked_at_index(unlocks, index)),
    )
}

const fn availability_from_facts(
    freeform: bool,
    scenario_available: bool,
    has_requirement: bool,
    always: bool,
    has_profile_requirement: bool,
    unlocked: bool,
) -> bool {
    unlocked
        || (!has_profile_requirement
            && (freeform || scenario_available || !has_requirement || always))
}

pub(crate) fn catalogue_entry_is_visible(
    catalog: WorldDefinitionsView<'_>,
    index: usize,
    entry: &CatalogueEntry,
    mode: Option<WorldSessionMode>,
    scenario: Option<&ScenarioContentAvailability>,
    unlocks: &UnlockedCatalogueDefinitionSet,
) -> bool {
    catalogue_entry_is_available(catalog, index, entry, mode, scenario, unlocks)
        || !entry
            .filters
            .contains_all(CatalogueFilterFlags::HIDDEN_UNTIL_UNLOCKED)
}

pub(crate) fn catalogue_entry_can_be_researched(
    catalog: WorldDefinitionsView<'_>,
    entry: &CatalogueEntry,
    availability: impl Iterator<Item = (AssetId, bool)>,
) -> bool {
    let eligible = availability
        .filter(|(_, available)| *available)
        .any(|(id, _)| {
            catalog
                .find_research(id)
                .is_some_and(|research| research.unlocks.contains(&entry.definition))
        });
    eligible
        && catalog
            .research()
            .any(|research| research.unlocks.contains(&entry.definition))
        && !catalog
            .unlock_definitions_targeting_catalogue_definition(entry.definition)
            .any(|unlock| !matches!(unlock.requirement, UnlockRequirement::Research(_)))
}
