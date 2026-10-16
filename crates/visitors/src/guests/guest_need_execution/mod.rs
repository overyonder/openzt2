use super::guest_definition_queries::find_guest_definition;
use super::guest_definition_queries::guest_need_rows;
use bevy::prelude::*;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use super::guest_simulation_calculations::apply_q16;
use super::guest_simulation_types::AdjustGuestSatisfaction;
use super::guest_simulation_types::Guest;
use super::guest_simulation_types::GuestAmusement;
use super::guest_simulation_types::GuestArchetype;
use super::guest_simulation_types::GuestDessert;
use super::guest_simulation_types::GuestEnergy;
use super::guest_simulation_types::GuestGift;
use super::guest_simulation_types::GuestHunger;
use super::guest_simulation_types::GuestRestroom;
use super::guest_simulation_types::GuestRng;
use super::guest_simulation_types::GuestSatisfaction;
use super::guest_simulation_types::GuestSocial;
use super::guest_simulation_types::GuestThirst;
use super::guest_simulation_types::GuestViewingNeed;
use super::guest_simulation_types::VisitTime;

pub(crate) fn apply_guest_satisfaction_adjustments(
    mut adjustments: MessageReader<AdjustGuestSatisfaction>,
    mut guests: Query<&mut GuestSatisfaction, With<Guest>>,
) {
    for adjustment in adjustments.read() {
        let Ok(mut satisfaction) = guests.get_mut(adjustment.guest) else {
            continue;
        };
        // The message is wellness percentage points; retain its fractional
        // permille in the canonical component instead of truncating each event.
        satisfaction.add_wellness_q16(adjustment.delta_q16.saturating_mul(10));
    }
}

#[allow(clippy::type_complexity)]
pub(crate) fn decay_guest_needs(
    time: Res<Time<Fixed>>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    mut guests: Query<
        (
            &GuestArchetype,
            &mut VisitTime,
            &mut GuestRng,
            &mut GuestHunger,
            &mut GuestThirst,
            &mut GuestDessert,
            &mut GuestGift,
            &mut GuestEnergy,
            &mut GuestRestroom,
            &mut GuestSocial,
            &mut GuestAmusement,
            Option<&mut GuestViewingNeed>,
            &mut GuestSatisfaction,
        ),
        With<Guest>,
    >,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    let Some(policy) = definitions.need_adjustment() else {
        return;
    };
    if !policy.enabled {
        return;
    }
    let step_ns = u64::try_from(time.timestep().as_nanos()).unwrap_or(u64::MAX);
    for (
        archetype,
        mut visit_time,
        mut rng,
        mut hunger,
        mut thirst,
        mut dessert,
        mut gift,
        mut energy,
        mut restroom,
        mut social,
        mut amusement,
        viewing_need,
        mut satisfaction,
    ) in &mut guests
    {
        if !openzt2_zoo_state::need_adjustment::advance_need_adjustment_timer(
            &mut visit_time.need_adjustment_remaining_ns,
            step_ns,
            policy.delay_steps,
            &mut rng.0,
        ) {
            continue;
        }
        let Some((_, definition)) = find_guest_definition(definitions, archetype.0) else {
            continue;
        };
        let Some(needs) = guest_need_rows(definition) else {
            continue;
        };
        if let Some(policy) = definition.happiness_need {
            satisfaction.add_wellness_q16(policy.adjustment_q16.saturating_mul(-10));
        }
        if let (Some(mut need), Some(policy)) = (viewing_need, definition.viewing_need) {
            need.add_source_delta(policy.adjustment_q16);
        }
        let hunger = &mut *hunger;
        apply_q16(
            &mut hunger.value,
            &mut hunger.residual_q16,
            needs[0].adjustment_q16,
        );
        let thirst = &mut *thirst;
        apply_q16(
            &mut thirst.value,
            &mut thirst.residual_q16,
            needs[1].adjustment_q16,
        );
        let dessert = &mut *dessert;
        apply_q16(
            &mut dessert.value,
            &mut dessert.residual_q16,
            needs[2].adjustment_q16,
        );
        let gift = &mut *gift;
        apply_q16(
            &mut gift.value,
            &mut gift.residual_q16,
            needs[3].adjustment_q16,
        );
        let energy = &mut *energy;
        apply_q16(
            &mut energy.value,
            &mut energy.residual_q16,
            needs[4].adjustment_q16,
        );
        let restroom = &mut *restroom;
        apply_q16(
            &mut restroom.value,
            &mut restroom.residual_q16,
            needs[5].adjustment_q16,
        );
        let social = &mut *social;
        apply_q16(
            &mut social.value,
            &mut social.residual_q16,
            needs[6].adjustment_q16,
        );
        let amusement = &mut *amusement;
        apply_q16(
            &mut amusement.value,
            &mut amusement.residual_q16,
            needs[7].adjustment_q16,
        );
    }
}
