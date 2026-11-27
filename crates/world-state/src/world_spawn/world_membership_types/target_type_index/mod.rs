use std::collections::{BTreeMap, BTreeSet};

use bevy::{
    ecs::{
        entity::EntityHashSet, entity_disabling::Disabled, query::Allow,
        relationship::RelationshipTarget, system::SystemParam,
    },
    prelude::*,
};
use openzt2_game_assets::world_definitions::{
    world_definition_asset_set_state_and_borrowing_queries::{
        WorldDefinitionSourcesPublished, WorldDefinitions,
    },
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
};
use openzt2_game_data::AssetId;

use super::{DefinitionId, WorldMember, WorldRoot};
use crate::world_spawn::persistent_id_types::PersistentId;

/// Target types supplied by an actor's concrete definition.
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct WorldTargetTypes(pub Box<[AssetId]>);

#[derive(Component)]
struct WorldObjectTargetTypes(Box<[AssetId]>);

type HasTargetTypes = Or<(With<WorldTargetTypes>, With<WorldObjectTargetTypes>)>;

#[derive(Component, Default)]
struct WorldTargetTypeIndex(BTreeMap<AssetId, Entity>);

/// Runs before the guest and staff selection passes.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WorldTargetTypePublication;

pub struct WorldTargetTypeIndexPlugin;

impl Plugin for WorldTargetTypeIndexPlugin {
    fn build(&self, app: &mut App) {
        app.register_required_components::<WorldRoot, WorldTargetTypeIndex>()
            .add_message::<WorldDefinitionSourcesPublished>()
            .add_systems(
                FixedUpdate,
                (
                    project_authored_world_target_types.before(project_world_target_type_index),
                    project_world_target_type_index,
                )
                    .in_set(WorldTargetTypePublication),
            );
    }
}

fn project_authored_world_target_types(
    definitions: Option<Res<WorldDefinitions>>,
    sources: Option<Res<Assets<WorldDefinitionAsset>>>,
    mut published: MessageReader<WorldDefinitionSourcesPublished>,
    mut enabled: RemovedComponents<Disabled>,
    mut removed_definitions: RemovedComponents<DefinitionId>,
    authored: Query<(Entity, &DefinitionId), With<WorldMember>>,
    local_changes: Query<
        Entity,
        (
            With<DefinitionId>,
            With<WorldMember>,
            Or<(Changed<DefinitionId>, Added<WorldMember>)>,
        ),
    >,
    mut targets: Query<(&DefinitionId, Option<&mut WorldObjectTargetTypes>), With<WorldMember>>,
    mut commands: Commands,
) {
    let Some(definitions) = definitions
        .as_deref()
        .zip(sources.as_deref())
        .and_then(|(definitions, sources)| definitions.get(sources))
    else {
        return;
    };
    let changed: BTreeSet<_> = published
        .read()
        .flat_map(|change| change.changed_objects.iter().copied())
        .collect();
    let mut dirty: EntityHashSet = local_changes.iter().chain(enabled.read()).collect();
    if !changed.is_empty() {
        dirty.extend(
            authored
                .iter()
                .filter(|(_, id)| changed.contains(&id.0))
                .map(|(entity, _)| entity),
        );
    }
    for entity in removed_definitions.read() {
        if !authored.contains(entity) {
            commands
                .entity(entity)
                .try_remove::<WorldObjectTargetTypes>();
        }
    }
    for entity in dirty {
        let Ok((definition, types)) = targets.get_mut(entity) else {
            continue;
        };
        let Some(object) = definitions.find_object(definition.0) else {
            if types.is_some() {
                commands
                    .entity(entity)
                    .try_remove::<WorldObjectTargetTypes>();
            }
            continue;
        };
        let authored = object.behavior_type_identifiers.as_slice();
        if let Some(mut types) = types {
            if types.0.as_ref() != authored {
                types.0 = authored.into();
            }
        } else {
            commands
                .entity(entity)
                .insert(WorldObjectTargetTypes(authored.into()));
        }
    }
}

#[derive(Component)]
struct TypedWorldTarget {
    kind: AssetId,
    root: Entity,
}

#[derive(Component)]
#[relationship(relationship_target = IndexedTargetRows)]
struct RowForTarget(Entity);

#[derive(Component)]
#[relationship_target(relationship = RowForTarget, linked_spawn)]
struct IndexedTargetRows(Vec<Entity>);

#[derive(Component)]
#[relationship(relationship_target = BucketRows)]
struct RowInBucket(Entity);

#[derive(Component)]
#[relationship_target(relationship = RowInBucket, linked_spawn)]
struct BucketRows(Vec<Entity>);

#[derive(SystemParam)]
struct TargetTypeIndexUpdates<'w, 's> {
    targets: Query<
        'w,
        's,
        (
            &'static WorldMember,
            Option<&'static WorldTargetTypes>,
            Option<&'static WorldObjectTargetTypes>,
        ),
        HasTargetTypes,
    >,
    changed: Query<
        'w,
        's,
        Entity,
        (
            HasTargetTypes,
            Or<(
                Changed<WorldTargetTypes>,
                Changed<WorldObjectTargetTypes>,
                Changed<WorldMember>,
            )>,
        ),
    >,
    disabled: Query<'w, 's, Entity, (With<IndexedTargetRows>, Added<Disabled>, Allow<Disabled>)>,
    rows_by_target: Query<'w, 's, &'static IndexedTargetRows, Allow<Disabled>>,
    rows: Query<'w, 's, &'static TypedWorldTarget>,
    roots: Query<'w, 's, &'static mut WorldTargetTypeIndex, With<WorldRoot>>,
}

fn project_world_target_type_index(
    mut commands: Commands,
    mut updates: TargetTypeIndexUpdates,
    mut removed_types: RemovedComponents<WorldTargetTypes>,
    mut removed_object_types: RemovedComponents<WorldObjectTargetTypes>,
    mut removed_members: RemovedComponents<WorldMember>,
    mut enabled: RemovedComponents<Disabled>,
) {
    let dirty: EntityHashSet = updates
        .changed
        .iter()
        .chain(updates.disabled.iter())
        .chain(removed_types.read())
        .chain(removed_object_types.read())
        .chain(removed_members.read())
        .chain(enabled.read())
        .collect();
    for target in dirty {
        let next = updates.targets.get(target).ok();
        let next_root = next.map(|(member, ..)| member.root);
        let mut missing: BTreeSet<_> = next
            .into_iter()
            .flat_map(|(_, actor, object)| {
                actor
                    .into_iter()
                    .flat_map(|kinds| kinds.0.iter().copied())
                    .chain(object.into_iter().flat_map(|kinds| kinds.0.iter().copied()))
            })
            .collect();
        if let Ok(rows) = updates.rows_by_target.get(target) {
            for row in rows.iter() {
                let Ok(member) = updates.rows.get(row) else {
                    continue;
                };
                if next_root != Some(member.root) || !missing.remove(&member.kind) {
                    commands.entity(row).try_despawn();
                }
            }
        }
        let Some(root) = next_root else { continue };
        let Ok(mut index) = updates.roots.get_mut(root) else {
            continue;
        };
        for kind in missing {
            let bucket = if let Some(&bucket) = index.0.get(&kind) {
                bucket
            } else {
                let bucket = commands.spawn(WorldMember { root }).id();
                index.0.insert(kind, bucket);
                bucket
            };
            commands.spawn((
                TypedWorldTarget { kind, root },
                RowForTarget(target),
                RowInBucket(bucket),
            ));
        }
    }
}

#[derive(SystemParam)]
pub struct WorldTargetTypeQueries<'w, 's> {
    roots: Query<'w, 's, &'static WorldTargetTypeIndex>,
    buckets: Query<'w, 's, &'static BucketRows>,
    rows: Query<'w, 's, &'static RowForTarget>,
    targets: Query<'w, 's, (&'static WorldMember, Option<&'static PersistentId>), HasTargetTypes>,
}

impl WorldTargetTypeQueries<'_, '_> {
    /// Merge requested buckets, deduplicate multi-type members and use
    /// persistent entity order for equal candidate scores.
    pub fn collect(
        &self,
        root: Entity,
        kinds: impl IntoIterator<Item = AssetId>,
        output: &mut Vec<Entity>,
    ) {
        output.clear();
        let Ok(index) = self.roots.get(root) else {
            return;
        };
        for kind in kinds {
            let Some(bucket) = index.0.get(&kind) else {
                continue;
            };
            if let Ok(rows) = self.buckets.get(*bucket) {
                output.extend(self.rows.iter_many(rows.iter()).flatten().map(|row| row.0));
            }
        }
        output.retain(|&entity| {
            self.targets
                .get(entity)
                .is_ok_and(|(member, _)| member.root == root)
        });
        output.sort_unstable_by_key(|&entity| {
            (
                self.targets
                    .get(entity)
                    .ok()
                    .and_then(|(_, id)| id)
                    .map(|id| id.0)
                    .unwrap_or(u64::MAX),
                entity,
            )
        });
        output.dedup();
    }
}
