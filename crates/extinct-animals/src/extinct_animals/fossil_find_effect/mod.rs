use std::{io, time::Duration};

use bevy::{
    asset::{io::Reader, AssetLoader, LoadContext},
    ecs::schedule::common_conditions::on_message,
    platform::collections::HashMap,
    prelude::*,
};
use openzt2_game_assets::{
    effect::{ParticleEffectDocumentAsset, SpawnCompleteParticleEffectDocument},
    source_document::{
        blue_fang_source_document_parsing::parse_blue_fang_source_document,
        blue_fang_source_numeric_lexeme::parse_blue_fang_source_numeric_lexeme, path::AssetPath,
    },
    world_definitions::{
        world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
        world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
    },
};
use openzt2_game_data::AssetId;
use openzt2_world_state::world_spawn::prefab_authored_attachment_identifier::PrefabAuthoredAttachmentIdentifier;

const FOSSIL_EFFECTS: &str = "ui/fx/particles/fossildict.xml";

#[derive(Message)]
pub(super) struct ShowFossilFindEffect {
    pub(super) camera: Entity,
    pub(super) species: AssetId,
}

#[derive(Asset, TypePath)]
struct FossilFindEffects(HashMap<AssetId, FossilEmote>);

struct FossilEmote {
    path: String,
    lifetime: Option<Duration>,
}

#[derive(Component)]
struct FossilEmoteLifetime(Timer);

#[derive(Resource)]
struct FossilEffectsSource(Handle<FossilFindEffects>);

#[derive(Default, TypePath)]
struct FossilEffectsLoader;

impl AssetLoader for FossilEffectsLoader {
    type Asset = FossilFindEffects;
    type Settings = ();
    type Error = io::Error;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _: &(),
        _: &mut LoadContext<'_>,
    ) -> io::Result<FossilFindEffects> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        let source = parse_blue_fang_source_document(AssetPath::new(FOSSIL_EFFECTS), &bytes)
            .map_err(io::Error::other)?;
        let mut effects = HashMap::default();
        for entry in source.root.element_children() {
            if let Some((path, lifetime)) = entry
                .element_children()
                .filter(|child| child.name.eq_ignore_ascii_case("Conditions"))
                .flat_map(|child| child.element_children())
                .find_map(|definition| {
                    Some((
                        definition.attribute("xmlFile")?,
                        definition
                            .attribute("removeTime")
                            .and_then(parse_blue_fang_source_numeric_lexeme::<f32>)
                            .and_then(|seconds| Duration::try_from_secs_f32(seconds).ok()),
                    ))
                })
            {
                effects.insert(
                    AssetId::from_key(&entry.name.to_ascii_lowercase()),
                    FossilEmote {
                        path: path.replace('\\', "/").to_ascii_lowercase(),
                        lifetime,
                    },
                );
            }
        }
        Ok(FossilFindEffects(effects))
    }

    fn extensions(&self) -> &[&str] {
        &["xml"]
    }
}

pub(super) fn register(app: &mut App) {
    app.init_asset::<FossilFindEffects>()
        .init_asset_loader::<FossilEffectsLoader>()
        .add_message::<ShowFossilFindEffect>()
        .add_systems(
            Startup,
            |server: Res<AssetServer>, mut commands: Commands| {
                commands.insert_resource(FossilEffectsSource(server.load(FOSSIL_EFFECTS)));
            },
        )
        .add_systems(
            Update,
            (
                show_fossil_find_effect.run_if(on_message::<ShowFossilFindEffect>),
                retire_fossil_emotes.run_if(any_with_component::<FossilEmoteLifetime>),
            )
                .in_set(super::GameSet::Presentation),
        );
}

fn show_fossil_find_effect(
    mut requests: MessageReader<ShowFossilFindEffect>,
    source: Res<FossilEffectsSource>,
    effects: Res<Assets<FossilFindEffects>>,
    assets: Res<AssetServer>,
    definitions: Res<WorldDefinitions>,
    definition_assets: Res<Assets<WorldDefinitionAsset>>,
    children: Query<&Children>,
    names: Query<(Option<&Name>, Option<&PrefabAuthoredAttachmentIdentifier>)>,
    mut commands: Commands,
) {
    let socket = AssetId::from_key("dock_fossilfinding");
    for request in requests.read() {
        let Some(effect) = definitions
            .get(&definition_assets)
            .and_then(|definitions| definitions.object_record_key(request.species))
            .map(|species| AssetId::from_key(&format!("fossil{}", species.to_ascii_lowercase())))
            .and_then(|id| effects.get(&source.0)?.0.get(&id))
        else {
            continue;
        };
        let Some(parent) = children
            .iter_descendants_depth_first::<Children>(request.camera)
            .find(|&node| {
                names.get(node).is_ok_and(|(name, identifier)| {
                    identifier.is_some_and(|identifier| identifier.0 == socket)
                        || name.is_some_and(|name| {
                            name.as_str().eq_ignore_ascii_case("Dock_FossilFinding")
                        })
                })
            })
        else {
            continue;
        };
        let parent = if let Some(lifetime) = effect.lifetime {
            commands
                .spawn((
                    ChildOf(parent),
                    Transform::IDENTITY,
                    Visibility::Inherited,
                    FossilEmoteLifetime(Timer::new(lifetime, TimerMode::Once)),
                ))
                .id()
        } else {
            parent
        };
        commands.write_message(SpawnCompleteParticleEffectDocument {
            particle_effect_asset: assets.load::<ParticleEffectDocumentAsset>(effect.path.clone()),
            parent_entity: Some(parent),
            inherit_parent_rotation: true,
            effect_transform: Transform::IDENTITY,
            manual_particle_count: 0,
        });
    }
}

fn retire_fossil_emotes(
    time: Res<Time>,
    mut emotes: Query<(Entity, &mut FossilEmoteLifetime)>,
    mut commands: Commands,
) {
    for (entity, mut lifetime) in &mut emotes {
        if lifetime.0.tick(time.delta()).is_finished() {
            commands.entity(entity).try_despawn();
        }
    }
}
