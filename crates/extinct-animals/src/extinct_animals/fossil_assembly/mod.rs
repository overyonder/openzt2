use bevy::{
    ecs::{
        entity::{EntityHashMap, EntityHashSet},
        relationship::RelationshipTarget,
    },
    platform::collections::HashMap,
    prelude::*,
};

use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_data::AssetId;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::extinct_animals::fossil_collection_and_assembly_types::{
    CollectedFossilPiece, CompletedFossilSetAssembly, FossilAssemblyCompleted, FossilAssemblySlot,
    FossilAssemblySlotOccupant, FossilAssemblySlotOf, FossilAssemblySlots, FossilAssemblyTable,
    FossilPieceInSlot, FossilPlacementSurface, FossilSetAssembly, PlaceFossilPieceInAssemblyRequest,
    FossilPieceOfWorld,
};
use openzt2_zoo_state::progression::unlock_types::ApplyCatalogueDefinitionUnlockRequest;

use super::fossil_definition_index::FossilDefinitionIndex;

/// The piece is being arranged at an assembly table.
#[derive(Component)]
pub struct FossilAssemblyPiecePose;

#[derive(Message)]
pub struct UpdateFossilAssemblyPiecePose {
    pub assembly: Entity,
    pub piece: Entity,
    pub pose: Transform,
}

pub(super) fn update_fossil_assembly_piece_poses(
    mut requests: MessageReader<UpdateFossilAssemblyPiecePose>,
    index: Res<FossilDefinitionIndex>,
    tables: Query<
        (
            &WorldMember,
            &FossilPlacementSurface,
            Option<&FossilSetAssembly>,
        ),
        With<FossilAssemblyTable>,
    >,
    mut pieces: Query<
        (
            &CollectedFossilPiece,
            &FossilPieceOfWorld,
            &mut Transform,
            Has<FossilAssemblyPiecePose>,
        ),
        Without<FossilPieceInSlot>,
    >,
    mut commands: Commands,
) {
    let mut activated = EntityHashSet::default();
    for request in requests.read() {
        let Ok((world, surface, set)) = tables.get(request.assembly) else {
            continue;
        };
        let Ok((collected, owner, mut pose, shown)) = pieces.get_mut(request.piece) else {
            continue;
        };
        let Some(definition) = index.piece(collected.0) else {
            continue;
        };
        if owner.0 != world.root
            || !surface.accepts_fossil_pieces
            || set.is_some_and(|set| set.0 != definition.set)
        {
            continue;
        }
        let Some(next) = released_piece_pose(request.pose, definition.scale_permille) else {
            continue;
        };
        pose.set_if_neq(next);
        if !shown && activated.insert(request.piece) {
            commands
                .entity(request.piece)
                .insert(FossilAssemblyPiecePose);
        }
    }
}

fn released_piece_pose(pose: Transform, scale_permille: u16) -> Option<Transform> {
    if !pose.translation.is_finite()
        || !pose.rotation.is_finite()
        || pose.rotation.length_squared() <= f32::EPSILON
    {
        return None;
    }
    Some(
        Transform::from_translation(pose.translation)
            .with_rotation(pose.rotation.normalize())
            .with_scale(Vec3::splat(f32::from(scale_permille) / 1000.0)),
    )
}

struct FossilTableClaims {
    set: AssetId,
    slots: HashMap<AssetId, (Entity, bool)>,
    occupied_count: usize,
    completed: bool,
}

pub(super) fn place_requested_fossil_pieces(
    mut requests: MessageReader<PlaceFossilPieceInAssemblyRequest>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    index: Res<FossilDefinitionIndex>,
    tables: Query<
        (
            &WorldMember,
            &GlobalTransform,
            &FossilPlacementSurface,
            Option<&FossilSetAssembly>,
            Option<&FossilAssemblySlots>,
            Has<FossilAssemblyCompleted>,
        ),
        With<FossilAssemblyTable>,
    >,
    slots: Query<(&FossilAssemblySlot, Has<FossilAssemblySlotOccupant>)>,
    mut available_pieces: Query<
        (
            &CollectedFossilPiece,
            &FossilPieceOfWorld,
            &mut Transform,
            Has<FossilAssemblyPiecePose>,
        ),
        Without<FossilPieceInSlot>,
    >,
    mut commands: Commands,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    let mut claimed_pieces = EntityHashSet::default();
    let mut arranged_pieces = EntityHashSet::default();
    let mut claims_by_table = EntityHashMap::<FossilTableClaims>::default();
    for request in requests.read() {
        let table = request.assembly_entity;
        if claimed_pieces.contains(&request.piece) {
            continue;
        }
        let Ok((collected, piece_world, mut piece_pose, shown)) =
            available_pieces.get_mut(request.piece)
        else {
            continue;
        };
        let Some(piece) = index.piece(collected.0) else {
            continue;
        };
        let (Some(set), Some(slot)) = (
            definitions.find_fossil_set(piece.set),
            definitions.find_fossil_slot(piece.slot),
        ) else {
            continue;
        };
        if !set.pieces.contains(&piece.id) {
            continue;
        }
        let Ok((member, table_pose, surface, assembly, table_slots, completed)) = tables.get(table)
        else {
            continue;
        };
        if piece_world.0 != member.root
            || !surface.accepts_fossil_pieces
            || assembly.is_some_and(|assembly| assembly.0 != set.id)
        {
            continue;
        }
        let Some(released) = released_piece_pose(request.released_pose, piece.scale_permille)
        else {
            continue;
        };
        if !shown && arranged_pieces.insert(request.piece) {
            commands
                .entity(request.piece)
                .insert(FossilAssemblyPiecePose);
        }
        let mut resting_pose = Transform::from_matrix(
            table_pose.to_matrix() * Mat4::from_cols_array_2d(&slot.transform),
        );
        let scale = f32::from(piece.scale_permille) / 1000.0;
        resting_pose.scale *= scale;
        let tolerance = f32::from(slot.tolerance_cm) / 100.0;
        if released
            .translation
            .distance_squared(resting_pose.translation)
            > tolerance * tolerance
        {
            piece_pose.set_if_neq(released);
            continue;
        }
        let claims = claims_by_table.entry(table).or_insert_with(|| {
            let mut claims = FossilTableClaims {
                set: set.id,
                slots: HashMap::default(),
                occupied_count: 0,
                completed,
            };
            if assembly.is_some() {
                for entity in table_slots.into_iter().flat_map(RelationshipTarget::iter) {
                    if let Ok((slot, occupied)) = slots.get(entity) {
                        claims.occupied_count += usize::from(occupied);
                        claims.slots.entry(slot.0).or_insert((entity, occupied));
                    }
                }
            } else {
                commands.entity(table).insert(FossilSetAssembly(set.id));
                for slot in set
                    .pieces
                    .iter()
                    .filter_map(|id| index.piece(*id).map(|piece| piece.slot))
                {
                    claims.slots.entry(slot).or_insert_with(|| {
                        let entity = commands
                            .spawn((FossilAssemblySlot(slot), FossilAssemblySlotOf(table)))
                            .id();
                        (entity, false)
                    });
                }
            }
            claims
        });
        if claims.set != set.id {
            continue;
        }
        let Some((slot_entity, occupied)) = claims.slots.get_mut(&slot.id) else {
            continue;
        };
        if *occupied {
            continue;
        }
        claimed_pieces.insert(request.piece);
        *occupied = true;
        claims.occupied_count += 1;
        piece_pose.set_if_neq(resting_pose);
        commands
            .entity(request.piece)
            .insert(FossilPieceInSlot(*slot_entity));
        if claims.occupied_count == set.pieces.len() && !claims.completed {
            claims.completed = true;
            commands.entity(table).insert(FossilAssemblyCompleted);
            commands.write_message(CompletedFossilSetAssembly {
                assembly_entity: table,
                fossil_set_identifier: set.id,
            });
        }
    }
}

pub(super) fn clear_assembly_when_table_marker_is_removed(
    removed: On<Remove<FossilAssemblyTable>>,
    mut commands: Commands,
) {
    let table = removed.entity;
    commands.queue(move |world: &mut World| {
        if let Ok(mut table) = world.get_entity_mut(table) {
            table.despawn_related::<FossilAssemblySlots>();
            table.remove::<(FossilSetAssembly, FossilAssemblyCompleted)>();
        }
    });
}

pub(super) fn unlock_catalogue_after_fossil_assembly(
    mut completed: MessageReader<CompletedFossilSetAssembly>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    mut unlocks: MessageWriter<ApplyCatalogueDefinitionUnlockRequest>,
    members: Query<&WorldMember>,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    for completion in completed.read() {
        let Some(set) = definitions.find_fossil_set(completion.fossil_set_identifier) else {
            continue;
        };
        let Ok(member) = members.get(completion.assembly_entity) else {
            continue;
        };
        if set.completion_unlock != AssetId::default() {
            unlocks.write(ApplyCatalogueDefinitionUnlockRequest {
                operation: completion.assembly_entity,
                root: member.root,
                definition: set.completion_unlock,
            });
        }
    }
}
