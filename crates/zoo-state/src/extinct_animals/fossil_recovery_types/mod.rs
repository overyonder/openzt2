use bevy::prelude::*;
use openzt2_game_data::AssetId;

use openzt2_world_state::simulation_time::deterministic_random_stream::DeterministicRng;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct FossilRecoverySite {
    pub fossil_set_definition_identifier: AssetId,
    pub all_pieces_collected: bool,
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ActiveFossilSearchMarker;

/// Current fossil-sonar result for the active mode controller.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq)]
pub struct ActiveFossilSonarArtifactTracking {
    pub selected_artifact: Option<Entity>,
    pub distance_squared: f32,
    pub sonar_strength: f32,
}

#[derive(Component, Debug, Clone, Copy)]
pub struct FossilDiscoveryRandomStream(pub DeterministicRng);

#[derive(Message, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ActivateFossilSiteMarkersRequest;

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiscoverFossilAtSiteRequest {
    pub fossil_site: Entity,
    pub interaction_tool: Entity,
}
