use bevy::prelude::*;
use openzt2_game_assets::species::species_asset_types::{SpeciesAsset, SpeciesAssets};
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_data::{AssetId, world_definitions::guest_simulation_definitions::GuestMemoryKind};
use openzt2_zoo_state::animal_lifecycle::types::SpeciesHandle;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState;
use openzt2_zoo_state::guests::guest_simulation_types::{Guest, GuestArchetype, GuestFavouriteAnimal, GuestReactionApplied};

use super::guest_view_animals_execution::ActiveGuestAnimalViewingMeter;

#[derive(Component)]
pub(super) struct GuestSpeciesViewCounts(Vec<(AssetId, u32)>);

pub(super) fn initialize_guest_species_view_counts(
    species_assets: Res<Assets<SpeciesAsset>>,
    active_species: Res<SpeciesAssets>,
    guests: Query<Entity, (With<Guest>, Without<GuestSpeciesViewCounts>)>,
    mut commands: Commands,
) {
    if guests.is_empty() {
        return;
    }
    let Some(species) = active_species.get(&species_assets) else {
        return;
    };
    let capacity = species.species().count();
    for guest in &guests {
        commands
            .entity(guest)
            .insert(GuestSpeciesViewCounts(Vec::with_capacity(capacity)));
    }
}

#[allow(
    clippy::type_complexity,
    reason = "view scoring updates each guest's species counters and task-owned meter together"
)]
pub(super) fn apply_accepted_view_events_to_view_meters(
    mut reactions: MessageReader<GuestReactionApplied>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    mut guests: Query<(
        &GuestArchetype,
        &GuestFavouriteAnimal,
        &GlobalTransform,
        &mut GuestSpeciesViewCounts,
        Option<&BehaviorTaskExecutionState>,
        Option<&mut ActiveGuestAnimalViewingMeter>,
    )>,
    animals: Query<(&SpeciesHandle, &GlobalTransform)>,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        reactions.clear();
        return;
    };
    let Some(viewing) = definitions.guest_viewing() else {
        reactions.clear();
        return;
    };
    for reaction in reactions.read() {
        if reaction.kind != GuestMemoryKind::AnimalView {
            continue;
        }
        let Some(origin) = reaction.view_event else {
            continue;
        };
        let Some(row) = definitions
            .find_object(origin.source_definition)
            .and_then(|object| {
                object
                    .view_data
                    .iter()
                    .find(|row| row.name == origin.view_key)
            })
        else {
            continue;
        };
        let Ok((archetype, favourite, position, mut counts, task, meter)) =
            guests.get_mut(reaction.guest)
        else {
            continue;
        };
        let Some(guest) = definitions.find_guest(archetype.0) else {
            continue;
        };
        let Ok((species, animal_position)) = animals.get(reaction.subject) else {
            continue;
        };
        let index = counts
            .0
            .iter()
            .position(|(id, _)| *id == species.species)
            .unwrap_or_else(|| {
                counts.0.push((species.species, 0));
                counts.0.len() - 1
            });
        let count = &mut counts.0[index].1;
        let policy = guest.view_scoring;
        #[allow(
            clippy::cast_precision_loss,
            reason = "authored species discount raises a float to the accumulated view count"
        )]
        let discount = policy
            .species_discount
            .powf(*count as f32)
            .max(policy.minimum_species_discount);
        let distance = position
            .translation()
            .distance(animal_position.translation());
        let distance_factor =
            openzt2_zoo_state::guests::guest_simulation_calculations::animal_view_distance_factor(
                policy,
                distance,
                viewing.ground_range_m,
            );
        let preferred = if favourite.0 == Some(species.species) {
            #[allow(
                clippy::cast_precision_loss,
                reason = "Q16 authored multipliers are converted to native score units"
            )]
            {
                guest.preferred_animal_view_factor_q16 as f32 / 65_536.0
            }
        } else {
            1.0
        };
        let score = row.view_score * preferred * discount * distance_factor;
        if let (Some(task), Some(mut meter)) = (task, meter) {
            if meter.owns(task) {
                meter.apply_view_score(score);
            }
        }
        if row.entertainment_score * preferred * distance_factor != 0.0 {
            *count = count.saturating_add(1);
        }
    }
}
