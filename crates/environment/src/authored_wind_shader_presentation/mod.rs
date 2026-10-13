use bevy::prelude::*;
use openzt2_zoo_state::environment::environment_state_types::AuthoredWindShaderPresentationState;
use openzt2_zoo_state::environment::environment_state_types::Wind;

pub(super) fn advance_authored_wind_shader_presentation_state(
    time: Res<Time>,
    mut environments: Query<(&Wind, &mut AuthoredWindShaderPresentationState)>,
) {
    let elapsed_seconds = time.delta_secs();
    for (wind, mut presentation) in &mut environments {
        presentation.advance(*wind, elapsed_seconds);
    }
}
