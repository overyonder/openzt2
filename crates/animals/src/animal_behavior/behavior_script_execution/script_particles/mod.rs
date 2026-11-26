use std::{io, path::PathBuf};

use bevy::{
    asset::{AssetLoader, LoadContext, io::Reader},
    platform::collections::{HashMap, HashSet},
    prelude::*,
};
use openzt2_game_assets::{
    asset_source::{ArchiveOverlayChanged, AssetArchives},
    effect::{
        ParticleEffectDocumentAsset, SpawnCompleteParticleEffectDocument,
        StopParticleEffectsAttachedToEntity,
    },
    graphics_settings_types::{GraphicsEffectQuality, GraphicsSettings, ThreeLevelGraphicsDetail},
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
    biome: Option<AssetId>,
    effect: Option<String>,
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

    fn extensions(&self) -> &[&str] {
        &["xml"]
    }
}

fn lower_conditions(
    source: &openzt2_game_assets::source_document::ordered_source_document_types::OrderedSourceDocument,
    dictionary: &mut HashMap<AssetId, Vec<ParticleCondition>>,
) {
    for entry in source.root.element_children() {
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
                let biome = condition
                    .attribute("biome")
                    .map(|biome| AssetId::from_key(&biome.to_ascii_lowercase()));
                let effect = condition
                    .element_children()
                    .find_map(|row| row.attribute("xmlFile"))
                    .map(|path| path.replace('\\', "/").to_ascii_lowercase());
                Some(ParticleCondition {
                    graphics,
                    ground,
                    biome,
                    effect,
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
        .add_observer(play_script_particle);
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
    dictionary_handle: Res<ScriptParticleDictionaryHandle>,
    dictionaries: Res<Assets<ScriptParticleDictionary>>,
    asset_server: Res<AssetServer>,
    settings: Option<Res<GraphicsSettings>>,
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
        let ground = world
            .and_then(|world| surface.dictionary_ground(world.root, target_pose.translation()))
            .map(|context| context.ground.dictionary_key());
        let graphics = settings
            .as_ref()
            .map_or(ThreeLevelGraphicsDetail::High, |settings| {
                if settings.effects == GraphicsEffectQuality::Low {
                    ThreeLevelGraphicsDetail::Low
                } else {
                    settings.object_detail
                }
            });
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
                    row.graphics.is_none_or(|required| graphics == required)
                        && row.ground.is_none_or(|required| ground == Some(required))
                        && row.biome.is_none_or(|required| biome == Some(required))
                })
                .max_by_key(|row| {
                    usize::from(row.graphics.is_some())
                        + usize::from(row.ground.is_some())
                        + usize::from(row.biome.is_some())
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
