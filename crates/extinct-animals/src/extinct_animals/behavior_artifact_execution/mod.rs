use bevy::prelude::*;
use openzt2_game_assets::world_definitions::{
    world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
};
use openzt2_game_data::AssetId;
use openzt2_world_state::{
    simulation_time::simulation_clock_types::ZooClock,
    world_spawn::{
        prefab_presentation_types::PrefabPresentation,
        world_membership_types::{DefinitionId, WorldMember},
    },
};
use openzt2_zoo_state::{
    behavior_task_execution_types::{
        running_actions::{
            mark_behavior_task_for_failure_and_stop_navigation, RunBehaviorDigArtifact,
            RunBehaviorFindArtifact, RunningBehaviorInstruction, WhenBehaviorAction,
        },
        BehaviorTaskExecutionState,
    },
    locomotion::locomotion_types::{ActiveTerrainDerivedNavigationGraph, NavFlags},
    placement::placement_occupancy_index::PlacedObjectFootprintQueries,
    terrain::dry_surface_eligibility::DrySurfaceEligibility,
    topology::topology_graph_types::{PathTile, TopologyGrid},
};

use crate::fossil_site_types::{DigPit, FossilSiteKind};
use openzt2_game_assets::localization::{
    localization_asset_types::LocalizationAsset,
    localization_precedence_index::LocalizationPrecedenceIndex,
};
use openzt2_game_data::localization::LocalizationFormatArgument;
use openzt2_interface_state::information::zoo_message_types::{ZooMessage, ZooMessages};

#[derive(EntityEvent)]
pub(super) struct DigAuthoredArtifact {
    #[event_target]
    pub site: Entity,
    pub root: Entity,
}

pub(super) fn register(app: &mut App) {
    app.add_observer(find_artifact)
        .add_observer(dig_artifact)
        .add_observer(present_blocked_artifact);
}

#[derive(Event)]
struct FossilMarkFailed {
    root: Entity,
    blocker: Option<AssetId>,
}

fn present_blocked_artifact(
    event: On<FossilMarkFailed>,
    localization: Res<LocalizationPrecedenceIndex>,
    assets: Res<Assets<LocalizationAsset>>,
    time: Res<Time<Real>>,
    mut messages: Query<&mut ZooMessages>,
) {
    let (Some(localization), Ok(mut messages)) = (
        localization.borrow_loaded_localization_view(&assets),
        messages.get_mut(event.root),
    ) else {
        return;
    };
    let key = if event.blocker.is_some() {
        "superstaff:artifact_blocked"
    } else {
        "superstaff:artifact_inaccessible"
    };
    let name = event
        .blocker
        .and_then(|key| localization.find_plain_localized_text(key))
        .unwrap_or_default();
    let mut text = String::new();
    if localization
        .write_localized_text_with_format_arguments(
            AssetId::from_key(key),
            &[LocalizationFormatArgument::Text(name)],
            &mut text,
        )
        .is_ok()
    {
        messages.messages.push(ZooMessage {
            text,
            expires_at_seconds: time.elapsed_secs_f64() + 60.0,
        });
    }
}

fn find_artifact(
    event: On<WhenBehaviorAction<RunBehaviorFindArtifact>>,
    actors: Query<
        (
            &BehaviorTaskExecutionState,
            &RunningBehaviorInstruction,
            &WorldMember,
            &Transform,
        ),
        Without<FossilSiteKind>,
    >,
    worlds: Query<(
        &ZooClock,
        &TopologyGrid,
        &ActiveTerrainDerivedNavigationGraph,
    )>,
    occupied: PlacedObjectFootprintQueries,
    path: Query<(), With<PathTile>>,
    ground: DrySurfaceEligibility,
    definitions: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    blockers: Query<&DefinitionId, Without<FossilSiteKind>>,
    pits: Query<(&WorldMember, &Transform), (With<DigPit>, Without<FossilSiteKind>)>,
    mut sites: Query<
        (
            Entity,
            &WorldMember,
            &mut Transform,
            &mut GlobalTransform,
            &mut DefinitionId,
            &mut PrefabPresentation,
            &mut Visibility,
        ),
        With<FossilSiteKind>,
    >,
    mut commands: Commands,
) {
    let actor = event.entity;
    let Ok((task, instruction, world, origin)) = actors.get(actor) else {
        return;
    };
    if !instruction.is_current(task) {
        return;
    }
    let Ok((clock, grid, graph)) = worlds.get(world.root) else {
        mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
        return;
    };
    let Some(view) = definitions.get(&assets) else {
        mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
        return;
    };
    let mark = (|| {
        let definition = view.find_object(AssetId::from_key("fossilmarker"))?;
        let prefab = view.scene(definition.prefab)?;
        let cell = grid.position_cell(origin.translation).truncate();
        let horizontal = grid.cell_translation(cell.extend(0)).xz();
        let height = ground.ground_height(world.root, horizontal)?;
        let position = Vec3::new(horizontal.x, height, horizontal.y);
        let graph = graph.terrain_derived_navigation_graph();
        let traversable = graph
            .navigation_surface_at_world_position(position)
            .and_then(|index| graph.find_navigation_node(index))
            .is_some_and(|node| {
                node.eligibility_flags.0 & (NavFlags::STAFF.0 | NavFlags::GUEST.0) != 0
            });
        if !traversable {
            return None;
        }
        if let Some(blocker) = occupied
            .entities_occupying_cell(world.root, cell)
            .filter(|object| !path.contains(*object))
            .find_map(|object| {
                blockers
                    .get(object)
                    .ok()
                    .and_then(|object| view.find_object(object.0))
            })
        {
            commands.trigger(FossilMarkFailed {
                root: world.root,
                blocker: Some(blocker.name_key),
            });
            return Some(false);
        }
        let at_cell = |site: &Transform| grid.position_cell(site.translation).truncate() == cell;
        if sites.iter().any(|(_, member, pose, _, id, ..)| {
            member.root == world.root && id.0 == definition.id && at_cell(pose)
        }) {
            return Some(true);
        }
        if pits
            .iter()
            .any(|(member, pose)| member.root == world.root && at_cell(pose))
        {
            return None;
        }
        let site = sites
            .iter()
            .filter(|(_, member, _, _, id, ..)| member.root == world.root && id.0 != definition.id)
            .min_by(|a, b| {
                a.2.translation
                    .xz()
                    .distance_squared(horizontal)
                    .total_cmp(&b.2.translation.xz().distance_squared(horizontal))
                    .then_with(|| a.0.cmp(&b.0))
            })
            .map(|site| site.0)?;
        let (_, _, mut pose, mut global, mut id, presentation, mut visibility) =
            sites.get_mut(site).ok()?;
        let next =
            Transform::from_translation(position).with_scale(Vec3::splat(definition.prefab_scale));
        pose.set_if_neq(next);
        global.set_if_neq(GlobalTransform::from(next));
        id.set_if_neq(DefinitionId(definition.id));
        presentation
            .map_unchanged(|presentation| &mut presentation.0)
            .set_if_neq(prefab.clone());
        visibility.set_if_neq(Visibility::Inherited);
        Some(true)
    })();
    match mark {
        Some(true) => instruction.complete(actor, clock.tick, &mut commands),
        None | Some(false) => {
            if mark.is_none() {
                commands.trigger(FossilMarkFailed {
                    root: world.root,
                    blocker: None,
                });
            }
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
        }
    }
}

fn dig_artifact(
    event: On<WhenBehaviorAction<RunBehaviorDigArtifact>>,
    actors: Query<(
        &BehaviorTaskExecutionState,
        &RunningBehaviorInstruction,
        &WorldMember,
    )>,
    worlds: Query<&ZooClock>,
    targets: Query<&WorldMember, With<FossilSiteKind>>,
    mut commands: Commands,
) {
    let actor = event.entity;
    let Ok((task, step, world)) = actors.get(actor) else {
        return;
    };
    if !step.is_current(task) {
        return;
    }
    let Ok(clock) = worlds.get(world.root) else {
        return;
    };
    if let Some(site) = task.target.filter(|site| {
        targets
            .get(*site)
            .is_ok_and(|target| target.root == world.root)
    }) {
        commands.trigger(DigAuthoredArtifact {
            site,
            root: world.root,
        });
    }
    step.complete(actor, clock.tick, &mut commands);
}
