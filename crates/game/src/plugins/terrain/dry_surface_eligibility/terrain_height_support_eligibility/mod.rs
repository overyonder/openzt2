use openzt2_game_data::world_definitions::{
    transportation_and_tours::TransportationTrackKind,
    world_objects::{WorldObjectKind, WorldObjectPropertyFlags},
};

use super::TerrainDrySurfaceEligibility;

impl TerrainDrySurfaceEligibility<'_, '_> {
    pub(super) fn has_unconstrained_terrain_height_support(&self) -> Option<bool> {
        let definitions = self.active_definitions.get(&self.definitions)?;
        // Ground objects contribute clearance marks. These only create terrain
        // height limits when an overhead path or track supplies a ceiling.
        // Tanks also register their own floor and wall constraints.
        for definition in &self.objects {
            let object = definitions.find_object(definition.0)?;
            if object
                .properties
                .contains_all(WorldObjectPropertyFlags::WATER_BOUNDARY)
                || object.kind == WorldObjectKind::Tank
                || definitions
                    .find_path(definition.0)
                    .is_some_and(|path| path.elevated)
                || definitions
                    .find_track(definition.0)
                    .is_some_and(|track| track.kind == TransportationTrackKind::Sky)
            {
                return Some(false);
            }
        }
        for path in &self.paths {
            if definitions.find_path(path.definition)?.elevated {
                return Some(false);
            }
        }
        for fence in &self.fences {
            if definitions
                .find_object(fence.definition)?
                .properties
                .contains_all(WorldObjectPropertyFlags::WATER_BOUNDARY)
            {
                return Some(false);
            }
        }
        for track in &self.tracks {
            if definitions.find_track(track.definition)?.kind == TransportationTrackKind::Sky {
                return Some(false);
            }
        }
        Some(true)
    }
}
