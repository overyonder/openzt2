use bevy::{
    ecs::{entity::EntityHashSet, entity_disabling::Disabled, query::Allow},
    prelude::*,
};

use openzt2_zoo_state::{
    construction::construction_transaction_types::{EditCommitPhase, EditCommitProgress},
    terrain::{
        terrain_edit_types::TerrainEdit,
        terrain_water_presentation_types::{TerrainWaterRegionRetired, TerrainWaterRegionSource},
    },
};

#[derive(Component)]
#[relationship(relationship_target = WaterRegionEditRetainers)]
pub(in crate::terrain) struct WaterRegionRetainedByEdit(pub(in crate::terrain) Entity);

#[derive(Component)]
#[relationship_target(relationship = WaterRegionRetainedByEdit, linked_spawn)]
pub(in crate::terrain) struct WaterRegionEditRetainers(Vec<Entity>);

pub(in crate::terrain) fn retain_water_owners_for_terrain_edit(
    inserted: On<Insert<TerrainEdit>>,
    mut commands: Commands,
    edits: Query<&TerrainEdit>,
) {
    let Ok(edit) = edits.get(inserted.entity) else {
        return;
    };
    let owners: EntityHashSet = edit
        .water_regions
        .iter()
        .map(|change| change.owner)
        .collect();
    for owner in owners {
        commands.spawn((WaterRegionRetainedByEdit(owner), ChildOf(inserted.entity)));
    }
}

pub(in crate::terrain) fn release_water_owners_from_discarded_terrain_edit(
    discarded: On<Discard<TerrainEdit>>,
    mut commands: Commands,
    children: Query<&Children, Allow<Disabled>>,
    retainers: Query<(), (With<WaterRegionRetainedByEdit>, Allow<Disabled>)>,
) {
    if let Ok(children) = children.get(discarded.entity) {
        for edge in children.iter().filter(|&edge| retainers.contains(edge)) {
            commands.entity(edge).try_despawn();
        }
    }
}

pub(in crate::terrain) fn release_water_owners_from_failed_transactions(
    mut commands: Commands,
    edits: Query<
        (&EditCommitProgress, &Children),
        (With<TerrainEdit>, Changed<EditCommitProgress>),
    >,
    retainers: Query<(), With<WaterRegionRetainedByEdit>>,
) {
    for (progress, children) in &edits {
        if progress.phase == EditCommitPhase::Failed {
            for edge in children.iter().filter(|&edge| retainers.contains(edge)) {
                commands.entity(edge).try_despawn();
            }
        }
    }
}

pub(in crate::terrain) fn reclaim_unretained_retired_water_owners(
    mut commands: Commands,
    newly_retired: Query<Entity, Added<TerrainWaterRegionRetired>>,
    mut last_retainer_removed: RemovedComponents<WaterRegionEditRetainers>,
    retired_owners: Query<
        (),
        (
            With<TerrainWaterRegionRetired>,
            Without<TerrainWaterRegionSource>,
            Without<WaterRegionEditRetainers>,
        ),
    >,
) {
    let affected: EntityHashSet = newly_retired
        .iter()
        .chain(last_retainer_removed.read())
        .collect();
    for owner in affected {
        if retired_owners.contains(owner) {
            commands.entity(owner).try_despawn();
        }
    }
}
