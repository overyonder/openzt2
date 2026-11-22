use bevy::prelude::*;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_data::world_definitions::animation_binder_rig::DecalTextureCycle;
use openzt2_world_state::simulation_time::native_visual_random_stream::NativeVisualRandomStream;

use super::authored_binder_rig_binding::BoundAnimationBinderRig;

#[derive(Component)]
pub(super) struct DecalCyclePlayback(Box<[DecalCycleCursor]>);

struct DecalCycleCursor {
    images: Box<[Handle<Image>]>,
    enabled: bool,
    current: Option<usize>,
    stop_at: Option<usize>,
    resume: bool,
    exhausted: bool,
    wait_seconds: f32,
}

impl DecalCyclePlayback {
    pub(super) fn new(
        cycles: &[DecalTextureCycle],
        mut load_image: impl FnMut(&str) -> Handle<Image>,
    ) -> Self {
        Self(
            cycles
                .iter()
                .map(|cycle| DecalCycleCursor {
                    images: cycle
                        .steps
                        .iter()
                        .map(|step| load_image(&step.image))
                        .collect(),
                    enabled: cycle.enabled,
                    current: None,
                    stop_at: cycle.stop_at,
                    resume: !cycle.frozen,
                    exhausted: false,
                    wait_seconds: 0.0,
                })
                .collect::<Box<[_]>>(),
        )
    }

    pub(super) fn set_enabled_from_key(&mut self, enabled: bool) {
        if let Some(cursor) = self.0.first_mut() {
            cursor.enabled = enabled;
            if enabled {
                cursor.resume = true;
                cursor.stop_at = None;
                cursor.wait_seconds = 0.0;
            }
        }
    }

    pub(super) fn set_stop_from_key(&mut self, step: i32) {
        if let Some(cursor) = self.0.first_mut() {
            cursor.stop_at = usize::try_from(step).ok();
            cursor.resume = step != -2;
            cursor.wait_seconds = 0.0;
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AnimationDecalFrame {
    pub(crate) material: Box<str>,
    pub(crate) layer: Option<usize>,
    pub(crate) texture_coordinates: Option<u32>,
    pub(crate) image: Handle<Image>,
}

#[derive(Component, PartialEq, Eq)]
pub(crate) struct AnimationDecalFrames(pub(crate) Vec<AnimationDecalFrame>);

pub(super) fn advance_authored_decal_texture_cycles(
    mut commands: Commands,
    time: Res<Time<Fixed>>,
    mut random: ResMut<NativeVisualRandomStream>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    mut controllers: Query<(
        Entity,
        &BoundAnimationBinderRig,
        &mut DecalCyclePlayback,
        Option<&mut AnimationDecalFrames>,
    )>,
) {
    for (entity, bound, mut playback, frames) in &mut controllers {
        let Some(rig) = bound.source.and_then(|source| source.get(&assets)) else {
            continue;
        };
        let mut changed = playback.is_added();
        for (cursor, cycle) in playback.0.iter_mut().zip(&rig.decal_cycles) {
            if !cursor.resume || cursor.exhausted {
                continue;
            }
            cursor.wait_seconds -= time.delta_secs();
            if cursor.wait_seconds > 0.0 {
                continue;
            }
            let next = cursor.current.map_or(0, |step| step + 1);
            let index = if next < cycle.steps.len() {
                next
            } else if cursor.enabled {
                0
            } else {
                cursor.exhausted = true;
                continue;
            };
            let Some(step) = cycle.steps.get(index) else {
                continue;
            };
            changed |= cursor.current != Some(index);
            cursor.current = Some(index);
            cursor.wait_seconds = step.hold_seconds;
            if step.variance_seconds > 0.0 {
                cursor.wait_seconds += (random.unit_f32() - 0.5) * step.variance_seconds;
            }
            if cursor.stop_at == Some(index) {
                cursor.resume = false;
            }
        }
        if !changed {
            continue;
        }
        let projected = AnimationDecalFrames(
            playback
                .0
                .iter()
                .zip(&rig.decal_cycles)
                .filter_map(|(cursor, cycle)| {
                    let image = cursor.images.get(cursor.current?)?.clone();
                    Some(AnimationDecalFrame {
                        material: cycle.material.clone(),
                        layer: cycle.layer,
                        texture_coordinates: cycle.texture_coordinates,
                        image,
                    })
                })
                .collect::<Vec<_>>(),
        );
        if let Some(mut frames) = frames {
            frames.set_if_neq(projected);
        } else {
            commands.entity(entity).try_insert(projected);
        }
    }
}
