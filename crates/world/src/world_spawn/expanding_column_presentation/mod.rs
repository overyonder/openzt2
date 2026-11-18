use openzt2_world_state::world_spawn::prefab_presentation_types::PrefabPresentation;
use openzt2_game_assets::scene_prefab::ScenePrefabAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use bevy::prelude::*;
use openzt2_game_data::world_definitions::expanding_columns::ExpandingColumnPresentationDefinition;
use openzt2_game_data::AssetId;

pub fn spawn_expanding_column_presentation(
    commands: &mut Commands,
    definitions: WorldDefinitionsView<'_>,
    parent: Entity,
    transform: Transform,
    height_metres: f32,
    top_rotation: Quat,
    column: &ExpandingColumnPresentationDefinition,
) -> bool {
    let (Some(base), Some(repeating), Some(top)) = (
        definitions.scene(column.base_prefab),
        definitions.scene(column.repeating_prefab),
        definitions.scene(column.top_prefab),
    ) else {
        return false;
    };
    let mut spawn_piece = |height, rotation, prefab| {
        commands
            .spawn((
                transform
                    .mul_transform(Transform::from_xyz(0.0, height, 0.0).with_rotation(rotation)),
                Visibility::Inherited,
                ChildOf(parent),
                PrefabPresentation::new(prefab),
            ))
            .id()
    };
    let base_entity = spawn_piece(0.0, Quat::IDENTITY, base);
    let count = ((height_metres - column.base_height_metres - column.top_height_metres)
        / column.repeating_height_metres)
        .max(0.0)
        .ceil() as usize;
    let excess_height = (column.base_height_metres
        + column.top_height_metres
        + count as f32 * column.repeating_height_metres
        - height_metres)
        .max(0.0);
    let mut pieces = Vec::with_capacity(count);
    for index in 0..count {
        pieces.push(spawn_piece(
            column.base_height_metres + index as f32 * column.repeating_height_metres
                - excess_height,
            Quat::IDENTITY,
            repeating.clone(),
        ));
    }
    let top_entity = spawn_piece(height_metres - column.top_height_metres, top_rotation, top);
    let mut previous_piece = base_entity;
    let mut previous_attachment = column.base_attachment;
    for (index, piece) in pieces.into_iter().enumerate() {
        commands.entity(piece).insert((
            ExpandingColumnPieceFollows(previous_piece),
            PendingExpandingColumnPieceAttachment {
                previous_attachment,
                own_attachment: column.repeating_attachment,
                vertical_offset: if index == 0 { -excess_height } else { 0.0 },
                rotation: Quat::IDENTITY,
                warned: false,
            },
        ));
        previous_piece = piece;
        previous_attachment = column.repeating_next_attachment;
    }
    commands.entity(top_entity).insert((
        ExpandingColumnPieceFollows(previous_piece),
        PendingExpandingColumnPieceAttachment {
            previous_attachment,
            own_attachment: column.top_attachment,
            vertical_offset: if count == 0 { -excess_height } else { 0.0 },
            rotation: top_rotation,
            warned: false,
        },
    ));
    true
}

#[derive(Component)]
#[relationship(relationship_target = ExpandingColumnPieceSuccessor)]
pub(super) struct ExpandingColumnPieceFollows(Entity);

#[derive(Component)]
#[relationship_target(relationship = ExpandingColumnPieceFollows)]
pub(super) struct ExpandingColumnPieceSuccessor(Entity);

#[derive(Component)]
pub(super) struct PendingExpandingColumnPieceAttachment {
    previous_attachment: AssetId,
    own_attachment: AssetId,
    vertical_offset: f32,
    rotation: Quat,
    warned: bool,
}

pub(super) fn align_expanding_column_piece_attachments(
    mut commands: Commands,
    prefabs: Res<Assets<ScenePrefabAsset>>,
    predecessors: Query<(Entity, &ExpandingColumnPieceFollows)>,
    successors: Query<&ExpandingColumnPieceSuccessor>,
    mut pieces: Query<(&mut Transform, &PrefabPresentation)>,
    mut pending: Query<&mut PendingExpandingColumnPieceAttachment>,
) {
    for (first_unaligned, predecessor) in &predecessors {
        if predecessors.contains(predecessor.0) {
            continue;
        }
        let mut previous_piece = predecessor.0;
        let mut entity = first_unaligned;
        while let Ok(mut attachment) = pending.get_mut(entity) {
            let Ok([(previous_transform, previous_presentation), (mut transform, presentation)]) =
                pieces.get_many_mut([previous_piece, entity])
            else {
                break;
            };
            let (Some(previous_prefab), Some(prefab)) = (
                prefabs.get(&previous_presentation.0),
                prefabs.get(&presentation.0),
            ) else {
                break;
            };
            let (Some(previous_attachment), Some(own_attachment)) = (
                previous_prefab.prefab_attachment_transform(attachment.previous_attachment),
                prefab.prefab_attachment_transform(attachment.own_attachment),
            ) else {
                if !attachment.warned {
                    warn!(
                        ?entity,
                        "expanding column references an absent model attachment"
                    );
                    attachment.warned = true;
                }
                break;
            };
            let offset = Transform::from_xyz(0.0, attachment.vertical_offset, 0.0)
                .with_rotation(attachment.rotation);
            *transform = Transform::from_matrix(
                previous_transform.to_matrix()
                    * previous_attachment
                    * offset.to_matrix()
                    * own_attachment.inverse(),
            );
            commands.entity(entity).remove::<(
                ExpandingColumnPieceFollows,
                PendingExpandingColumnPieceAttachment,
            )>();
            let Ok(successor) = successors.get(entity) else {
                break;
            };
            previous_piece = entity;
            entity = successor.0;
        }
    }
}
