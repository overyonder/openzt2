use avian3d::prelude::{SpatialQuery, SpatialQueryFilter};
use bevy::{ecs::relationship::RelationshipTarget, prelude::*};
use openzt2_game_assets::world_definitions::{
    world_definition_asset_set_state_and_borrowing_queries::{
        WorldDefinitions, WorldDefinitionsView,
    },
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
};
use openzt2_game_data::AssetId;
use openzt2_world_state::world_spawn::world_membership_types::{
    DefinitionId, WorldMember, WorldMembers,
};
use openzt2_zoo_state::{
    shows::{
        running_show_types::{
            RunningShow, ShowPhase, ShowViewingLocationOf, ShowViewingLocations, StageRunningShow,
        },
        show_stage_types::{ShowStage, ShowStageOpenState},
    },
    topology::topology_graph_types::PathTile,
};

pub(super) fn refresh_stage_viewing_locations(
    runs: Query<&RunningShow>,
    stages: Query<
        (
            Entity,
            &GlobalTransform,
            &WorldMember,
            Option<&ShowViewingLocations>,
            Option<&StageRunningShow>,
            Option<&ShowStageOpenState>,
        ),
        With<ShowStage>,
    >,
    targets: Query<
        (
            Entity,
            &GlobalTransform,
            &WorldMember,
            Option<&DefinitionId>,
            Has<PathTile>,
            Option<&Visibility>,
        ),
        Or<(With<PathTile>, With<DefinitionId>)>,
    >,
    parents: Query<(Option<&ChildOf>, Option<&DefinitionId>)>,
    members: Query<&WorldMembers>,
    definitions: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    spatial: SpatialQuery,
    mut commands: Commands,
) {
    let Some(definitions) = definitions.get(&assets) else {
        return;
    };
    let Some(view) = definitions.guest_viewing() else {
        return;
    };
    let range_squared = view.ground_range_m * view.ground_range_m;
    let seating = ["bench", "viewingarea", "b_grandstand"].map(AssetId::from_key);
    for (stage, stage_pose, world, locations, run, open) in &stages {
        let running = open != Some(&ShowStageOpenState::Closed)
            && run.is_some_and(|run| {
                runs.get(*run.collection())
                    .is_ok_and(|run| run.phase != ShowPhase::BetweenShows)
            });
        if !running && locations.is_none() {
            continue;
        }
        let mut previous: bevy::ecs::entity::EntityHashMap<_> = locations
            .into_iter()
            .flat_map(RelationshipTarget::iter)
            .filter_map(|location| Some((parents.get(location).ok()?.0?.parent(), location)))
            .collect();
        if running {
            let focus = stage_pose.translation() + Vec3::Y * view.guest_eye_height_m;
            let Ok(stage_world) = members.get(world.root) else {
                continue;
            };
            for (target, pose, member, definition, is_path, visibility) in
                targets.iter_many(stage_world.iter()).flatten()
            {
                if member.root != world.root
                    || target == stage
                    || visibility == Some(&Visibility::Hidden)
                    || pose.translation().xz().distance_squared(focus.xz()) > range_squared
                    || !(is_path
                        || definition
                            .and_then(|definition| definitions.find_object(definition.0))
                            .is_some_and(|object| {
                                seating.iter().any(|kind| {
                                    object.id == *kind
                                        || object.behavior_type_identifiers.contains(kind)
                                })
                            }))
                {
                    continue;
                }
                let clear = |ground| {
                    let eye = ground + Vec3::Y * view.guest_eye_height_m;
                    let offset = focus - eye;
                    Dir3::new(offset).map_or(true, |ray| {
                        spatial
                            .cast_ray_predicate(
                                eye,
                                ray,
                                offset.length(),
                                false,
                                &SpatialQueryFilter::DEFAULT,
                                &|hit| blocks_view(hit, target, stage, &parents, definitions),
                            )
                            .is_none()
                    })
                };
                let visible = definition
                    .and_then(|definition| definitions.find_viewing_opportunity(definition.0))
                    .filter(|area| !area.slots.is_empty())
                    .map_or_else(
                        || clear(pose.translation()),
                        |area| {
                            area.slots.iter().any(|slot| {
                                clear(pose.transform_point(Vec3::from_array(slot.local_position_m)))
                            })
                        },
                    );
                if !visible {
                    continue;
                }
                if previous.remove(&target).is_none() {
                    commands.spawn((
                        ChildOf(target),
                        Transform::IDENTITY,
                        *world,
                        ShowViewingLocationOf(stage),
                    ));
                }
            }
        }
        for (_, location) in previous {
            commands.entity(location).try_despawn();
        }
    }
}

fn blocks_view(
    mut entity: Entity,
    target: Entity,
    stage: Entity,
    parents: &Query<(Option<&ChildOf>, Option<&DefinitionId>)>,
    definitions: WorldDefinitionsView<'_>,
) -> bool {
    let see_through = AssetId::from_key("b_seethrough");
    loop {
        if entity == target || entity == stage {
            return false;
        }
        let Ok((parent, definition)) = parents.get(entity) else {
            return true;
        };
        if definition
            .and_then(|definition| definitions.find_object(definition.0))
            .is_some_and(|object| object.behavior_type_identifiers.contains(&see_through))
        {
            return false;
        }
        let Some(parent) = parent else { return true };
        entity = parent.parent();
    }
}
