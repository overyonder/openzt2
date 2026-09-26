use bevy::prelude::*;

use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::guests::guest_simulation_types::Guest;

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
