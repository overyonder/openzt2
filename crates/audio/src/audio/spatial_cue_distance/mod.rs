use bevy::audio::{SpatialListener, SpatialScale};
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use openzt2_game_data::audio::AudioMixerPolicy;
use openzt2_zoo_state::terrain::terrain_water_view_state::TerrainWaterCameraContact;

use super::audio_cue_playback::PositionedListenerWater;

#[derive(Component, Clone, Copy)]
pub(super) struct SpatialCueDistance {
    minimum: f32,
    maximum: f32,
    world_cutoff: f32,
}

impl SpatialCueDistance {
    pub(super) fn from_authored_range(
        range: Option<[f32; 2]>,
        mixer: Option<&AudioMixerPolicy>,
        uses_world_cutoff: bool,
    ) -> Self {
        let [minimum, maximum] = range
            .filter(|[min, max]| min.is_finite() && max.is_finite() && *min >= 0.0 && max >= min)
            .unwrap_or_else(|| {
                mixer.map_or([2500.0, 3000.0], |policy| {
                    [policy.spatial_in_range, policy.spatial_out_range]
                })
            });
        let world_cutoff = if uses_world_cutoff {
            mixer.map_or(45.0, |policy| policy.max_world_distance)
        } else {
            f32::INFINITY
        };
        Self {
            minimum: minimum.max(0.0001),
            maximum,
            world_cutoff,
        }
    }

    pub(super) fn panning_scale(self) -> SpatialScale {
        // Keep both ears within Rodio's full-volume radius throughout the audible
        // range. The cue applies its own attenuation while Rodio handles panning.
        // A far-away voice is silent, so the audible range determines the scale.
        SpatialScale::new(0.5 / (self.maximum.min(self.world_cutoff) + 4.0))
    }

    fn gain(self, distance: f32) -> f32 {
        if !distance.is_finite() || distance > self.maximum.min(self.world_cutoff) {
            0.0
        } else if distance <= self.minimum + 0.0001 {
            1.0
        } else {
            self.minimum / distance
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct SpatialListenerSnapshot {
    position: Vec3,
    underwater: Option<bool>,
}

impl SpatialListenerSnapshot {
    pub(super) fn gain(
        self,
        policy: SpatialCueDistance,
        position: Vec3,
        water: Option<&PositionedListenerWater>,
    ) -> f32 {
        if water.is_some_and(|water| !water.allows(self.underwater)) {
            return 0.0;
        }
        policy.gain(self.position.distance(position))
    }
}

#[derive(SystemParam)]
pub(super) struct SpatialCueListener<'w, 's> {
    listeners: Query<'w, 's, (Ref<'static, GlobalTransform>, &'static SpatialListener)>,
    emitters: Query<
        'w,
        's,
        (
            &'static GlobalTransform,
            Option<&'static PositionedListenerWater>,
        ),
    >,
    camera_contacts: Query<'w, 's, Ref<'static, TerrainWaterCameraContact>>,
}

impl SpatialCueListener<'_, '_> {
    pub(super) fn snapshot(&self) -> (SpatialListenerSnapshot, bool) {
        let contact = self.camera_contacts.single().ok();
        let listener = self.listeners.iter().next().map(|(pose, _)| pose);
        let changed = contact.as_ref().is_some_and(Ref::is_changed)
            || listener.as_ref().is_some_and(Ref::is_changed);
        let snapshot = SpatialListenerSnapshot {
            position: listener.map_or(Vec3::ZERO, |pose| pose.translation()),
            underwater: contact.and_then(|contact| contact.underwater_status),
        };
        (snapshot, changed)
    }

    pub(super) fn emitter(
        &self,
        emitter: Entity,
    ) -> Option<(Vec3, Option<&PositionedListenerWater>)> {
        self.emitters
            .get(emitter)
            .ok()
            .map(|(pose, water)| (pose.translation(), water))
    }
}
