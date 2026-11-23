use std::io;

use bevy::{
    asset::{AssetLoader, LoadContext, io::Reader},
    platform::collections::HashMap,
    prelude::*,
};
use openzt2_game_assets::{
    effect::{
        ParticleEffectDocumentAsset, SpawnCompleteParticleEffectDocument,
        StopParticleEffectsAttachedToEntity,
    },
    source_document::{
        animation_text_keys::parse_blue_fang_embedded_animation_text_key,
        blue_fang_source_document_parsing::parse_blue_fang_source_document, path::AssetPath,
    },
};
use openzt2_game_data::{AssetId, animation::animation_text_key::AuthoredAnimationTextAction};
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::animal_behavior::behavior_candidate_selection::BehaviorTargetBiome;
use openzt2_zoo_state::terrain::dry_surface_eligibility::DrySurfaceEligibility;

use crate::animal_behavior::behavior_named_nodes::NamedNodeTransforms;

const PARTICLE_DICTIONARY: &str = "world/particlesystems/maindict.xml";

#[derive(EntityEvent)]
pub(super) struct ScriptParticle {
    pub(super) entity: Entity,
    pub(super) command: String,
}

#[derive(Asset, TypePath)]
struct ScriptParticleDictionary(HashMap<AssetId, Vec<ParticleCondition>>);

struct ParticleCondition {
    water: Option<bool>,
    biome: Option<AssetId>,
    effect: Option<String>,
}

#[derive(Resource)]
struct ScriptParticleDictionaryHandle(Handle<ScriptParticleDictionary>);

#[derive(Default, TypePath)]
struct ScriptParticleDictionaryLoader;

impl AssetLoader for ScriptParticleDictionaryLoader {
    type Asset = ScriptParticleDictionary;
    type Settings = ();
    type Error = io::Error;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _: &(),
        _: &mut LoadContext<'_>,
    ) -> io::Result<ScriptParticleDictionary> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        let source = parse_blue_fang_source_document(AssetPath::new(PARTICLE_DICTIONARY), &bytes)
            .map_err(io::Error::other)?;
        let mut dictionary = HashMap::default();
        for entry in source.root.element_children() {
            let conditions = entry
                .element_children()
                .filter_map(|condition| {
                    if !condition.name.eq_ignore_ascii_case("Conditions")
                        || condition
                            .attribute("graphics")
                            .is_some_and(|quality| !quality.eq_ignore_ascii_case("high"))
                    {
                        return None;
                    }
                    let water = condition
                        .attribute("ground")
                        .map(|ground| ground.eq_ignore_ascii_case("water"));
                    let biome = condition
                        .attribute("biome")
                        .map(|biome| AssetId::from_key(&biome.to_ascii_lowercase()));
                    let effect = condition
                        .element_children()
                        .find_map(|row| row.attribute("xmlFile"))
                        .map(|path| path.replace('\\', "/").to_ascii_lowercase());
                    Some(ParticleCondition {
                        water,
                        biome,
                        effect,
                    })
                })
                .collect::<Vec<_>>();
            if !conditions.is_empty() {
                dictionary.insert(
                    AssetId::from_key(&entry.name.to_ascii_lowercase()),
                    conditions,
                );
            }
        }
        Ok(ScriptParticleDictionary(dictionary))
    }

    fn extensions(&self) -> &[&str] {
        &["xml"]
    }
}

pub(in crate::animal_behavior) fn register(app: &mut App) {
    app.init_asset::<ScriptParticleDictionary>()
        .init_asset_loader::<ScriptParticleDictionaryLoader>()
        .add_systems(
            Startup,
            |assets: Res<AssetServer>, mut commands: Commands| {
                commands.insert_resource(ScriptParticleDictionaryHandle(
                    assets.load(PARTICLE_DICTIONARY),
                ));
            },
        )
        .add_observer(play_script_particle);
}

#[allow(clippy::too_many_arguments)]
fn play_script_particle(
    request: On<ScriptParticle>,
    dictionary_handle: Res<ScriptParticleDictionaryHandle>,
    dictionaries: Res<Assets<ScriptParticleDictionary>>,
    asset_server: Res<AssetServer>,
    named: NamedNodeTransforms,
    transforms: Query<(&GlobalTransform, Option<&WorldMember>)>,
    surface: DrySurfaceEligibility,
    biomes: BehaviorTargetBiome,
    mut play: MessageWriter<SpawnCompleteParticleEffectDocument>,
    mut stop: MessageWriter<StopParticleEffectsAttachedToEntity>,
) {
    let Some(key) = parse_blue_fang_embedded_animation_text_key(0.0, &request.command) else {
        return;
    };
    let Ok((root_pose, world)) = transforms.get(request.entity) else {
        return;
    };
    for command in key.animation_text_commands {
        let target = command.target_skeleton_joint_asset_key.as_deref();
        let Some((entity, target_pose)) = target
            .map_or(Some((request.entity, root_pose)), |name| {
                named.entity_and_pose(request.entity, name)
            })
        else {
            continue;
        };
        let water = world
            .and_then(|world| surface.on_land_at_position(world.root, target_pose.translation()))
            .map(|land| !land);
        let biome = world.and_then(|world| biomes.at(world.root, target_pose.translation()));
        let load = |effect_key: &str| {
            let asset_key = effect_key.replace('\\', "/").to_ascii_lowercase();
            if asset_key.contains('.') {
                return Some(asset_server.load::<ParticleEffectDocumentAsset>(asset_key));
            }
            dictionaries
                .get(&dictionary_handle.0)?
                .0
                .get(&AssetId::from_key(&asset_key))?
                .iter()
                .filter(|row| {
                    row.water.is_none_or(|required| water == Some(required))
                        && row.biome.is_none_or(|required| biome == Some(required))
                })
                .max_by_key(|row| {
                    usize::from(row.water.is_some()) + usize::from(row.biome.is_some())
                })
                .and_then(|row| row.effect.as_ref())
                .map(|path| asset_server.load::<ParticleEffectDocumentAsset>(path.clone()))
        };
        let (effect, attach, target_rotation, z_rotation, scale) =
            match command.animation_text_action {
                AuthoredAnimationTextAction::RunParticleSystem {
                    particle_system_asset_key,
                    particle_scale,
                    use_target_rotation,
                    use_target_z_axis_rotation,
                } => (
                    particle_system_asset_key,
                    false,
                    use_target_rotation,
                    use_target_z_axis_rotation,
                    particle_scale,
                ),
                AuthoredAnimationTextAction::AttachParticleSystem {
                    particle_system_asset_key,
                    particle_scale,
                    use_target_rotation,
                } => (
                    particle_system_asset_key,
                    true,
                    use_target_rotation,
                    false,
                    particle_scale,
                ),
                AuthoredAnimationTextAction::DetachParticleSystem {
                    particle_system_asset_key,
                } => {
                    if let Some(effect) = load(&particle_system_asset_key) {
                        stop.write(StopParticleEffectsAttachedToEntity {
                            attached_entity: entity,
                            effect_asset: Some(effect.id()),
                        });
                    }
                    continue;
                }
                _ => continue,
            };
        let Some(effect) = load(&effect) else {
            continue;
        };
        let mut pose = if attach {
            Transform::IDENTITY
        } else {
            target_pose.compute_transform()
        };
        pose.scale *= scale.unwrap_or(1.0);
        if !attach && !target_rotation {
            let forward = pose.rotation * Vec3::Z;
            pose.rotation = if z_rotation {
                Quat::from_rotation_y(forward.x.atan2(forward.z))
            } else {
                Quat::IDENTITY
            };
        }
        play.write(SpawnCompleteParticleEffectDocument {
            particle_effect_asset: effect,
            parent_entity: attach.then_some(entity),
            inherit_parent_rotation: target_rotation,
            effect_transform: pose,
            manual_particle_count: 0,
        });
    }
}
