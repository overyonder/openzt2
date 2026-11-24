use bevy::audio::{SpatialListener, SpatialScale};
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use openzt2_game_data::audio::AudioMixerPolicy;
use openzt2_zoo_state::terrain::terrain_water_view_state::TerrainWaterCameraContact;

use super::audio_cue_playback::PositionedListenerWater;

#[derive(Component, Clone, Copy)]
pub(super) struct SpatialCueDistance {
    sample_range: [f32; 2],
    uses_world_cutoff: bool,
    looped: bool,
    pub(super) retained_in_range: bool,
}

impl SpatialCueDistance {
    pub(super) fn from_authored_range(
        range: Option<[f32; 2]>,
        uses_world_cutoff: bool,
        looped: bool,
    ) -> Self {
        let sample_range = range
            .filter(|[min, max]| min.is_finite() && max.is_finite() && *min >= 0.0 && max >= min)
            .unwrap_or([15.0, 3000.0]);
        Self {
            sample_range,
            uses_world_cutoff,
            looped,
            retained_in_range: false,
        }
    }

    pub(super) fn panning_scale(self, mixer: Option<&AudioMixerPolicy>) -> SpatialScale {
        let maximum = self.sample_range[1].min(self.world_cutoff(mixer));
        SpatialScale::new(0.5 / (maximum + 4.0))
    }

    fn world_cutoff(self, mixer: Option<&AudioMixerPolicy>) -> f32 {
        if self.uses_world_cutoff {
            mixer.map_or(45.0, |policy| policy.max_world_distance)
        } else {
            f32::INFINITY
        }
    }

    fn gain(
        &mut self,
        distance_squared: f32,
        water_eligible: bool,
        mixer: Option<&AudioMixerPolicy>,
    ) -> f32 {
        let [minimum, maximum] = self.sample_range;
        let minimum = minimum.max(0.0001);
        let cutoff = self.world_cutoff(mixer);
        let in_world_range = water_eligible
            && distance_squared.is_finite()
            && distance_squared <= maximum * maximum
            && distance_squared < cutoff * cutoff;
        let admitted = if self.looped {
            let [start, stop] = mixer.map_or([2500.0, 3000.0], |policy| {
                [policy.spatial_in_range, policy.spatial_out_range]
            });
            in_world_range
                && if self.retained_in_range {
                    distance_squared <= stop
                } else {
                    distance_squared < start
                }
        } else {
            in_world_range
        };
        self.retained_in_range = admitted;
        if !admitted {
            0.0
        } else {
            let distance = distance_squared.sqrt();
            if distance <= minimum + 0.0001 {
                1.0
            } else {
                minimum
                    / (minimum
                        + mixer.map_or(1.0, |policy| policy.spatial_rolloff) * (distance - minimum))
            }
        }
    }
}

#[derive(Component, Clone, Copy, Default, PartialEq)]
pub(super) struct SpatialListenerSnapshot {
    position: Vec3,
    forward: Vec3,
    pub(super) camera_position: Option<Vec3>,
    pub(super) underwater: Option<bool>,
}

pub(super) fn publish_spatial_audio_listener(
    mut commands: Commands,
    mut listeners: Query<
        (
            Entity,
            &GlobalTransform,
            Option<&TerrainWaterCameraContact>,
            Option<&mut SpatialListenerSnapshot>,
        ),
        With<SpatialListener>,
    >,
    mut removed: RemovedComponents<SpatialListener>,
) {
    for entity in removed.read().filter(|&entity| !listeners.contains(entity)) {
        commands
            .entity(entity)
            .try_remove::<SpatialListenerSnapshot>();
    }
    let mut listeners = listeners.iter_mut();
    let Some((entity, transform, contact, current)) = listeners.next() else {
        return;
    };
    let camera_position = contact.and_then(|contact| contact.position);
    let snapshot = SpatialListenerSnapshot {
        position: camera_position.unwrap_or_else(|| transform.translation()),
        forward: transform.forward().into(),
        camera_position,
        underwater: contact.and_then(|contact| contact.underwater_status),
    };
    if let Some(mut current) = current {
        current.set_if_neq(snapshot);
    } else {
        commands.entity(entity).try_insert(snapshot);
    }
    for (entity, _, _, current) in listeners {
        if current.is_some() {
            commands
                .entity(entity)
                .try_remove::<SpatialListenerSnapshot>();
        }
    }
}

impl SpatialListenerSnapshot {
    pub(super) fn gain(
        self,
        policy: &mut SpatialCueDistance,
        position: Vec3,
        water: Option<&PositionedListenerWater>,
        mixer: Option<&AudioMixerPolicy>,
    ) -> f32 {
        let offset = position - self.position;
        let gain = policy.gain(
            offset.length_squared(),
            water.is_none_or(|water| water.allows(self.underwater)),
            mixer,
        );
        if offset.dot(self.forward) < 0.0 {
            gain * 0.75
        } else {
            gain
        }
    }
}

#[derive(SystemParam)]
pub(super) struct SpatialCueListener<'w, 's> {
    listener: Query<'w, 's, Ref<'static, SpatialListenerSnapshot>>,
    emitters: Query<
        'w,
        's,
        (
            &'static GlobalTransform,
            Option<&'static PositionedListenerWater>,
        ),
    >,
}

impl SpatialCueListener<'_, '_> {
    pub(super) fn snapshot(&self) -> (SpatialListenerSnapshot, bool) {
        self.listener
            .single()
            .map_or((SpatialListenerSnapshot::default(), false), |listener| {
                (*listener, listener.is_changed())
            })
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
