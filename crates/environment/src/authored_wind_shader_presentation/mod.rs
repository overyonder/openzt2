use bevy::prelude::*;
use openzt2_world_state::simulation_time::native_visual_random_stream::NativeVisualRandomStream;
use openzt2_zoo_state::environment::environment_state_types::AuthoredWindShaderPresentationState;
use openzt2_zoo_state::environment::environment_state_types::Wind;

pub(super) fn advance_authored_wind_shader_presentation_state(
    time: Res<Time>,
    mut random: ResMut<NativeVisualRandomStream>,
    mut environments: Query<(Ref<Wind>, &mut AuthoredWindShaderPresentationState)>,
) {
    let elapsed_seconds = time.delta_secs();
    for (wind, mut presentation) in &mut environments {
        if wind.is_changed() {
            presentation.set_direction(wind.direction);
        }
        presentation.advance(*wind, elapsed_seconds, || random.unit_f32());
    }
}
