use bevy::{
    ecs::{
        entity::EntityLocation,
        relationship::{Relationship, RelationshipTarget},
        system::SystemParam,
    },
    prelude::*,
};

use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_interface_state::camera::camera_runtime_state_types::ZooCamera;
use openzt2_interface_state::immersive_modes::immersive_mode_state_types::{
    ActiveImmersiveMode, ImmersiveCamera, ImmersiveMode, ImmersiveSubject,
};
use openzt2_zoo_state::animal_behavior::interaction_container_membership::{
    InteractionContainerMemberships, InteractionContainerSlotMembers, InteractionContainerSlotOf, InteractionContainerSlots,
};
use openzt2_zoo_state::extinct_animals::fossil_recovery_types::{
    ActivateFossilSiteMarkersRequest, ActiveFossilSonarArtifactTracking,
};

use super::fossil_sonar_scoring::select_highest_scoring_fossil_sonar_candidate;
use crate::fossil_site_types::FossilSiteKind;
use openzt2_world_state::world_spawn::world_membership_types::{WorldMember, WorldRoot};
use openzt2_zoo_state::locomotion::locomotion_types::SpatialGrid;

#[derive(Component)]
pub(super) struct ActiveFossilSonar;

#[derive(Component)]
#[relationship(relationship_target = FossilSonarSites)]
struct FossilSiteOfWorld(Entity);

#[derive(Component)]
#[relationship_target(relationship = FossilSiteOfWorld)]
struct FossilSonarSites(Vec<Entity>);

#[derive(Component)]
#[relationship(relationship_target = FossilSonarCellSites)]
struct FossilSiteOfCell(Entity);

#[derive(Component)]
#[relationship_target(relationship = FossilSiteOfCell)]
struct FossilSonarCellSites(Vec<Entity>);

type FossilSiteProjection = (
    Entity,
    &'static WorldMember,
    &'static GlobalTransform,
    Option<&'static FossilSiteOfWorld>,
    Option<&'static FossilSiteOfCell>,
);

pub(super) fn register_fossil_sonar_projection(app: &mut App) {
    app.add_observer(clear_fossil_sonar_site_membership)
        .add_systems(
            Update,
            project_fossil_sonar_sites
                .after(super::fossil_world_objects::spawn_world_fossil_recovery_sites)
                .before(update_fossil_sonar)
                .in_set(super::GameSet::Intent)
                .run_if(in_state(super::GamePhase::InGame)),
        );
}

fn clear_fossil_sonar_site_membership(removed: On<Remove<FossilSiteKind>>, mut commands: Commands) {
    commands
        .entity(removed.entity)
        .try_remove::<(FossilSiteOfWorld, FossilSiteOfCell)>();
}

fn project_fossil_sonar_sites(
    updates: Query<
        FossilSiteProjection,
        (
            With<FossilSiteKind>,
            Or<(
                Changed<WorldMember>,
                Changed<GlobalTransform>,
                Without<FossilSiteOfWorld>,
            )>,
        ),
    >,
    sites: Query<FossilSiteProjection, With<FossilSiteKind>>,
    grids: Query<&SpatialGrid>,
    changed_grids: Query<(), Changed<SpatialGrid>>,
    mut commands: Commands,
) {
    let all = !changed_grids.is_empty();
    let affected = all
        .then(|| sites.iter())
        .into_iter()
        .flatten()
        .chain((!all).then(|| updates.iter()).into_iter().flatten());
    for (site, member, pose, current_world, current_cell) in affected {
        if current_world.is_none_or(|world| world.0 != member.root) {
            commands.entity(site).insert(FossilSiteOfWorld(member.root));
        }
        let next = grids.get(member.root).ok().and_then(|grid| {
            grid.cell_of(pose.translation().xz())
                .and_then(|cell| grid.cells.get(cell as usize).copied())
        });
        if current_cell.map(|cell| cell.0) != next {
            if let Some(next) = next {
                commands.entity(site).insert(FossilSiteOfCell(next));
            } else {
                commands.entity(site).remove::<FossilSiteOfCell>();
            }
        }
    }
}

pub(super) fn activate_sonar_markers_for_search_session(
    mut commands: Commands,
    mut requests: MessageReader<ActivateFossilSiteMarkersRequest>,
    sessions: Query<(Entity, &ActiveImmersiveMode), Without<ActiveFossilSonar>>,
) {
    if requests.read().count() == 0 {
        return;
    }
    if let Some((session, _)) = sessions
        .iter()
        .find(|(_, mode)| mode.mode == ImmersiveMode::FossilSearch)
    {
        commands.entity(session).insert(ActiveFossilSonar);
    }
}

#[derive(SystemParam)]
pub(super) struct SonarMembershipChanges<'w, 's> {
    containers: Query<'w, 's, Ref<'static, InteractionContainerSlots>>,
    slots: Query<'w, 's, Ref<'static, InteractionContainerSlotMembers>>,
    slot_owners: Query<'w, 's, &'static InteractionContainerSlotOf>,
    removed_containers: RemovedComponents<'w, 's, InteractionContainerSlots>,
    removed_membership: RemovedComponents<'w, 's, InteractionContainerSlotMembers>,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn update_fossil_sonar(
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    mut sessions: Query<(
        Entity,
        Ref<ActiveFossilSonar>,
        Ref<ImmersiveCamera>,
        Option<Ref<ImmersiveSubject>>,
        &mut ActiveFossilSonarArtifactTracking,
    )>,
    cameras: Query<Ref<GlobalTransform>, With<ZooCamera>>,
    root: Single<Entity, With<WorldRoot>>,
    artifacts: Query<
        (
            Entity,
            &GlobalTransform,
            EntityLocation,
            Has<FossilSiteOfCell>,
        ),
        With<FossilSiteKind>,
    >,
    grid: Query<Ref<SpatialGrid>>,
    world_sites: Query<Ref<FossilSonarSites>>,
    cells: Query<&FossilSonarCellSites>,
    changed_artifacts: Query<
        &WorldMember,
        (
            With<FossilSiteKind>,
            Or<(Added<FossilSiteKind>, Changed<GlobalTransform>)>,
        ),
    >,
    mut membership_changes: SonarMembershipChanges,
    mut removed_world_sites: RemovedComponents<FossilSonarSites>,
    mut removed_subjects: RemovedComponents<ImmersiveSubject>,
    memberships: InteractionContainerMemberships,
) {
    let root = *root;
    let removed_subjects = removed_subjects
        .read()
        .collect::<bevy::ecs::entity::EntityHashSet>();
    let removed_membership = membership_changes
        .removed_membership
        .read()
        .filter_map(|slot| {
            membership_changes
                .slot_owners
                .get(slot)
                .ok()
                .map(Relationship::get)
        })
        .chain(membership_changes.removed_containers.read())
        .collect::<bevy::ecs::entity::EntityHashSet>();
    let removed_sites = removed_world_sites.read().any(|entity| entity == root);
    removed_world_sites.clear();
    let world_sites = world_sites.get(root).ok();
    let grid = grid.get(root).ok();
    let search_changed = active_definitions.is_changed()
        || definitions.is_changed()
        || removed_sites
        || world_sites.as_ref().is_some_and(DetectChanges::is_changed)
        || grid.as_ref().is_some_and(DetectChanges::is_changed)
        || changed_artifacts.iter().any(|site| site.root == root);
    let policy = active_definitions
        .get(&definitions)
        .and_then(|definitions| definitions.fossil_placement());
    for (session, active, camera, subject, mut tracking) in &mut sessions {
        let Ok(view) = cameras.get(camera.0) else {
            tracking.set_if_neq(ActiveFossilSonarArtifactTracking::default());
            continue;
        };
        let membership_changed = std::iter::once(session)
            .chain(subject.as_ref().map(|subject| subject.0))
            .any(|owner| {
                removed_membership.contains(&owner)
                    || membership_changes
                        .containers
                        .get(owner)
                        .is_ok_and(|slots_of_owner| {
                            slots_of_owner.is_changed()
                                || membership_changes
                                    .slots
                                    .iter_many(slots_of_owner.iter())
                                    .flatten()
                                    .any(|members| members.is_changed())
                        })
            });
        if !search_changed
            && !removed_subjects.contains(&session)
            && !membership_changed
            && !active.is_added()
            && !camera.is_changed()
            && !view.is_changed()
            && !subject.as_ref().is_some_and(|subject| subject.is_changed())
        {
            continue;
        }
        let retained = memberships
            .members_of_container(session)
            .chain(
                subject
                    .iter()
                    .flat_map(|subject| memberships.members_of_container(subject.0)),
            )
            .filter_map(|member| artifacts.get(member).ok())
            .min_by_key(|(_, _, location, _)| {
                (location.table_id.as_u32(), location.table_row.index())
            })
            .map(|(member, _, _, _)| member);
        let result = policy.and_then(|policy| {
            let radius = policy.maximum_sonar_distance_squared.sqrt();
            let centre = view.translation().xz();
            let candidates = sonar_candidate_sites(
                grid.as_deref(),
                world_sites.as_deref(),
                &cells,
                &artifacts,
                centre,
                radius,
            );
            select_highest_scoring_fossil_sonar_candidate(
                view.translation(),
                view.forward().as_vec3(),
                retained,
                policy.minimum_sonar_distance_squared,
                policy.maximum_sonar_distance_squared,
                policy.minimum_sonar_view_dot,
                candidates
                    .into_iter()
                    .map(|(entity, pose, _)| (entity, pose)),
            )
        });
        tracking.set_if_neq(result.map_or_else(
            ActiveFossilSonarArtifactTracking::default,
            |candidate| ActiveFossilSonarArtifactTracking {
                selected_artifact: Some(candidate.entity),
                distance_squared: candidate.distance_squared,
                sonar_strength: candidate.score,
            },
        ));
    }
}

type SonarSiteCandidates = Vec<(Entity, Vec3, (u32, usize))>;

fn sonar_candidate_sites(
    grid: Option<&SpatialGrid>,
    world_sites: Option<&FossilSonarSites>,
    cells: &Query<&FossilSonarCellSites>,
    artifacts: &Query<
        (
            Entity,
            &GlobalTransform,
            EntityLocation,
            Has<FossilSiteOfCell>,
        ),
        With<FossilSiteKind>,
    >,
    centre: Vec2,
    radius: f32,
) -> SonarSiteCandidates {
    let mut candidates = Vec::new();
    let grid = grid.filter(|grid| grid.width != 0 && grid.height != 0 && radius.is_finite());
    if let Some(grid) = grid {
        let extent = Vec2::splat(radius);
        let min = ((centre - extent - grid.origin) / grid.cell_size_m)
            .floor()
            .as_ivec2()
            .max(IVec2::ZERO);
        let max = ((centre + extent - grid.origin) / grid.cell_size_m)
            .floor()
            .as_ivec2()
            .min(IVec2::new(grid.width as i32 - 1, grid.height as i32 - 1));
        for y in min.y..=max.y {
            for x in min.x..=max.x {
                let index = (y as u32 * grid.width + x as u32) as usize;
                if let Some(sites) = grid.cells.get(index).and_then(|&cell| cells.get(cell).ok()) {
                    candidates.extend(artifacts.iter_many(sites.iter()).flatten().map(
                        |(entity, pose, location, _)| {
                            (
                                entity,
                                pose.translation(),
                                (location.table_id.as_u32(), location.table_row.index()),
                            )
                        },
                    ));
                }
            }
        }
    }
    candidates.extend(
        artifacts
            .iter_many(world_sites.into_iter().flat_map(RelationshipTarget::iter))
            .flatten()
            .filter(|(_, _, _, indexed)| grid.is_none() || !indexed)
            .map(|(entity, pose, location, _)| {
                (
                    entity,
                    pose.translation(),
                    (location.table_id.as_u32(), location.table_row.index()),
                )
            }),
    );
    candidates.sort_unstable_by_key(|(_, _, location)| *location);
    candidates
}
