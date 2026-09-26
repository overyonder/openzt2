use bevy::prelude::*;
use openzt2_game_data::AssetId;

use crate::plugins::simulation_time::deterministic_random_stream::DeterministicRng;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FossilRecoverySite {
    pub(crate) fossil_set_definition_identifier: AssetId,
    pub(crate) all_pieces_collected: bool,
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct ActiveFossilSearchMarker;

/// Current fossil-sonar result for the active mode controller.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq)]
pub(super) struct ActiveFossilSonarArtifactTracking {
    pub(super) selected_artifact: Option<Entity>,
    pub(super) distance_squared: f32,
    pub(super) sonar_strength: f32,
}

#[derive(Component, Debug, Clone, Copy)]
pub(super) struct FossilDiscoveryRandomStream(pub(super) DeterministicRng);

#[derive(Message, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct ActivateFossilSiteMarkersRequest;

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DiscoverFossilAtSiteRequest {
    pub(crate) fossil_site: Entity,
    pub(crate) interaction_tool: Entity,
}
