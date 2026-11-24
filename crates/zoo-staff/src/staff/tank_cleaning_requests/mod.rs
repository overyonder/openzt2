use bevy::prelude::*;
use openzt2_game_data::{world_definitions::staff_management::StaffJobKind, AssetId};
use openzt2_world_state::game_session_types::WorldSessionMode;
use openzt2_world_state::world_spawn::selected_world_identity::SelectedWorldIdentity;

use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::aquatic::aquatic_simulation_types::{Tank, TankGeometry, WaterQuality};
use openzt2_zoo_state::habitat::habitat_types::HabitatRegion;
use openzt2_zoo_state::terrain::dry_surface_eligibility::DrySurfaceEligibility;
use openzt2_zoo_state::terrain::terrain_navigation_change_types::{TerrainNavigationChanged, TerrainNavigationRestored};
use openzt2_zoo_state::topology::topology_graph_types::TopologyGrid;

use super::{
    staff_job_types::{
        authored_staff_request_urgency, StaffJob, StaffJobRouting, TankCleaningRequestSite,
        TankCleaningSite,
    },
    staff_lifecycle_messages::{CancelStaffJobRequest, StaffJobRequest},
};

/// The water-cleaning manager's rule: a tank at least as dirty as the
/// authored threshold keeps one request site and its keeper request, and a
/// tank cleaner than that withdraws both. The site is the first tank cell from
/// the middle of its cells whose ground lies below the water, at the surface.
#[allow(
    clippy::too_many_arguments,
    reason = "the manager reads each canonical tank and definition owner directly"
)]
pub(crate) fn request_keeper_cleaning_for_dirty_tanks(
    mut commands: Commands,
    mut identifiers: Query<
        &mut openzt2_world_state::world_spawn::persistent_id_types::PersistentIdAllocator,
    >,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    grids: Query<Ref<TopologyGrid>>,
    terrain_chunks: Query<
        &WorldMember,
        With<openzt2_zoo_state::terrain::terrain_chunk_types::TerrainChunk>,
    >,
    (mut terrain_edits, mut terrain_restores): (
        MessageReader<TerrainNavigationChanged>,
        MessageReader<TerrainNavigationRestored>,
    ),
    surfaces: DrySurfaceEligibility,
    tanks: Query<
        (
            Entity,
            Ref<WaterQuality>,
            Ref<TankGeometry>,
            Ref<HabitatRegion>,
            Option<&WorldMember>,
            Option<&TankCleaningSite>,
        ),
        With<Tank>,
    >,
    mut sites: Query<&mut Transform, With<TankCleaningRequestSite>>,
    worlds: Query<Ref<SelectedWorldIdentity>>,
    jobs: Query<(Entity, &StaffJob, Option<&StaffJobRouting>)>,
    mut removed_sites: RemovedComponents<TankCleaningSite>,
    mut requests: MessageWriter<StaffJobRequest>,
    mut cancellations: MessageWriter<CancelStaffJobRequest>,
) {
    let Some(catalogue) = active_definitions.get(&definitions) else {
        return;
    };
    if catalogue.has_pending_assets() {
        return;
    }
    let policy_changed = definitions.is_changed() || active_definitions.is_changed();
    let edited_worlds: bevy::ecs::entity::EntityHashSet = terrain_edits
        .read()
        .map(|edit| edit.chunk)
        .chain(terrain_restores.read().map(|restore| restore.chunk))
        .filter_map(|chunk| terrain_chunks.get(chunk).ok().map(|member| member.root))
        .collect();
    let lost_sites: bevy::ecs::entity::EntityHashSet = removed_sites.read().collect();
    for (tank, quality, geometry, region, world, site) in &tanks {
        let identity = world.and_then(|world| worlds.get(world.root).ok());
        let grid = world.and_then(|world| grids.get(world.root).ok());
        let policy = identity.as_ref().and_then(|identity| {
            let path = match identity.mode {
                WorldSessionMode::Freeform => "config/freeform_mode.xml",
                WorldSessionMode::Challenge => "config/challenge_mode.xml",
                WorldSessionMode::Campaign => "config/campaign_mode.xml",
            };
            catalogue.find_staff_tank_cleaning_request(AssetId::from_virtual_path(path))
        });
        if !policy_changed
            && !quality.is_changed()
            && !geometry.is_changed()
            && !region.is_changed()
            && !lost_sites.contains(&tank)
            && world.is_none_or(|world| !edited_worlds.contains(&world.root))
            && !grid.as_ref().is_some_and(DetectChanges::is_changed)
            && !identity
                .as_ref()
                .is_some_and(|identity| identity.is_changed())
        {
            continue;
        }
        let site = site.and_then(|site| site.iter().next());
        let dirtiness = 1.0 - f32::from(quality.0.min(1000)) / 1000.0;
        let requested = policy.and_then(|policy| {
            let token = policy.request.token?;
            let urgency = authored_staff_request_urgency(policy.request.priority)?;
            if dirtiness < policy.dirt_threshold {
                return None;
            }
            let world = world?;
            let grid = *grid.as_deref()?;
            let point = tank_cleaning_point(world.root, &region, &geometry, grid, &surfaces)?;
            let routing = StaffJobRouting {
                subject_type: policy.request.subject_type,
                recipient: policy.request.staff,
            };
            Some((token, urgency, point, routing))
        });
        if let Some((token, urgency, point, routing)) = requested {
            let Some(world) = world else {
                continue;
            };
            let site = match site {
                Some(site) => {
                    if let Ok(transform) = sites.get_mut(site) {
                        transform
                            .map_unchanged(|transform| &mut transform.translation)
                            .set_if_neq(point);
                    } else {
                        commands
                            .entity(site)
                            .insert(Transform::from_translation(point));
                    }
                    for (job, _, _) in jobs.iter().filter(|(_, job, current_routing)| {
                        job.kind == Some(StaffJobKind::MaintainTank)
                            && job.target == site
                            && (job.token != Some(token)
                                || current_routing.copied().unwrap_or_default() != routing)
                    }) {
                        cancellations.write(CancelStaffJobRequest { job });
                    }
                    site
                }
                None => {
                    let Some(id) = identifiers
                        .get_mut(world.root)
                        .ok()
                        .and_then(|mut allocator| allocator.allocate().ok())
                    else {
                        continue;
                    };
                    commands
                        .spawn((
                            TankCleaningRequestSite { tank },
                            id,
                            *world,
                            Transform::from_translation(point),
                        ))
                        .id()
                }
            };
            requests.write(StaffJobRequest {
                kind: Some(StaffJobKind::MaintainTank),
                target: site,
                urgency,
                token: Some(token),
                routing,
            });
            continue;
        }
        let Some(site) = site else {
            continue;
        };
        for (job, ..) in jobs.iter().filter(|(_, job, _)| {
            job.kind == Some(StaffJobKind::MaintainTank) && job.target == site
        }) {
            cancellations.write(CancelStaffJobRequest { job });
        }
        commands.entity(site).despawn();
    }
}

fn tank_cleaning_point(
    root: Entity,
    region: &HabitatRegion,
    geometry: &TankGeometry,
    grid: TopologyGrid,
    surfaces: &DrySurfaceEligibility,
) -> Option<Vec3> {
    let centre = |cell: &IVec2| {
        grid.cell_translation(cell.extend(0)).xz() + Vec2::splat(grid.spacing_m * 0.5)
    };
    let cells = &region.topology_cells;
    let chosen = cells[cells.len() / 2..]
        .iter()
        .chain(cells[..cells.len() / 2].iter())
        .find(|cell| {
            surfaces
                .ground_height(root, centre(cell))
                .is_some_and(|ground| ground < geometry.water_height)
        })?;
    let point = centre(chosen);
    Some(Vec3::new(point.x, geometry.water_height, point.y))
}
