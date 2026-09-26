use openzt2_game_data::{
    particle::{
        legacy_nif_particle_effect::{
            LegacyNifParticleColorKey as ColorKey, LegacyNifParticleEffect as ParticleEffect,
            LegacyNifParticleModifier as Modifier,
        },
        ParticleEffectDocument,
    },
    AssetId,
};

use super::{
    native_model_source_lowering::native_model_particle_effect_labelled_asset_path,
    native_model_source_lowering_types::LoweredNativeModelParticleEffect,
    netimmerse_nif_source::{
        block_payload::NetImmerseNifBlockPayload,
        document_source_types::NetImmerseNifDocument,
        particle_source_types::{
            NetImmerseNiParticleModifier, NetImmerseNiParticleSystemController,
        },
        scene_object_source_types::NetImmerseNiGeometry,
    },
};

const MAX_MODIFIERS: usize = 64;

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("NIF particle source {path}: {kind:?}")]
pub(crate) struct NativeParticleLoweringError {
    path: String,
    kind: NativeParticleLoweringErrorKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum NativeParticleLoweringErrorKind {
    MissingController { particle_block: u32 },
    MissingParticleData { particle_block: u32 },
    ParticleCapacityExceedsU32 { particle_block: u32 },
    ModifierChainIsCyclicOrTooLong,
    MissingModifier { modifier_reference: i32 },
    InvalidModifierBlock { modifier_reference: i32 },
    ModifierBelongsToAnotherController { modifier_reference: i32 },
    MissingParticleColorData { modifier_reference: i32 },
}

impl NativeParticleLoweringError {
    fn new(document: &NetImmerseNifDocument, kind: NativeParticleLoweringErrorKind) -> Self {
        Self {
            path: document.source_path.as_str().to_owned(),
            kind,
        }
    }
}

pub(crate) fn lower_all(
    document: &NetImmerseNifDocument,
    source: &str,
) -> Result<Vec<LoweredNativeModelParticleEffect>, NativeParticleLoweringError> {
    document
        .blocks()
        .filter_map(|block| match &block.payload {
            NetImmerseNifBlockPayload::NiParticles(value) => {
                Some((block.index, &value.geometry, false))
            }
            NetImmerseNifBlockPayload::NiParticleMeshes(value) => {
                Some((block.index, &value.geometry, true))
            }
            _ => None,
        })
        .map(|(block, geometry, mesh_particles)| {
            let effect = lower(document, block, geometry, mesh_particles)?;
            Ok(LoweredNativeModelParticleEffect {
                labelled_asset_path: native_model_particle_effect_labelled_asset_path(
                    source, block,
                ),
                particle_effect_document: ParticleEffectDocument::LegacyNif(effect),
            })
        })
        .collect()
}

fn lower(
    document: &NetImmerseNifDocument,
    block: u32,
    geometry: &NetImmerseNiGeometry,
    mesh_particles: bool,
) -> Result<ParticleEffect, NativeParticleLoweringError> {
    let (controller_ref, controller) =
        find_controller(document, geometry.av_object.object.controller_ref).ok_or_else(|| {
            NativeParticleLoweringError::new(
                document,
                NativeParticleLoweringErrorKind::MissingController {
                    particle_block: block,
                },
            )
        })?;
    let (capacity, particle_radius) =
        particle_data(document, geometry.data_ref).ok_or_else(|| {
            NativeParticleLoweringError::new(
                document,
                NativeParticleLoweringErrorKind::MissingParticleData {
                    particle_block: block,
                },
            )
        })?;
    let capacity = u32::try_from(capacity).map_err(|_| {
        NativeParticleLoweringError::new(
            document,
            NativeParticleLoweringErrorKind::ParticleCapacityExceedsU32 {
                particle_block: block,
            },
        )
    })?;
    let modifiers = modifiers(document, controller_ref, controller.particle_extra_ref)?;
    let identity = AssetId::from_key(&format!(
        "{}.__effect/nif_{block:08x}",
        document.source_path.to_ascii_lowercase()
    ));
    let model = document.source_path.as_str().to_owned();
    Ok(ParticleEffect {
        effect_id: identity,
        emitter_id: identity,
        model,
        source_block: block,
        capacity,
        particle_radius,
        emit_rate: controller.emit_rate,
        emit_start_seconds: controller.emit_start_time,
        emit_stop_seconds: controller.emit_stop_time,
        lifetime_seconds: [
            (controller.lifetime - controller.lifetime_random.abs()).max(0.0),
            controller.lifetime + controller.lifetime_random.abs(),
        ],
        speed: [
            (controller.speed - controller.speed_random.abs()).max(0.0),
            controller.speed + controller.speed_random.abs(),
        ],
        vertical_direction: controller.vertical_direction,
        vertical_angle: controller.vertical_angle,
        horizontal_direction: controller.horizontal_direction,
        horizontal_angle: controller.horizontal_angle,
        initial_normal: controller.initial_normal,
        initial_color: controller.initial_color,
        initial_size: controller.size,
        start_random: controller.start_random,
        emitter_block: controller.emitter_ref,
        mesh_particles,
        modifiers,
    })
}

fn find_controller(
    document: &NetImmerseNifDocument,
    mut reference: i32,
) -> Option<(i32, &NetImmerseNiParticleSystemController)> {
    let mut visited = Vec::new();
    while reference >= 0 && !visited.contains(&reference) {
        visited.push(reference);
        match &document.block(reference)?.payload {
            NetImmerseNifBlockPayload::NiParticleSystemController(value) => {
                return Some((reference, value));
            }
            NetImmerseNifBlockPayload::NiAlphaController(value) => {
                reference = value.controller.next_controller_ref;
            }
            NetImmerseNifBlockPayload::NiKeyframeController(value) => {
                reference = value.controller.next_controller_ref;
            }
            NetImmerseNifBlockPayload::NiMaterialColorController(value) => {
                reference = value.controller.next_controller_ref;
            }
            NetImmerseNifBlockPayload::NiUVController(value) => {
                reference = value.controller.next_controller_ref;
            }
            NetImmerseNifBlockPayload::NiGeomMorpherController(value) => {
                reference = value.controller.next_controller_ref;
            }
            NetImmerseNifBlockPayload::NiVisController(value) => {
                reference = value.controller.next_controller_ref;
            }
            _ => return None,
        }
    }
    None
}

fn particle_data(document: &NetImmerseNifDocument, reference: i32) -> Option<(usize, f32)> {
    document
        .block(reference)
        .and_then(|block| match &block.payload {
            NetImmerseNifBlockPayload::NiParticlesData(value) => Some((
                value.geometry.vertices.as_ref()?.len(),
                value.particle_radius,
            )),
            NetImmerseNifBlockPayload::NiParticleMeshesData(value) => Some((
                value.particles.geometry.vertices.as_ref()?.len(),
                value.particles.particle_radius,
            )),
            _ => None,
        })
}

fn modifiers(
    document: &NetImmerseNifDocument,
    controller_ref: i32,
    mut reference: i32,
) -> Result<Vec<Modifier>, NativeParticleLoweringError> {
    let mut output = Vec::new();
    let mut visited = Vec::new();
    while reference >= 0 {
        if visited.contains(&reference) || visited.len() == MAX_MODIFIERS {
            return Err(NativeParticleLoweringError::new(
                document,
                NativeParticleLoweringErrorKind::ModifierChainIsCyclicOrTooLong,
            ));
        }
        visited.push(reference);
        let block = document.block(reference).ok_or_else(|| {
            NativeParticleLoweringError::new(
                document,
                NativeParticleLoweringErrorKind::MissingModifier {
                    modifier_reference: reference,
                },
            )
        })?;
        let link = modifier_link(&block.payload).ok_or_else(|| {
            NativeParticleLoweringError::new(
                document,
                NativeParticleLoweringErrorKind::InvalidModifierBlock {
                    modifier_reference: reference,
                },
            )
        })?;
        if link.controller_ref != controller_ref {
            return Err(NativeParticleLoweringError::new(
                document,
                NativeParticleLoweringErrorKind::ModifierBelongsToAnotherController {
                    modifier_reference: reference,
                },
            ));
        }
        output.push(match &block.payload {
            NetImmerseNifBlockPayload::NiParticleRotation(value) => Modifier::Rotation {
                random_initial_axis: value.random_initial_axis != 0,
                initial_axis: value.initial_axis,
                radians_per_second: value.rotation_speed,
            },
            NetImmerseNifBlockPayload::NiParticleColorModifier(value) => {
                let data = document
                    .block(value.color_data_ref)
                    .and_then(|block| match &block.payload {
                        NetImmerseNifBlockPayload::NiColorData(data) => Some(&data.data),
                        _ => None,
                    })
                    .ok_or_else(|| {
                        NativeParticleLoweringError::new(
                            document,
                            NativeParticleLoweringErrorKind::MissingParticleColorData {
                                modifier_reference: reference,
                            },
                        )
                    })?;
                Modifier::ColorOverLifetime {
                    interpolation: data.interpolation,
                    keys: data
                        .keys
                        .iter()
                        .map(|key| ColorKey {
                            time: key.time,
                            value: key.value,
                            forward: key.forward,
                            backward: key.backward,
                        })
                        .collect(),
                }
            }
            NetImmerseNifBlockPayload::NiParticleGrowFade(value) => Modifier::GrowFade {
                grow_seconds: value.grow,
                fade_seconds: value.fade,
            },
            NetImmerseNifBlockPayload::NiParticleMeshModifier(value) => Modifier::MeshSelection {
                prototype_blocks: value.particle_mesh_refs.clone(),
            },
            NetImmerseNifBlockPayload::NiGravity(value) if value.field_type == 0 => {
                Modifier::DirectionalGravity {
                    direction: value.direction,
                    acceleration: value.force,
                }
            }
            NetImmerseNifBlockPayload::NiGravity(value) => Modifier::PointGravity {
                position: value.position,
                force: value.force,
                decay: value.decay,
            },
            NetImmerseNifBlockPayload::NiPlanarCollider(value) => Modifier::PlanarCollision {
                bounce: value.collider.bounce,
                spawn_on_collide: value.collider.spawn_on_collide,
                die_on_collide: value.collider.die_on_collide,
                height: value.height,
                width: value.width,
                position: value.position,
                x_axis: value.x_axis,
                y_axis: value.y_axis,
                plane_normal: value.plane.normal,
                plane_constant: value.plane.constant,
            },
            _ => unreachable!("modifier_link accepted an unhandled particle modifier"),
        });
        reference = link.next_modifier_ref;
    }
    Ok(output)
}

fn modifier_link(payload: &NetImmerseNifBlockPayload) -> Option<&NetImmerseNiParticleModifier> {
    match payload {
        NetImmerseNifBlockPayload::NiParticleRotation(value) => Some(&value.modifier),
        NetImmerseNifBlockPayload::NiParticleColorModifier(value) => Some(&value.modifier),
        NetImmerseNifBlockPayload::NiParticleGrowFade(value) => Some(&value.modifier),
        NetImmerseNifBlockPayload::NiParticleMeshModifier(value) => Some(&value.modifier),
        NetImmerseNifBlockPayload::NiGravity(value) => Some(&value.modifier),
        NetImmerseNifBlockPayload::NiPlanarCollider(value) => Some(&value.collider.modifier),
        _ => None,
    }
}
