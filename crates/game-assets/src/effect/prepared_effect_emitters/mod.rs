use bevy::asset::{LoadContext, VisitAssetDependencies};
use bevy::gltf::{Gltf, GltfMesh};
use bevy::prelude::*;
use bevy_hanabi::EffectAsset;
use openzt2_game_data::{
    particle::{
        authored_particle_system::{
            AuthoredParticleCurvePoint, AuthoredParticleSpawnPolicy, AuthoredParticleSystemEmitter,
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
    pub(super) clock: ParticleEffectSimulationClock,
    pub(super) retires_when_empty: bool,
    pub(super) emission_window: Option<ParticleEmissionWindow>,
    pub(super) birth_rate_curve: Option<ParticleBirthRateCurve>,
    pub(super) manually_driven: bool,
    pub(super) uses_plane: bool,
}

#[derive(Clone, Debug, VisitAssetDependencies)]
pub(super) struct PreparedParticleRenderer {
    #[dependency]
    pub(super) effect: Handle<EffectAsset>,
    #[dependency]
    pub(super) material: Handle<MaterialAsset>,
    pub(super) sort_by_distance: bool,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct ParticleEmissionWindow {
    pub(super) start: f32,
    pub(super) stop: f32,
    pub(super) frequency: f32,
    pub(super) phase: f32,
}

impl ParticleEmissionWindow {
    pub(super) fn contains(self, elapsed: f32) -> bool {
        let time = self.phase + elapsed * self.frequency;
        (self.start..=self.stop).contains(&time)
    }

    pub(super) fn has_ended(self, elapsed: f32) -> bool {
        let time = self.phase + elapsed * self.frequency;
        if self.frequency < 0.0 {
            time < self.start
        } else {
            time > self.stop
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct ParticleBirthRateCurve {
    emitter_index: usize,
    modifier_index: usize,
}

impl ParticleBirthRateCurve {
    pub(super) fn points(
        self,
        document: &ParticleEffectDocument,
    ) -> Option<(&[AuthoredParticleCurvePoint], bool)> {
        let ParticleEffectDocument::Effect(effect) = document else {
            return None;
        };
        match effect
            .emitters
            .get(self.emitter_index)?
            .modifiers
            .get(self.modifier_index)?
        {
            AuthoredParticleSystemModifier::BirthRateCurve { points, looped } => {
                Some((points, *looped))
            }
            _ => None,
        }
    }
}

#[derive(Clone, Debug, VisitAssetDependencies)]
pub(super) struct PreparedCollisionParticleEmitter {
    pub(super) emitter: AssetId,
    pub(super) occurrence: usize,
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
            for (emitter_index, emitter) in effect.emitters.iter().enumerate() {
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
                    .filter_map(|(child, initialization)| {
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
                        clock: emitter.clock,
                        retires_when_empty: emitter_tree_can_retire_when_empty(
                            id,
                            &emitters,
                            &mut retiring_emitters,
                        ),
                        emission_window: emitter.modifiers.iter().find_map(
                            |modifier| match modifier {
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
                            },
                        ),
                        birth_rate_curve: emitter
                            .modifiers
                            .iter()
                            .position(|modifier| {
                                matches!(
                                    modifier,
                                    AuthoredParticleSystemModifier::BirthRateCurve { .. }
                                )
                            })
                            .map(|modifier_index| ParticleBirthRateCurve {
                                emitter_index,
                                modifier_index,
                            }),
                        manually_driven: matches!(
                            emitter.spawn,
                            AuthoredParticleSpawnPolicy::Manual
                                | AuthoredParticleSpawnPolicy::Curve { .. },
                        ),
                        uses_plane,
                    },
                );
            }
        }
        ParticleEffectDocument::LegacyNif(effect) => {
            let prototype_mesh = native_model.and_then(|model| {
                let geometry = effect
                    .modifiers
                    .iter()
                    .find_map(|modifier| match modifier {
                        LegacyNifParticleModifier::MeshSelection { prototype_blocks } => {
                            prototype_blocks.iter().copied().find(|&block| block >= 0)
                        }
                        _ => None,
                    })?;
                let path = std::path::Path::new(&effect.model).with_extension("nif");
                let name = format!("{}#geometry/nif_{geometry:08x}", path.display());
                model.named_meshes.get(name.as_str()).cloned()
            });
            prepared_emitters.insert(
                effect.emitter_id,
                PreparedParticleEmitter {
                    effect: load_context.add_labeled_asset(
                        format!("{label_prefix}/emitter"),
                        prepare_legacy_nif_particle_effect_as_hanabi_effect_asset(effect),
                    ),
                    wave_effect: None,
                    collision_children: Vec::new(),
                    renderers: Vec::new(),
                    prototype_mesh,
                    clock: ParticleEffectSimulationClock::Presentation,
                    retires_when_empty: true,
                    emission_window: Some(ParticleEmissionWindow {
                        start: effect.emit_start_seconds,
                        stop: effect.emit_stop_seconds,
                        frequency: 1.0,
                        phase: 0.0,
                    }),
                    birth_rate_curve: None,
                    manually_driven: false,
                    uses_plane: false,
                },
            );
        }
    }
    ParticleEffectDocumentAsset {
        particle_document: document,
        roots,
        decal_emitter,
        prepared_emitters,
    }
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
