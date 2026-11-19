use bevy::prelude::*;
use openzt2_world_state::simulation_time::deterministic_random_stream::DeterministicRng;

#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub(super) struct Daylight {
    pub(super) fraction: f32,
}

#[derive(Resource, Default, PartialEq)]
pub(super) struct EnvironmentDaylightFraction(pub(super) Option<f32>);

#[derive(Component, Copy, Clone, PartialEq)]
pub(super) struct ObjectAmbientLight(pub(super) Vec3, pub(super) Option<Vec3>);

#[derive(Component, Clone, Copy)]
pub(super) struct EnvironmentRandom(pub(super) DeterministicRng);

/// Active authored quake presentation on the world environment entity.
#[derive(Component)]
pub(super) struct Earthquake;

#[derive(EntityEvent)]
pub(super) struct EarthquakeRefreshed {
    pub(super) entity: Entity,
}
