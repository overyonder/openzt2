use bevy::prelude::*;
use openzt2_game_assets::world_definitions::{
    world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
};
use openzt2_zoo_state::{
    behavior_task_execution_types::running_actions::GuestTourDepartureGate,
    guests::guest_simulation_types::{GuestArchetype, GuestNeedAdjustmentDue, GuestRng},
    transport_tours::transport_rider_types::TransportRider,
};

pub(super) fn initialize_tour_need(
    definitions: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    mut guests: Query<
        (Entity, Ref<GuestArchetype>, &mut GuestRng),
        Without<GuestTourDepartureGate>,
    >,
    mut commands: Commands,
) {
    let definitions_changed = definitions.is_changed() || assets.is_changed();
    let Some(definitions) = definitions.get(&assets) else {
        return;
    };
    let Some(tour) = definitions.tour_scoring() else {
        return;
    };
    for (guest, archetype, mut random) in &mut guests {
        if !definitions_changed && !archetype.is_changed() {
            continue;
        }
        let Some(policy) = definitions
            .find_guest(archetype.0)
            .and_then(|guest| guest.view_scoring.tour_need)
        else {
            continue;
        };
        let [minimum, maximum] = policy.start;
        let need = if minimum == maximum {
            minimum
        } else {
            minimum + (maximum - minimum) * random.0.unit_f32()
        };
        commands.entity(guest).insert(GuestTourDepartureGate {
            need,
            threshold: tour.need_threshold,
            policy,
            triggered: need >= policy.trigger,
        });
    }
}

pub(super) fn adjust_due_tour_needs(
    mut due: MessageReader<GuestNeedAdjustmentDue>,
    mut guests: Query<&mut GuestTourDepartureGate, Without<TransportRider>>,
) {
    for event in due.read() {
        if let Ok(mut need) = guests.get_mut(event.0) {
            let adjustment = need.policy.adjustment;
            need.adjust(adjustment);
        }
    }
}
