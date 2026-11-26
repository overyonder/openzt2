use bevy::asset::{LoadContext, VisitAssetDependencies};
use bevy::gltf::{Gltf, GltfMesh};
use bevy::prelude::*;
use bevy_hanabi::EffectAsset;
use openzt2_game_data::{
    particle::{
        authored_particle_system::{
            AuthoredParticleSpawnPolicy, AuthoredParticleSystemEmitter,
            AuthoredParticleSystemModifier, ParticleEffectSimulationClock,
        },
        legacy_nif_particle_effect::LegacyNifParticleModifier,
        ParticleEffectDocument,
    },
    AssetId,
};
use std::collections::BTreeMap;

use crate::material::material_asset_types::MaterialAsset;

use super::{
    authored_particle_system_hanabi_effect_preparation::prepare_authored_particle_system_emitter_as_hanabi_effect_asset,
    effect_document_runtime_queries::{
        collect_root_effect_emitter_ids, create_collision_spawn_initialization,
    },
    effect_runtime_types::ControlledEffectEmissionSettings,
    legacy_nif_hanabi_effect_preparation::prepare_legacy_nif_particle_effect_as_hanabi_effect_asset,
    ParticleEffectDocumentAsset,
};

#[derive(Clone, Debug, VisitAssetDependencies)]
pub(super) struct PreparedParticleEmitter {
    #[dependency]
    pub(super) effect: Handle<EffectAsset>,
    #[dependency]
    pub(super) wave_effect: Option<Handle<EffectAsset>>,
    #[dependency]
    pub(super) collision_children: Vec<PreparedCollisionParticleEmitter>,
    #[dependency]
    pub(super) renderers: Vec<PreparedParticleRenderer>,
    #[dependency]
    pub(super) prototype_mesh: Option<Handle<GltfMesh>>,
    pub(super) emission_settings: ControlledEffectEmissionSettings,
    pub(super) authored_initial_size: Option<f32>,
    pub(super) uses_plane: bool,
    pub(super) shared_spawn_capacity: Option<u32>,
}

#[derive(Clone, Debug, VisitAssetDependencies)]
pub(super) struct PreparedParticleRenderer {
    #[dependency]
    pub(super) effect: Handle<EffectAsset>,
    #[dependency]
    pub(super) material: Handle<MaterialAsset>,
    pub(super) sort_by_distance: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct ParticleEmissionWindow {
    pub(super) start: f32,
    pub(super) stop: f32,
    pub(super) frequency: f32,
    pub(super) phase: f32,
}

impl ParticleEmissionWindow {
    pub(super) fn sample(self, elapsed: f32) -> (bool, bool) {
        let time = self.phase + elapsed * self.frequency;
        let closed = if self.frequency < 0.0 {
            time < self.start
        } else {
            time > self.stop
        };
        ((self.start..=self.stop).contains(&time), closed)
    }

    pub(super) fn active_interval(self, start: f32, end: f32) -> (f32, f32) {
        if self.start > self.stop {
            return (start, start);
        }
        if self.frequency == 0.0 {
            return if self.sample(start).0 {
                (start, end)
            } else {
                (start, start)
            };
        }
        let window_start = (self.start - self.phase) / self.frequency;
        let window_end = (self.stop - self.phase) / self.frequency;
        (
            start.max(window_start.min(window_end)),
            end.min(window_start.max(window_end)),
        )
    }
}

#[derive(Clone, Debug, VisitAssetDependencies)]
pub(super) struct PreparedCollisionParticleEmitter {
    pub(super) emitter: AssetId,
    pub(super) occurrence: usize,
    pub(super) event_capacity: u32,
    #[dependency]
    pub(super) effect: Handle<EffectAsset>,
}

pub(crate) fn prepare_particle_effect_document(
    document: ParticleEffectDocument,
    label_prefix: &str,
    native_model: Option<&Gltf>,
    blend_modes: &BTreeMap<String, bevy_hanabi::AlphaMode>,
    load_context: &mut LoadContext<'_>,
) -> ParticleEffectDocumentAsset {
    let roots = collect_root_effect_emitter_ids(&document);
    let mut prepared_emitters = BTreeMap::new();
    let mut decal_emitter = None;
    match &document {
        ParticleEffectDocument::Effect(effect) => {
            let emitters: BTreeMap<_, _> = effect
                .emitters
                .iter()
                .map(|emitter| (emitter.emitter_id, emitter))
                .collect();
            let mut retiring_emitters = BTreeMap::new();
            for emitter in &effect.emitters {
                let id = emitter.emitter_id;
                let label = format!("{label_prefix}/{}", id.to_lowercase_hexadecimal_string());
                let apply_blend = |effect: EffectAsset, renderer: Option<&openzt2_game_data::particle::authored_particle_system::AuthoredParticleRenderer>| {
                    let mode = renderer.and_then(|renderer| blend_modes.get(&renderer.material).copied());
                    effect.with_alpha_mode(mode.unwrap_or_default())
                };
                let primary = emitter.renderers.first();
                let normal = apply_blend(
                    prepare_authored_particle_system_emitter_as_hanabi_effect_asset(
                        emitter, primary, None, false,
                    ),
                    primary,
                );
                let wave_effect = (!emitter.modifiers.iter().any(|modifier| {
                    matches!(modifier, AuthoredParticleSystemModifier::AngularDrag { .. })
                }))
                .then(|| {
                    load_context.add_labeled_asset(
                        format!("{label}/wave"),
                        apply_blend(
                            prepare_authored_particle_system_emitter_as_hanabi_effect_asset(
                                emitter, primary, None, true,
                            ),
                            primary,
                        ),
                    )
                });
                if decal_emitter.is_none()
                    && matches!(emitter.shape, openzt2_game_data::particle::authored_particle_system::AuthoredParticleEmitterShape::Decal)
                {
                    decal_emitter = Some((id, wave_effect.is_some()));
                }
                let mut occurrences = BTreeMap::<AssetId, usize>::new();
                let collision_children = emitter
                    .modifiers
                    .iter()
                    .filter_map(create_collision_spawn_initialization)
                    .filter_map(|(child, max_count, initialization)| {
                        let target = emitters.get(&child)?;
                        let count = occurrences.entry(child).or_default();
                        let occurrence = *count;
                        *count += 1;
                        let renderer = target.renderers.first();
                        let effect = apply_blend(
                            prepare_authored_particle_system_emitter_as_hanabi_effect_asset(
                                target,
                                renderer,
                                Some(initialization),
                                false,
                            ),
                            renderer,
                        );
                        Some(PreparedCollisionParticleEmitter {
                            emitter: child,
                            occurrence,
                            event_capacity: emitter.capacity.saturating_mul(max_count),
                            effect: load_context.add_labeled_asset(
                                format!(
                                    "{label}/collision/{}/{occurrence}",
                                    child.to_lowercase_hexadecimal_string(),
                                ),
                                effect,
                            ),
                        })
                    })
                    .collect::<Vec<_>>();
                let uses_plane = emitter.modifiers.iter().any(|modifier| {
                    matches!(
                        modifier,
                        AuthoredParticleSystemModifier::KillPlane { .. }
                            | AuthoredParticleSystemModifier::SpawnOnPlane { .. }
                    )
                });
                let effect = load_context.add_labeled_asset(label.clone(), normal);
                let renderers =
                    emitter
                        .renderers
                        .iter()
                        .enumerate()
                        .map(|(ordinal, renderer)| PreparedParticleRenderer {
                            effect: if ordinal == 0 {
                                effect.clone()
                            } else {
                                load_context.add_labeled_asset(
                                format!("{label}/renderer/{ordinal}"),
                                apply_blend(
                                    prepare_authored_particle_system_emitter_as_hanabi_effect_asset(
                                        emitter, Some(renderer), None, false,
                                    ),
                                    Some(renderer),
                                ),
                            )
                            },
                            material: load_context.load(renderer.material.clone()),
                            sort_by_distance: renderer.sort_by_distance,
                        })
                        .collect();
                prepared_emitters.insert(
                    id,
                    PreparedParticleEmitter {
                        effect,
                        wave_effect,
                        collision_children,
                        renderers,
                        prototype_mesh: None,
                        emission_settings: ControlledEffectEmissionSettings {
                            clock: emitter.clock,
                            controller_clock: None,
                            retires_when_empty: emitter_tree_can_retire_when_empty(
                                id,
                                &emitters,
                                &mut retiring_emitters,
                            ),
                            emission_window: emitter.modifiers.iter().find_map(|modifier| {
                                match modifier {
                                    AuthoredParticleSystemModifier::EmitWindow {
                                        start_seconds,
                                        stop_seconds,
                                        frequency,
                                        phase,
                                    } => Some(ParticleEmissionWindow {
                                        start: *start_seconds,
                                        stop: *stop_seconds,
                                        frequency: *frequency,
                                        phase: *phase,
                                    }),
                                    _ => None,
                                }
                            }),
                            birth_rate_curve: emitter.modifiers.iter().find_map(|modifier| {
                                match modifier {
                                    AuthoredParticleSystemModifier::BirthRateCurve {
                                        points,
                                        looped,
                                    } => Some((points.clone().into(), *looped)),
                                    _ => None,
                                }
                            }),
                            manually_driven: matches!(
                                emitter.spawn,
                                AuthoredParticleSpawnPolicy::Manual
                                    | AuthoredParticleSpawnPolicy::Curve { .. },
                            ),
                        },
                        authored_initial_size: Some(
                            emitter
                                .modifiers
                                .iter()
                                .find_map(|modifier| match modifier {
                                    AuthoredParticleSystemModifier::InitialSizeRange {
                                        max,
                                        ..
                                    } => Some(*max),
                                    _ => None,
                                })
                                .unwrap_or_default(),
                        ),
                        uses_plane,
                        shared_spawn_capacity: None,
                    },
                );
            }
        }
        ParticleEffectDocument::LegacyNif(effect) => {
            let policy = effect.spawn_policy;
            let collision_triggers = effect
                .modifiers
                .iter()
                .filter(|modifier| {
                    matches!(
                        modifier,
                        LegacyNifParticleModifier::PlanarCollision {
                            spawn_on_collide: true,
                            ..
                        }
                    )
                })
                .count();
            let spawn_triggers = u32::try_from(collision_triggers)
                .unwrap_or(u32::MAX)
                .saturating_add(u32::from(policy.on_death));
            let last_generation =
                if spawn_triggers != 0 && policy.multiplier != 0 && policy.probability > 0.0 {
                    policy.maximum_generation
                } else {
                    0
                };
            let mut next: Option<(AssetId, Handle<EffectAsset>)> = None;
            let mut variants = BTreeMap::<(bool, bool), Handle<EffectAsset>>::new();
            for generation in (0..=last_generation).rev() {
                let id = if generation == 0 {
                    effect.emitter_id
                } else {
                    AssetId::from_key(&format!(
                        "{}:generation:{generation}",
                        effect.emitter_id.to_lowercase_hexadecimal_string()
                    ))
                };
                let inherits_parent = generation != 0;
                let spawns_children = next.is_some();
                let handle = variants
                    .entry((inherits_parent, spawns_children))
                    .or_insert_with(|| {
                        load_context.add_labeled_asset(
                            format!("{label_prefix}/generation/{generation}"),
                            prepare_legacy_nif_particle_effect_as_hanabi_effect_asset(
                                effect,
                                inherits_parent,
                                spawns_children,
                            ),
                        )
                    })
                    .clone();
                let collision_children = next
                    .take()
                    .map(
                        |(emitter, effect_handle)| PreparedCollisionParticleEmitter {
                            emitter,
                            occurrence: 0,
                            event_capacity: effect
                                .capacity
                                .saturating_mul(spawn_triggers)
                                .saturating_mul(u32::from(policy.multiplier)),
                            effect: effect_handle,
                        },
                    )
                    .into_iter()
                    .collect();
                prepared_emitters.insert(
                    id,
                    PreparedParticleEmitter {
                        effect: handle.clone(),
                        wave_effect: None,
                        collision_children,
                        renderers: Vec::new(),
                        prototype_mesh: native_model
                            .and_then(|model| legacy_nif_prototype(model, effect, generation)),
                        emission_settings: ControlledEffectEmissionSettings {
                            clock: ParticleEffectSimulationClock::Presentation,
                            controller_clock: Some(effect.controller_clock),
                            retires_when_empty: true,
                            emission_window: Some(ParticleEmissionWindow {
                                start: effect.emit_start_seconds,
                                stop: effect.emit_stop_seconds,
                                frequency: 1.0,
                                phase: 0.0,
                            }),
                            birth_rate_curve: None,
                            manually_driven: false,
                        },
                        authored_initial_size: None,
                        uses_plane: false,
                        shared_spawn_capacity: Some(effect.capacity),
                    },
                );
                next = Some((id, handle));
            }
        }
    }
    ParticleEffectDocumentAsset {
        roots,
        decal_emitter,
        prepared_emitters,
    }
}

fn legacy_nif_prototype(
    model: &Gltf,
    effect: &openzt2_game_data::particle::legacy_nif_particle_effect::LegacyNifParticleEffect,
    generation: u16,
) -> Option<Handle<GltfMesh>> {
    let prototypes = effect
        .modifiers
        .iter()
        .find_map(|modifier| match modifier {
            LegacyNifParticleModifier::MeshSelection { prototype_blocks } => Some(prototype_blocks),
            _ => None,
        })?;
    let index = usize::from(generation).min(prototypes.len().checked_sub(1)?);
    let geometry = u32::try_from(prototypes[index]).ok()?;
    let path = std::path::Path::new(&effect.model).with_extension("nif");
    let name = format!("{}#geometry/nif_{geometry:08x}", path.display());
    model.named_meshes.get(name.as_str()).cloned()
}

fn emitter_tree_can_retire_when_empty(
    emitter_id: AssetId,
    emitters: &BTreeMap<AssetId, &AuthoredParticleSystemEmitter>,
    prepared: &mut BTreeMap<AssetId, bool>,
) -> bool {
    use AuthoredParticleSpawnPolicy as Spawn;
    use AuthoredParticleSystemModifier as Modifier;

    if let Some(&retires) = prepared.get(&emitter_id) {
        return retires;
    }
    let Some(emitter) = emitters.get(&emitter_id) else {
        return false;
    };
    let can_stop_emitting = match emitter.spawn {
        Spawn::Once | Spawn::Burst { .. } | Spawn::Manual => true,
        Spawn::Curve { looped: false } => emitter.modifiers.iter().any(|modifier| {
            matches!(modifier, Modifier::BirthRateCurve { points, .. } if !points.is_empty())
        }),
        Spawn::Rate { .. } | Spawn::Curve { looped: true } => emitter.modifiers.iter().any(|modifier| {
            matches!(modifier, Modifier::EmitWindow { frequency, .. } if *frequency != 0.0)
        }),
    };
    let retires = can_stop_emitting
        && emitter.modifiers.iter().all(|modifier| match modifier {
            Modifier::DieWhenEmpty { enabled } => *enabled,
            Modifier::SpawnOnPlane { target_emitter, .. } => {
                emitter_tree_can_retire_when_empty(*target_emitter, emitters, prepared)
            }
            _ => true,
        });
    prepared.insert(emitter_id, retires);
    retires
}
