use bevy::prelude::*;

use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_zoo_state::guests::guest_simulation_types::Guest;

use super::{
    award_and_progression_fact_types::ProgressionFactChanged,
    catalogue_definition_index_queries::find_catalogue_definition_index,
    catalogue_unlock_set_operations::mark_catalogue_definition_unlocked_at_index,
    fame_calculations::{
        select_fame_level_for_rating_and_guest_count, update_current_and_maximum_fame,
    },
    fame_types::Fame,
    rating_types::ZooRating,
    unlock_types::UnlockedCatalogueDefinitionSet,
};

pub(super) fn apply_scenario_maximum_fame_requests(
    mut requests: MessageReader<openzt2_zoo_state::progression::fame_types::SetScenarioMaximumFame>,
    mut fame: ResMut<Fame>,
    mut changed: MessageWriter<ProgressionFactChanged>,
) {
    for request in requests.read() {
        if !request.percent.is_finite() || !(0.0..=100.0).contains(&request.percent) {
            continue;
        }
        if fame
            .maximum_percent_reached
            .is_none_or(|previous| request.percent > previous)
        {
            fame.maximum_percent_reached = Some(request.percent);
            fame.maximum_reached = fame
                .maximum_reached
                .max((request.percent / 10.0).floor() as u8);
            changed.write(ProgressionFactChanged::Fame);
        }
    }
}

pub(super) fn update_fame_and_apply_authored_fame_unlocks(
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    rating: Res<ZooRating>,
    guests: Query<(), With<Guest>>,
    mut fame: ResMut<Fame>,
    mut unlocks: ResMut<UnlockedCatalogueDefinitionSet>,
    mut changed: MessageWriter<ProgressionFactChanged>,
) {
    if !rating.is_changed() || !rating.overall_available {
        return;
    }
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    // An empty threshold set supplies no rating-to-fame rule.
    if definitions.fame_thresholds().next().is_none() {
        return;
    }
    let guest_count = guests.iter().count() as u32;
    let next = select_fame_level_for_rating_and_guest_count(
        definitions.fame_thresholds().map(|threshold| {
            (
                threshold.level,
                threshold.minimum_rating,
                threshold.minimum_guests,
            )
        }),
        rating.overall_permille,
        guest_count,
    );
    if update_current_and_maximum_fame(&mut fame, next) {
        changed.write(ProgressionFactChanged::Fame);
    }
    for threshold in definitions
        .fame_thresholds()
        .filter(|threshold| threshold.level <= u16::from(next))
    {
        let targets = &threshold.unlocks;
        if !targets
            .iter()
            .all(|target| find_catalogue_definition_index(definitions, *target).is_some())
        {
            continue;
        }
        for target in targets {
            let id = *target;
            if let Some(index) = find_catalogue_definition_index(definitions, id) {
                if mark_catalogue_definition_unlocked_at_index(&mut unlocks, index) == Some(true) {
                    changed.write(ProgressionFactChanged::Unlock(id));
                }
            }
        }
    }
}
