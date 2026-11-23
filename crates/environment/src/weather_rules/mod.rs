use bevy::prelude::Vec2;
use openzt2_zoo_state::environment::environment_state_types::Wind;

use openzt2_world_state::simulation_time::deterministic_random_stream::DeterministicRng;

pub(super) fn generate_deterministic_wind_from_speed_range(
    speed_range_meters_per_second: [f32; 2],
    random: &mut DeterministicRng,
) -> Wind {
    let speed_meters_per_second = speed_range_meters_per_second[0]
        + (speed_range_meters_per_second[1] - speed_range_meters_per_second[0]) * random.unit_f32();
    let angle_radians = random.unit_f32() * std::f32::consts::TAU;
    Wind {
        direction: Vec2::new(angle_radians.cos(), angle_radians.sin()),
        speed_mps: speed_meters_per_second,
    }
}
