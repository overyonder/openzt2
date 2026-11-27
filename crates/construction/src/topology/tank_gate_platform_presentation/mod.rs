use avian3d::prelude::RigidBody;
use bevy::prelude::*;
use openzt2_game_assets::{
    scene_prefab::ScenePrefabAsset,
    world_definitions::{
        world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
        world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
    },
};
use openzt2_game_data::AssetId;
use openzt2_world::world_spawn::prefab_world_instance_spawning::spawn_loaded_scene_prefab_as_world_instance;
use openzt2_world_state::world_spawn::{
    persistent_id_types::PersistentIdAllocator,
    world_membership_types::{DefinitionId, WorldMember},
};
use openzt2_zoo_state::{
    aquatic::{
        aquatic_simulation_types::{Tank, TankGeometry, TankWallTopHeight},
        tank_wall_ground_sampling::TankWallGroundSampling,
    },
    habitat::{
        habitat_membership_and_containment::locate_habitat_at_world_position,
        habitat_types::{HabitatCellOf, HabitatIndex},
    },
    topology::{
        gate_operation_types::{AuthoredGateVariant, Gate},
        habitat_boundary_queries::HabitatBoundaryQueries,
        topology_graph_types::{FenceSegment, TopologyGrid, TopologyNode},
    },
};

use super::topology_prefab_presentation::authored_gate_definition;

#[derive(Component)]
pub(crate) struct TankGatePlatformSide(usize);

pub(super) fn project_tank_gate_platforms(
    mut commands: Commands,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active: Res<WorldDefinitions>,
    prefabs: Res<Assets<ScenePrefabAsset>>,
    mut roots: Query<(&TopologyGrid, &HabitatIndex, &mut PersistentIdAllocator)>,
    nodes: Query<&TopologyNode>,
    cell_owners: Query<&HabitatCellOf>,
    tanks: Query<&TankGeometry, With<Tank>>,
    boundaries: HabitatBoundaryQueries,
    ground: TankWallGroundSampling,
    gates: Query<
        (
            Entity,
            FenceSegment,
            &WorldMember,
            &GlobalTransform,
            Option<&AuthoredGateVariant>,
            Option<&Children>,
        ),
        (With<Gate>, With<TankWallTopHeight>),
    >,
    mut platforms: Query<(&TankGatePlatformSide, &DefinitionId, &mut Transform)>,
) {
    let Some(catalogue) = active.get(&definitions) else {
        return;
    };
    let platform_key = AssetId::from_key("platformtype");
    for (gate, segment, member, pose, variant, children) in &gates {
        let Ok((grid, habitats, mut ids)) = roots.get_mut(member.root) else {
            continue;
        };
        let (Ok(a), Ok(b)) = (nodes.get(segment.a()), nodes.get(segment.b())) else {
            continue;
        };
        let first = grid.cell_translation(a.cell).xz();
        let last = grid.cell_translation(b.cell).xz();
        let Some(heading) = (last - first).try_normalize() else {
            continue;
        };
        let Some(units) = ground.terrain_units_at_topology_cell(member.root, a.cell) else {
            continue;
        };
        let platform = authored_gate_definition(catalogue, segment.definition, variant)
            .and_then(|fence| catalogue.find_object(fence.object))
            .and_then(|object| {
                object
                    .authored_runtime
                    .morph_targets
                    .iter()
                    .find_map(|&(key, platform)| (key == platform_key).then_some(platform))
            })
            .and_then(|id| {
                let object = catalogue.find_object(id)?;
                Some((id, catalogue.scene(object.prefab)?, object.prefab_scale))
            });
        let midpoint = first.midpoint(last);
        let outward = Vec2::new(-heading.y, heading.x);
        let offset = outward * units.cell_size_metres;
        let mut existing = [None; 2];
        for child in children.into_iter().flat_map(|children| children.iter()) {
            if let Ok((role, ..)) = platforms.get(child) {
                existing[role.0] = Some(child);
            }
        }
        for (side, sign) in [1.0, -1.0].into_iter().enumerate() {
            let xy = midpoint + sign * offset;
            let tank = locate_habitat_at_world_position(habitats, &cell_owners, xy, *grid)
                .filter(|&tank| boundaries.habitats(gate).any(|habitat| habitat == tank))
                .and_then(|tank| tanks.get(tank).ok());
            let (Some(tank), Some((definition, handle, scale))) = (tank, platform.as_ref()) else {
                if let Some(child) = existing[side] {
                    commands.entity(child).try_despawn();
                }
                continue;
            };
            let facing = sign * outward;
            let world_pose = Transform::from_xyz(xy.x, tank.water_height, xy.y)
                .with_rotation(Quat::from_rotation_y((-facing.x).atan2(-facing.y)))
                .with_scale(Vec3::splat(*scale));
            let local_pose = GlobalTransform::from(world_pose).reparented_to(pose);
            if let Some(child) = existing[side] {
                if let Ok((_, live_definition, mut transform)) = platforms.get_mut(child) {
                    if live_definition.0 == *definition {
                        transform.set_if_neq(local_pose);
                        continue;
                    }
                }
                commands.entity(child).try_despawn();
            }
            let Some(prefab) = prefabs.get(handle) else {
                continue;
            };
            let Ok(id) = ids.allocate() else {
                continue;
            };
            let child = spawn_loaded_scene_prefab_as_world_instance(
                &mut commands,
                prefab,
                handle.clone(),
                member.root,
                *definition,
                id,
                local_pose,
                true,
                Some(gate),
                RigidBody::Static,
            );
            commands.entity(child).insert(TankGatePlatformSide(side));
        }
    }
}

pub(super) fn remove_platforms_when_tank_gate_is_removed(
    event: On<Discard<(Gate, TankWallTopHeight)>>,
    children: Query<&Children>,
    platforms: Query<(), With<TankGatePlatformSide>>,
    mut commands: Commands,
) {
    if let Ok(children) = children.get(event.entity) {
        for child in children.iter().filter(|child| platforms.contains(*child)) {
            commands.entity(child).try_despawn();
        }
    }
}
