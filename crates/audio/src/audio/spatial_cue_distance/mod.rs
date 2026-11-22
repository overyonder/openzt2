use bevy::audio::{SpatialListener, SpatialScale};
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use openzt2_zoo_state::terrain::terrain_water_view_state::TerrainWaterCameraContact;

use super::audio_cue_playback::PositionedListenerWater;

#[derive(Component, Clone, Copy)]
pub(super) struct SpatialCueDistance {
    minimum: f32,
    maximum: f32,
}

impl SpatialCueDistance {
    pub(super) fn from_authored_range(range: Option<[f32; 2]>) -> Self {
        let [minimum, maximum] = range
            .filter(|[min, max]| min.is_finite() && max.is_finite() && *min >= 0.0 && max >= min)
            .unwrap_or([20.0, 200.0]);
        Self {
            minimum: minimum.max(0.0001),
            maximum,
        }
    }

    pub(super) fn panning_scale(self) -> SpatialScale {
        // Keep both ears within Rodio's full-volume radius throughout the audible
        // range. The cue applies its own attenuation while Rodio handles panning.
        SpatialScale::new(0.5 / (self.maximum + 4.0))
    }

    fn gain(self, distance: f32) -> f32 {
        if !distance.is_finite() || distance > self.maximum {
            0.0
        } else if distance <= self.minimum + 0.0001 {
            1.0
        } else {
            self.minimum / distance
        }
    }
}

#[derive(SystemParam)]
pub(super) struct SpatialCueListener<'w, 's> {
    listeners: Query<'w, 's, (&'static GlobalTransform, &'static SpatialListener)>,
    emitters: Query<
        'w,
        's,
        (
            &'static GlobalTransform,
            Option<&'static PositionedListenerWater>,
        ),
    >,
    camera_contacts: Query<'w, 's, &'static TerrainWaterCameraContact>,
}

impl SpatialCueListener<'_, '_> {
    pub(super) fn gain(&self, policy: SpatialCueDistance, emitter: Entity) -> f32 {
        let Ok((pose, water)) = self.emitters.get(emitter) else {
            return 0.0;
        };
        let underwater = self
            .camera_contacts
            .single()
            .ok()
            .and_then(|contact| contact.underwater_status);
        if water.is_some_and(|water| !water.allows(underwater)) {
            return 0.0;
        }
        let listener_position = self
            .listeners
            .iter()
            .next()
            .map_or(Vec3::ZERO, |(pose, _)| pose.translation());
        policy.gain(listener_position.distance(pose.translation()))
    }
}
