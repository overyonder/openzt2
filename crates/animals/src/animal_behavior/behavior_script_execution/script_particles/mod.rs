use std::{io, path::PathBuf};

use bevy::{
    asset::{io::Reader, AssetLoader, LoadContext},
    ecs::system::SystemParam,
    platform::collections::{HashMap, HashSet},
    prelude::*,
};
use openzt2_game_assets::{
    asset_source::{ArchiveOverlayChanged, AssetArchives},
    effect::{
        ParticleEffectDocumentAsset, PlayAuthoredParticleDictionaryEffect,
        SpawnCompleteParticleEffectDocument, StopParticleEffectsAttachedToEntity,
    },
    graphics_settings_types::{GraphicsEffectQuality, GraphicsSettings, ThreeLevelGraphicsDetail},
    source_document::{
        animation_text_keys::parse_blue_fang_embedded_animation_text_key,
        blue_fang_source_document_parsing::parse_blue_fang_source_document, path::AssetPath,
    },
};
use openzt2_game_data::{animation::animation_text_key::AuthoredAnimationTextAction, AssetId};
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::animal_behavior::behavior_candidate_selection::BehaviorTargetBiome;
use openzt2_zoo_state::terrain::dry_surface_eligibility::DrySurfaceEligibility;

use crate::animal_behavior::behavior_named_nodes::NamedNodeTransforms;

const PARTICLE_DICTIONARY: &str = "config/particledict.xml";

#[derive(EntityEvent)]
pub(super) struct ScriptParticle {
    pub(super) entity: Entity,
    pub(super) command: String,
}

#[derive(Asset, TypePath)]
struct ScriptParticleDictionary(HashMap<AssetId, Vec<ParticleCondition>>);

struct ParticleCondition {
    graphics: Option<ThreeLevelGraphicsDetail>,
    ground: Option<AssetId>,
    underwater: Option<bool>,
    weights: [i32; 4],
    biome: Option<AssetId>,
    effect: Option<String>,
    removal_time: Option<std::time::Duration>,
}

#[derive(Resource)]
struct ScriptParticleDictionaryHandle(Handle<ScriptParticleDictionary>);

#[derive(TypePath)]
struct ScriptParticleDictionaryLoader(Option<AssetArchives>);

impl FromWorld for ScriptParticleDictionaryLoader {
    fn from_world(world: &mut World) -> Self {
        Self(world.get_resource::<AssetArchives>().cloned())
    }
}

impl AssetLoader for ScriptParticleDictionaryLoader {
    type Asset = ScriptParticleDictionary;
    type Settings = ();
    type Error = io::Error;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _: &(),
        context: &mut LoadContext<'_>,
    ) -> io::Result<ScriptParticleDictionary> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        let source = parse_blue_fang_source_document(AssetPath::new(PARTICLE_DICTIONARY), &bytes)
            .map_err(io::Error::other)?;
        let mut dictionary = HashMap::default();
        lower_conditions(&source, &mut dictionary);
        if let Some(archives) = &self.0 {
            let mut directories = source
                .root
                .element_children()
                .filter(|entry| entry.name.eq_ignore_ascii_case("DictionaryDir"))
                .filter_map(|entry| entry.attribute("directory"))
                .map(|directory| PathBuf::from(directory.replace('\\', "/").to_ascii_lowercase()))
                .collect::<Vec<_>>();
            let mut visited = HashSet::<PathBuf>::default();
            let mut next = 0;
            while let Some(directory) = directories.get(next).cloned() {
                next += 1;
                if !visited.insert(directory.clone()) {
                    continue;
                }
                for path in archives.source_paths_under(&directory) {
                    if path.parent() != Some(directory.as_path())
                        || path
                            .extension()
                            .is_none_or(|ext| !ext.eq_ignore_ascii_case("xml"))
                    {
                        continue;
                    }
                    let Ok(bytes) = archives.read_source(&path) else {
                        continue;
                    };
                    if !bytes
                        .windows(b"BFParticleDictionary".len())
                        .any(|bytes| bytes.eq_ignore_ascii_case(b"BFParticleDictionary"))
                    {
                        continue;
                    }
                    let bytes = context
                        .read_asset_bytes(path.clone())
                        .await
                        .map_err(io::Error::other)?;
                    let document = parse_blue_fang_source_document(
                        AssetPath::new(&path.to_string_lossy()),
                        &bytes,
                    )
                    .map_err(io::Error::other)?;
                    if !document
                        .root
                        .name
                        .eq_ignore_ascii_case("BFParticleDictionary")
                    {
                        continue;
                    }
                    lower_conditions(&document, &mut dictionary);
                    directories.extend(
                        document
                            .root
                            .element_children()
                            .filter(|entry| entry.name.eq_ignore_ascii_case("DictionaryDir"))
                            .filter_map(|entry| entry.attribute("directory"))
                            .map(|directory| {
                                PathBuf::from(directory.replace('\\', "/").to_ascii_lowercase())
                            }),
                    );
                }
            }
        }
        Ok(ScriptParticleDictionary(dictionary))
    }
}

fn lower_conditions(
    source: &openzt2_game_assets::source_document::ordered_source_document_types::OrderedSourceDocument,
    dictionary: &mut HashMap<AssetId, Vec<ParticleCondition>>,
) {
    for entry in source.root.element_children() {
        let weights = [
            "biomeWeight",
            "groundWeight",
            "underwaterWeight",
            "graphicsWeight",
        ]
        .map(|key| {
            entry
                .attribute_named_any(&[key])
                .and_then(|value| value.parse::<i32>().ok())
                .unwrap_or(1)
        });
        let conditions = entry
            .element_children()
            .filter_map(|condition| {
                if !condition.name.eq_ignore_ascii_case("Conditions") {
                    return None;
                }
                let graphics = match condition
                    .attribute("graphics")
                    .map(str::to_ascii_lowercase)
                    .as_deref()
                {
                    None => None,
                    Some("low") => Some(ThreeLevelGraphicsDetail::Low),
                    Some("med" | "medium") => Some(ThreeLevelGraphicsDetail::Medium),
                    Some("high") => Some(ThreeLevelGraphicsDetail::High),
                    Some(_) => return None,
                };
                let ground = condition
                    .attribute("ground")
                    .map(|ground| AssetId::from_key(&ground.to_ascii_lowercase()));
                let underwater = condition.attribute("underwater").and_then(|value| {
                    match value.to_ascii_lowercase().as_str() {
                        "true" | "1" => Some(true),
                        "false" | "0" => Some(false),
                        _ => None,
                    }
                });
                let biome = condition
                    .attribute("biome")
                    .map(|biome| AssetId::from_key(&biome.to_ascii_lowercase()));
                let definition = condition
                    .element_children()
                    .find(|row| row.name.eq_ignore_ascii_case("BFParticleDefinition"));
                let effect = definition
                    .and_then(|row| row.attribute("xmlFile"))
                    .map(|path| path.replace('\\', "/").to_ascii_lowercase());
                let removal_time = definition
                    .and_then(|row| row.attribute("removeTime"))
                    .and_then(|value| value.parse::<f64>().ok())
                    .and_then(|seconds| std::time::Duration::try_from_secs_f64(seconds).ok());
                Some(ParticleCondition {
                    graphics,
                    ground,
                    underwater,
                    weights,
                    biome,
                    effect,
                    removal_time,
                })
            })
            .collect::<Vec<_>>();
        if !conditions.is_empty() {
            dictionary
                .entry(AssetId::from_key(&entry.name.to_ascii_lowercase()))
                .or_default()
                .extend(conditions);
        }
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
        .add_systems(Update, reload_particle_dictionary_after_overlay_change)
        .add_systems(Update, retire_authored_dictionary_effects)
        .add_observer(play_script_particle)
        .add_observer(play_authored_dictionary_effect);
}

fn reload_particle_dictionary_after_overlay_change(
    assets: Res<AssetServer>,
    mut overlays: Option<MessageReader<ArchiveOverlayChanged>>,
) {
    if overlays
        .as_mut()
        .is_some_and(|overlays| overlays.read().count() != 0)
    {
        assets.reload(PARTICLE_DICTIONARY);
    }
}

#[allow(clippy::too_many_arguments)]
fn play_script_particle(
    request: On<ScriptParticle>,
    lookup: ParticleDictionaryLookup,
    named: NamedNodeTransforms,
    transforms: Query<(&GlobalTransform, Option<&WorldMember>)>,
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
        let load = |effect_key: &str| {
            let asset_key = effect_key.replace('\\', "/").to_ascii_lowercase();
            if asset_key.contains('.') {
                return Some(
                    lookup
                        .asset_server
                        .load::<ParticleEffectDocumentAsset>(asset_key),
                );
            }
            let (path, _) = lookup.resolve(
                AssetId::from_key(&asset_key),
                world,
                target_pose.translation(),
            )?;
            Some(
                lookup
                    .asset_server
                    .load::<ParticleEffectDocumentAsset>(path.to_owned()),
            )
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

#[derive(SystemParam)]
struct ParticleDictionaryLookup<'w, 's> {
    dictionary_handle: Res<'w, ScriptParticleDictionaryHandle>,
    dictionaries: Res<'w, Assets<ScriptParticleDictionary>>,
    asset_server: Res<'w, AssetServer>,
    settings: Option<Res<'w, GraphicsSettings>>,
    surface: DrySurfaceEligibility<'w, 's>,
    biomes: BehaviorTargetBiome<'w, 's>,
}

impl ParticleDictionaryLookup<'_, '_> {
    fn resolve(
        &self,
        key: AssetId,
        world: Option<&WorldMember>,
        position: Vec3,
    ) -> Option<(&str, Option<std::time::Duration>)> {
        let rows = self
            .dictionaries
            .get(&self.dictionary_handle.0)?
            .0
            .get(&key)?;
        let context = world.and_then(|world| self.surface.dictionary_ground(world.root, position));
        let ground = context.map(|context| context.ground.dictionary_key());
        let underwater = context.is_some_and(|context| context.underwater);
        let graphics = self
            .settings
            .as_ref()
            .map_or(ThreeLevelGraphicsDetail::High, |settings| {
                if settings.effects == GraphicsEffectQuality::Low {
                    ThreeLevelGraphicsDetail::Low
                } else {
                    settings.object_detail
                }
            });
        let biome = world.and_then(|world| self.biomes.at(world.root, position));
        let (_, _, winner) = rows
            .iter()
            .enumerate()
            .filter_map(|(ordinal, row)| {
                let exact = [
                    row.biome.is_some_and(|required| biome == Some(required)),
                    row.ground.is_some_and(|required| ground == Some(required)),
                    row.underwater == Some(underwater),
                    row.graphics == Some(graphics),
                ];
                let score = row
                    .weights
                    .into_iter()
                    .zip(exact)
                    .filter_map(|(weight, matches)| matches.then_some(i64::from(weight)))
                    .sum::<i64>();
                (score >= 0).then_some((score, std::cmp::Reverse(ordinal), row))
            })
            .max_by_key(|(score, ordinal, _)| (*score, *ordinal))?;
        Some((winner.effect.as_deref()?, winner.removal_time))
    }
}

#[derive(Component)]
struct RemoveAuthoredDictionaryEffect(Timer);

fn play_authored_dictionary_effect(
    request: On<PlayAuthoredParticleDictionaryEffect>,
    lookup: ParticleDictionaryLookup,
    subjects: Query<(&GlobalTransform, Option<&WorldMember>)>,
    mut commands: Commands,
) {
    let Ok((pose, world)) = subjects.get(request.subject) else {
        return;
    };
    let position = if request.attach {
        pose.translation()
    } else {
        request.transform.translation
    };
    let Some((path, lifetime)) = lookup.resolve(request.effect, world, position) else {
        return;
    };
    let entity = openzt2_game_assets::effect::effect_instance_spawning_and_preparation::
        spawn_complete_particle_effect_document(&mut commands, &SpawnCompleteParticleEffectDocument {
            particle_effect_asset: lookup.asset_server.load(path.to_owned()),
            parent_entity: request.attach.then_some(request.subject),
            inherit_parent_rotation: request.inherit_parent_rotation,
            effect_transform: request.transform,
            manual_particle_count: 0,
        });
    if let Some(lifetime) = lifetime {
        commands
            .entity(entity)
            .insert(RemoveAuthoredDictionaryEffect(Timer::new(
                lifetime,
                TimerMode::Once,
            )));
    }
}

fn retire_authored_dictionary_effects(
    time: Res<Time>,
    mut effects: Query<(Entity, &mut RemoveAuthoredDictionaryEffect)>,
    mut commands: Commands,
) {
    for (entity, mut effect) in &mut effects {
        if effect.0.tick(time.delta()).is_finished() {
            commands.entity(entity).try_despawn();
        }
    }
}
