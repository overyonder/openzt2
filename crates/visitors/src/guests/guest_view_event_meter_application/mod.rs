use crate::guests::view_event_execution::AppliedAnimalView;
use openzt2_zoo_state::behavior_task_execution_types::running_actions::RunningBehaviorInstruction;
use openzt2_zoo_state::guests::guest_simulation_types::GuestSpeciesViewCounts;
use openzt2_zoo_state::guests::guest_survey_types::GuestSurveyExperience;
use bevy::prelude::*;
use openzt2_zoo_state::progression::fame_types::{GuestSpeciesFameViews, ZooSpeciesFameViews};
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_zoo_state::animal_lifecycle::types::SpeciesHandle;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState;
use openzt2_zoo_state::guests::guest_simulation_types::{GuestArchetype, GuestFavouriteAnimal};

use openzt2_zoo_state::behavior_task_execution_types::running_actions::BehaviorViewingProgress;

#[allow(
    clippy::type_complexity,
    reason = "view scoring updates each guest's species counters and task-owned meter together"
)]
pub(super) fn apply_accepted_view_events_to_view_meters(
    mut reactions: MessageReader<AppliedAnimalView>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    mut guests: Query<(
        &GuestArchetype,
        &GuestFavouriteAnimal,
        &GlobalTransform,
        &mut GuestSpeciesViewCounts,
        &mut GuestSurveyExperience,
        &mut GuestSpeciesFameViews,
        &WorldMember,
        Option<(&BehaviorTaskExecutionState, &RunningBehaviorInstruction)>,
        Option<&mut BehaviorViewingProgress>,
    )>,
    animals: Query<(&SpeciesHandle, &GlobalTransform)>,
    mut zoo_views: Query<&mut ZooSpeciesFameViews>,
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
        let Ok((
            archetype,
            favourite,
            position,
            mut counts,
            mut experience,
            mut fame_views,
            member,
            task,
            meter,
        )) = guests.get_mut(reaction.guest)
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
            .powi(i32::try_from(*count).unwrap_or(i32::MAX))
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
        let score = reaction.view_score * preferred * discount * distance_factor;
        if let (Some((task, instruction)), Some(mut meter)) = (task, meter) {
            if meter.instruction == instruction.id() && instruction.is_current(task) {
                meter.apply_view_score(score);
            }
        }
        experience.saw_favorite_animal |= favourite.0 == Some(species.species);
        let entertainment = reaction.entertainment_score * preferred * distance_factor;
        experience.observe_entertainment(entertainment);
        let fame = fame_views.0.entry(species.species).or_default();
        let previous = *fame;
        fame.events = fame.events.saturating_add(1);
        fame.entertainment += entertainment;
        if let Ok(mut zoo) = zoo_views.get_mut(member.root) {
            zoo.replace_contribution(species.species, previous, *fame);
        }
        if reaction.donation_score != 0.0 {
            *count = count.saturating_add(1);
        }
    }
}
