use bevy::{
    platform::collections::{HashMap, HashSet},
    prelude::*,
};

use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_data::AssetId;
use openzt2_game_data::world_definitions::extinct_animal_recovery::{
    FossilPieceDefinition,
};

use super::fossil_assembly::FossilAssemblyPiecePose;
use openzt2_zoo_state::extinct_animals::fossil_collection_and_assembly_types::{
    CollectedFossilPiece, FossilPieceInSlot,
};

#[derive(Resource, Default)]
pub(super) struct FossilDefinitionIndex {
    piece_indices: HashMap<AssetId, usize>,
    pieces: Vec<FossilPieceDefinition>,
    pub(super) sets: Vec<AssetId>,
    pub(super) placeable_rewards: Vec<Vec<FossilRewardCandidate>>,
    pub(super) cash_rewards: Vec<Vec<FossilRewardCandidate>>,
}

#[derive(Clone, Copy)]
pub(super) struct FossilRewardCandidate {
    pub(super) definition: AssetId,
    pub(super) developer_only: bool,
}

impl FossilDefinitionIndex {
    pub(super) fn piece(&self, id: AssetId) -> Option<&FossilPieceDefinition> {
        self.pieces.get(*self.piece_indices.get(&id)?)
    }
}

pub(super) fn index_winning_fossil_definitions(
    active: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    mut events: MessageReader<AssetEvent<WorldDefinitionAsset>>,
    mut index: ResMut<FossilDefinitionIndex>,
) {
    let assets_changed = events.read().any(|event| {
        matches!(
            event,
            AssetEvent::Added { .. } | AssetEvent::Modified { .. } | AssetEvent::Removed { .. }
        )
    });
    events.clear();
    if !index.is_added() && !active.is_changed() && !assets_changed {
        return;
    }
    let Some(definitions) = active.get(&assets) else {
        *index = FossilDefinitionIndex::default();
        return;
    };
    index.pieces = definitions.fossil_pieces().copied().collect();
    let mut seen = HashSet::<AssetId>::default();
    index.sets = index
        .pieces
        .iter()
        .map(|piece| piece.set)
        .filter(|set| seen.insert(*set))
        .collect::<Vec<_>>();
    index.piece_indices = index
        .pieces
        .iter()
        .enumerate()
        .map(|(position, piece)| (piece.id, position))
        .collect();
    index.placeable_rewards.clear();
    index.cash_rewards.clear();
    if let Some(policy) = definitions.fossil_placement() {
        let objects = definitions.objects().collect::<Vec<_>>();
        let parents = objects
            .iter()
            .flat_map(|object| {
                object
                    .behavior_type_identifiers
                    .iter()
                    .copied()
                    .filter(|&kind| kind != object.id)
            })
            .collect::<HashSet<_>>();
        let developer = AssetId::from_key("ztdevcomponent");
        let groups = |types: &[AssetId], needs_placement: bool| {
            types
                .iter()
                .map(|&kind| {
                    objects
                        .iter()
                        .copied()
                        .filter(|object| {
                            (object.id == kind || object.behavior_type_identifiers.contains(&kind))
                                && !parents.contains(&object.id)
                                && (!needs_placement
                                    || definitions.find_placeable(object.id).is_some())
                        })
                        .map(|object| FossilRewardCandidate {
                            definition: object.id,
                            developer_only: object.behavior_type_identifiers.contains(&developer),
                        })
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>()
        };
        index.placeable_rewards = groups(&policy.placeable_objects, true);
        index.cash_rewards = groups(&policy.non_placeable_objects, false);
    }
}

#[derive(Component)]
pub(super) struct FossilPiecePicture(Entity);

pub(super) fn bind_fossil_piece_pictures(
    mut commands: Commands,
    definitions: Res<WorldDefinitions>,
    definitions_assets: Res<Assets<WorldDefinitionAsset>>,
    (mut meshes, mut materials): (ResMut<Assets<Mesh>>, ResMut<Assets<StandardMaterial>>),
    pieces: Query<
        (Entity, &CollectedFossilPiece, Option<&FossilPiecePicture>),
        Or<(With<FossilAssemblyPiecePose>, With<FossilPieceInSlot>)>,
    >,
    changed_pieces: Query<
        Entity,
        Or<(
            Changed<CollectedFossilPiece>,
            Added<FossilAssemblyPiecePose>,
            Added<FossilPieceInSlot>,
        )>,
    >,
    mut ready: MessageReader<AssetEvent<WorldDefinitionAsset>>,
    pictures: Query<&MeshMaterial3d<StandardMaterial>>,
) {
    let definitions_changed = definitions.is_changed()
        || ready.read().any(|event| {
            matches!(
                event,
                AssetEvent::Added { .. }
                    | AssetEvent::Modified { .. }
                    | AssetEvent::LoadedWithDependencies { .. }
            )
        });
    ready.clear();
    let Some(definitions) = definitions.get(&definitions_assets) else {
        return;
    };
    let entities: Vec<_> = if definitions_changed {
        pieces.iter().map(|(entity, ..)| entity).collect()
    } else {
        changed_pieces.iter().collect()
    };
    for (entity, piece, picture) in pieces.iter_many(entities).flatten() {
        let Some(texture) = definitions
            .find_fossil_piece(piece.0)
            .and_then(|piece| definitions.texture_image(piece.model))
        else {
            continue;
        };
        if let Some(picture) = picture {
            if let Ok(material) = pictures.get(picture.0) {
                if let Some(material) = materials.get_mut(&material.0) {
                    if material.base_color_texture.as_ref() != Some(&texture) {
                        material.base_color_texture = Some(texture);
                    }
                }
            }
        } else {
            let material = materials.add(StandardMaterial {
                base_color_texture: Some(texture),
                alpha_mode: AlphaMode::Blend,
                unlit: true,
                cull_mode: None,
                ..default()
            });
            let child = commands
                .spawn((
                    ChildOf(entity),
                    Mesh3d(meshes.add(Rectangle::new(1.0, 1.0))),
                    MeshMaterial3d(material),
                    Transform::IDENTITY,
                ))
                .id();
            commands.entity(entity).insert(FossilPiecePicture(child));
        }
    }
}
