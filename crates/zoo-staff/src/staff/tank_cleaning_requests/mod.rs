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
use openzt2_zoo_state::topology::topology_graph_types::TopologyGrid;

use super::{
    staff_job_types::{authored_staff_request_urgency, StaffJob, TankCleaningRequestSite},
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
    mut ids: ResMut<openzt2_world_state::world_spawn::persistent_id_types::PersistentIdAllocator>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    grid: Res<TopologyGrid>,
    surfaces: DrySurfaceEligibility,
    tanks: Query<
        (
            Entity,
            Ref<WaterQuality>,
            Ref<TankGeometry>,
            Ref<HabitatRegion>,
            Option<&WorldMember>,
        ),
        With<Tank>,
    >,
    sites: Query<(Entity, &TankCleaningRequestSite)>,
    worlds: Query<Ref<SelectedWorldIdentity>>,
    jobs: Query<(Entity, &StaffJob)>,
    mut requests: MessageWriter<StaffJobRequest>,
    mut cancellations: MessageWriter<CancelStaffJobRequest>,
) {
    for (site, request) in &sites {
        if tanks.get(request.tank).is_err() {
            commands.entity(site).despawn();
        }
    }
    let Some(catalogue) = active_definitions.get(&definitions) else {
        return;
    };
    let policy_changed = definitions.is_changed() || active_definitions.is_changed();
    for (tank, quality, geometry, region, world) in &tanks {
        let identity = world.and_then(|world| worlds.get(world.root).ok());
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
            && !identity
                .as_ref()
                .is_some_and(|identity| identity.is_changed())
        {
            continue;
        }
        let site = sites
            .iter()
            .find(|(_, request)| request.tank == tank)
            .map(|(site, _)| site);
        let dirtiness = 1.0 - f32::from(quality.0.min(1000)) / 1000.0;
        let requested = policy.and_then(|policy| {
            let token = policy.request.token?;
            let urgency = authored_staff_request_urgency(policy.request.priority)?;
            if dirtiness < policy.dirt_threshold {
                return None;
            }
            let point = tank_cleaning_point(&region, &geometry, *grid, &surfaces)?;
            Some((token, urgency, point))
        });
        if let Some((token, urgency, point)) = requested {
            let Some(world) = world else {
                continue;
            };
            let site = match site {
                Some(site) => {
                    commands
                        .entity(site)
                        .insert(Transform::from_translation(point));
                    site
                }
                None => {
                    let Ok(id) = ids.allocate(world.root) else {
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
                kind: StaffJobKind::MaintainTank,
                target: site,
                urgency,
                token: Some(token),
            });
            continue;
        }
        let Some(site) = site else {
            continue;
        };
        for (job, _) in jobs
            .iter()
            .filter(|(_, job)| job.kind == StaffJobKind::MaintainTank && job.target == site)
        {
            cancellations.write(CancelStaffJobRequest { job });
        }
        commands.entity(site).despawn();
    }
}

fn tank_cleaning_point(
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
                .ground_height(centre(cell))
                .is_some_and(|ground| ground < geometry.water_height)
        })?;
    let point = centre(chosen);
    Some(Vec3::new(point.x, geometry.water_height, point.y))
}
