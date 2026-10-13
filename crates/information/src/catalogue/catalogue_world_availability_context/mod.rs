use bevy::prelude::*;

use openzt2_world_state::game_session_types::WorldSessionMode;
use openzt2_world_state::world_spawn::selected_world_identity::SelectedWorldIdentity;
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;
use openzt2_zoo_state::progression::adoption_and_content_availability_types::ScenarioContentAvailability;

pub(crate) fn resolve_catalogue_world_session_mode_and_scenario_availability<'a>(
    selected_worlds: &'a Query<
        '_,
        '_,
        (
            Ref<SelectedWorldIdentity>,
            Option<Ref<ScenarioContentAvailability>>,
        ),
        With<WorldRoot>,
    >,
) -> (
    Option<WorldSessionMode>,
    Option<&'a ScenarioContentAvailability>,
) {
    selected_worlds
        .single()
        .map_or((None, None), |(world_selection, scenario_availability)| {
            (
                Some(world_selection.mode),
                scenario_availability.map(Ref::into_inner),
            )
        })
}

pub(crate) fn catalogue_world_session_or_scenario_availability_changed(
    selected_worlds: &Query<
        '_,
        '_,
        (
            Ref<SelectedWorldIdentity>,
            Option<Ref<ScenarioContentAvailability>>,
        ),
        With<WorldRoot>,
    >,
) -> bool {
    selected_worlds
        .iter()
        .any(|(world_selection, scenario_availability)| {
            world_selection.is_changed()
                || scenario_availability.is_some_and(|availability| availability.is_changed())
        })
}
