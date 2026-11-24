use bevy::ecs::relationship::RelationshipTarget;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use openzt2_game_assets::world_definitions::{
    world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
};
use openzt2_game_data::{world_definitions::world_objects::WorldObjectAudioStageVolume, AssetId};
use openzt2_world_state::world_spawn::{
    persistent_id_types::PersistentId,
    world_membership_types::{DefinitionId, WorldMember},
};

#[derive(Component)]
#[relationship(relationship_target = RoomStageObjects)]
struct RoomStageObjectOf(Entity);

#[derive(Component)]
#[relationship_target(relationship = RoomStageObjectOf)]
struct RoomStageObjects(Vec<Entity>);

#[derive(Component, PartialEq)]
struct RoomStageVolumes(Box<[WorldObjectAudioStageVolume]>);

pub(super) fn bind_room_stage_volumes(
    mut commands: Commands,
    catalogue: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    mut asset_events: MessageReader<AssetEvent<WorldDefinitionAsset>>,
    changed: Query<Entity, Or<(Changed<DefinitionId>, Changed<WorldMember>)>>,
    mut removed: RemovedComponents<DefinitionId>,
    mut removed_members: RemovedComponents<WorldMember>,
    mut objects: Query<(
        Entity,
        &DefinitionId,
        &WorldMember,
        Option<&RoomStageObjectOf>,
        Option<&mut RoomStageVolumes>,
    )>,
) {
    let reload = catalogue.is_changed() | (asset_events.read().count() != 0);
    let Some(definitions) = catalogue.get(&assets) else {
        return;
    };
    for entity in removed.read().chain(removed_members.read()) {
        if !objects.contains(entity) {
            commands
                .entity(entity)
                .try_remove::<(RoomStageObjectOf, RoomStageVolumes)>();
        }
    }
    let candidates = if reload {
        objects
            .iter()
            .map(|(entity, ..)| entity)
            .collect::<Vec<_>>()
    } else {
        changed.iter().collect()
    };
    let mut objects = objects.iter_many_mut(candidates).matched();
    while let Some((entity, id, member, owner, volumes)) = objects.fetch_next() {
        let authored = definitions
            .find_object(id.0)
            .map(|definition| &definition.audio_stage_volumes[..])
            .filter(|rows| rows.iter().any(|volume| !volume.fringe));
        let Some(authored) = authored else {
            if owner.is_some() {
                commands
                    .entity(entity)
                    .try_remove::<(RoomStageObjectOf, RoomStageVolumes)>();
            }
            continue;
        };
        let next = RoomStageVolumes(authored.to_vec().into_boxed_slice());
        if let Some(mut volumes) = volumes {
            volumes.set_if_neq(next);
        } else {
            commands.entity(entity).try_insert(next);
        }
        if owner.is_none_or(|owner| owner.0 != member.root) {
            commands
                .entity(entity)
                .try_insert(RoomStageObjectOf(member.root));
        }
    }
}

#[derive(SystemParam)]
pub(super) struct ListenerRoomStages<'w, 's> {
    groups: Query<'w, 's, Ref<'static, RoomStageObjects>>,
    sources: Query<
        'w,
        's,
        (
            Entity,
            &'static RoomStageVolumes,
            &'static GlobalTransform,
            Option<&'static PersistentId>,
        ),
    >,
    changed: Query<
        'w,
        's,
        &'static RoomStageObjectOf,
        Or<(Changed<RoomStageVolumes>, Changed<GlobalTransform>)>,
    >,
    removed: RemovedComponents<'w, 's, RoomStageObjects>,
}

impl ListenerRoomStages<'_, '_> {
    pub(super) fn affected_worlds(&mut self) -> bevy::ecs::entity::EntityHashSet {
        self.changed
            .iter()
            .map(|source| source.0)
            .chain(self.removed.read())
            .collect()
    }

    pub(super) fn scope_changed(&self, root: Entity) -> bool {
        self.groups.get(root).is_ok_and(|group| group.is_changed())
    }

    pub(super) fn select(&self, root: Entity, listener: Vec3) -> Option<(Entity, AssetId, bool)> {
        let group = self.groups.get(root).ok()?;
        self.sources
            .iter_many(group.iter())
            .matched()
            .filter_map(|(entity, volumes, transform, id)| {
                let position = room_local_position(transform, listener);
                let volume = volumes
                    .0
                    .iter()
                    .find(|volume| !volume.fringe && contains_room_position(volume, position))?;
                Some((
                    id.map_or(u64::MAX, |id| id.0),
                    entity,
                    volume.stage,
                    volume.occlude_crowd,
                ))
            })
            .min_by_key(|(id, ..)| *id)
            .map(|(_, entity, stage, occlude_crowd)| (entity, stage, occlude_crowd))
    }

    pub(super) fn contains_source(&self, room: Entity, source: Vec3) -> bool {
        self.sources
            .get(room)
            .is_ok_and(|(_, volumes, transform, _)| {
                let position = room_local_position(transform, source);
                volumes
                    .0
                    .iter()
                    .any(|volume| contains_room_position(volume, position))
            })
    }

    pub(super) fn room_changed(&self, room: Entity) -> bool {
        self.changed.contains(room)
    }
}

fn room_local_position(transform: &GlobalTransform, point: Vec3) -> [f32; 3] {
    let local = transform.rotation().inverse() * (point - transform.translation());
    [local.x, -local.z, local.y]
}

fn contains_room_position(volume: &WorldObjectAudioStageVolume, position: [f32; 3]) -> bool {
    let axes = if volume.fringe { 2 } else { 3 };
    (0..axes).all(|axis| {
        position[axis] >= volume.minimum[axis] && position[axis] <= volume.maximum[axis]
    })
}
