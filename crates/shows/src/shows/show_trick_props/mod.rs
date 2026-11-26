use avian3d::prelude::{RigidBody, SpatialQuery, SpatialQueryFilter};
use bevy::{ecs::system::SystemParam, gltf::Gltf, platform::collections::HashMap, prelude::*};
use openzt2_game_assets::{
    scene_prefab::ScenePrefabAsset,
    world_definitions::{
        world_definition_asset_set_state_and_borrowing_queries::{
            WorldDefinitions, WorldDefinitionsView,
        },
        world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
    },
};
use openzt2_game_data::{
    world_definitions::{
        animal_shows_and_training::TrickDefinition,
        object_placement::{FootprintCellFlags, PlaceableDefinition},
    },
    AssetId,
};
use openzt2_world::world_spawn::prefab_world_instance_spawning::spawn_loaded_scene_prefab_as_world_instance;
use openzt2_world_state::world_spawn::{
    persistent_id_types::PersistentIdAllocator,
    prefab_model_readiness::first_missing_prefab_collider_model_asset_id,
    world_membership_types::{WorldMember, WorldRoot},
};
use openzt2_zoo_state::{
    aquatic::aquatic_simulation_types::{Tank, TankGeometry},
    habitat::habitat_types::{HabitatMember, HabitatRegion},
    placement::{
        object_placement_geometry::ObjectPlacementGeometry,
        object_placement_validation::{
            calculate_authored_object_placement_eighth_turns,
            calculate_object_placement_footprint_origin,
            rotate_object_placement_point_by_quarter_turns,
        },
    },
    shows::{
        running_show_types::{RunningShow, ShowPhase},
        show_stage_types::ShowStage,
    },
    topology::topology_graph_types::TopologyGrid,
};

use super::running_show_schedule::ShowRunSlotOrder;
use openzt2_zoo_state::shows::show_schedule_types::ScheduledShowTrickPerformance;

#[derive(Component)]
#[relationship(relationship_target = ShowTrickProp)]
pub(super) struct ShowTrickPropOf(Entity);

#[derive(Component)]
#[relationship_target(relationship = ShowTrickPropOf, linked_spawn)]
pub(super) struct ShowTrickProp(Entity);

#[derive(Component)]
pub(super) struct ShowPropAssets(HashMap<AssetId, Handle<ScenePrefabAsset>>);

pub(super) fn load_scheduled_prop_prefabs(
    runs: Query<(Entity, &RunningShow, &ShowRunSlotOrder), Without<ShowPropAssets>>,
    slots: Query<&ScheduledShowTrickPerformance>,
    definitions: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    mut commands: Commands,
) {
    let Some(definitions) = definitions.get(&assets) else {
        return;
    };
    if definitions.has_pending_assets() {
        return;
    }
    for (entity, run, order) in &runs {
        if run.phase != ShowPhase::Preshow {
            continue;
        }
        let handles: HashMap<_, _> = slots
            .iter_many(order.slots())
            .flatten()
            .filter_map(|slot| {
                let target = definitions.find_trick(slot.trick)?.target;
                let object = prop_definition(definitions, target)?;
                Some((target, definitions.scene(object.prefab)?))
            })
            .collect();
        commands.entity(entity).insert(ShowPropAssets(handles));
    }
}

fn prop_definition(
    definitions: WorldDefinitionsView<'_>,
    target: AssetId,
) -> Option<&openzt2_game_data::world_definitions::world_objects::WorldObjectDefinition> {
    definitions.find_object(target).or_else(|| {
        definitions
            .objects()
            .find(|object| object.behavior_type_identifiers.contains(&target))
    })
}

#[derive(SystemParam)]
pub(super) struct ShowPropPlacement<'w, 's> {
    stages: Query<'w, 's, (&'static Transform, &'static WorldMember), With<ShowStage>>,
    performers: Query<'w, 's, &'static HabitatMember>,
    tanks: Query<
        'w,
        's,
        (
            &'static HabitatRegion,
            &'static TankGeometry,
            &'static WorldMember,
        ),
        With<Tank>,
    >,
    grids: Query<'w, 's, &'static TopologyGrid>,
    spatial: SpatialQuery<'w, 's>,
    requested: Query<'w, 's, &'static ShowPropAssets>,
    prefabs: Res<'w, Assets<ScenePrefabAsset>>,
    models: Res<'w, Assets<Gltf>>,
    allocators: Query<'w, 's, &'static mut PersistentIdAllocator, With<WorldRoot>>,
}

impl ShowPropPlacement<'_, '_> {
    pub(super) fn spawn(
        &mut self,
        definitions: WorldDefinitionsView<'_>,
        trick: &TrickDefinition,
        stage: Entity,
        performer: Entity,
        show: Entity,
        action: Entity,
        commands: &mut Commands,
    ) -> Option<Entity> {
        let (stage_pose, member) = self.stages.get(stage).ok()?;
        let grid = self.grids.get(member.root).ok()?;
        let habitat = self.performers.get(performer).ok()?;
        let (region, tank, tank_world) = self.tanks.get(habitat.habitat_entity).ok()?;
        let cell_at = |point: Vec2| ((point - grid.origin) / grid.spacing_m).floor().as_ivec2();
        let contains = |cell: IVec2| {
            region
                .topology_cells
                .binary_search_by_key(&(cell.x, cell.y), |cell| (cell.x, cell.y))
                .is_ok()
        };
        if tank_world.root != member.root
            || !tank.is_valid()
            || !contains(cell_at(stage_pose.translation.xz()))
        {
            return None;
        }

        let object = prop_definition(definitions, trick.target)?;
        let handle = self.requested.get(show).ok()?.0.get(&trick.target)?.clone();
        let prefab = self.prefabs.get(&handle)?;
        if first_missing_prefab_collider_model_asset_id(prefab, &self.models).is_some() {
            return None;
        }
        let placement = definitions.find_placeable(object.id);
        let yaw = stage_pose.rotation.to_euler(EulerRot::YXZ).0;
        let stage_heading = Quat::from_rotation_y(yaw);
        let rotation =
            Quat::from_rotation_y(yaw + object.authored_runtime.prop_rotation_offset_radians);
        let height = object.terrain_fit_water_offset_m.map_or(
            tank.floor_height + object.terrain_fit_height_offset_m,
            |offset| tank.water_height + offset,
        );
        let preferred = stage_pose.translation - stage_heading * Vec3::Z * grid.spacing_m;
        let depth = tank.depth();
        let depth_matches = placement.is_none_or(|placement| {
            let surface = placement.surface_policy;
            (surface.minimum_depth_metres < 0.0 || depth >= surface.minimum_depth_metres)
                && (surface.maximum_depth_metres < 0.0 || depth <= surface.maximum_depth_metres)
        });
        let candidates = || {
            region.topology_cells.iter().map(|&cell| {
                let center = grid.origin + (cell.as_vec2() + Vec2::splat(0.5)) * grid.spacing_m;
                Transform::from_xyz(center.x, height, center.y)
                    .with_rotation(rotation)
                    .with_scale(Vec3::splat(object.prefab_scale))
            })
        };
        let clear = |candidate: &Transform| {
            let headroom = placement.map_or(0.0, |definition| definition.minimum_headroom_metres);
            headroom <= 0.0
                || self
                    .spatial
                    .cast_ray(
                        candidate.translation + Vec3::Y * 0.01,
                        Dir3::Y,
                        headroom,
                        false,
                        &SpatialQueryFilter::DEFAULT,
                    )
                    .is_none()
        };
        let nearest = |left: &Transform, right: &Transform| {
            left.translation
                .xz()
                .distance_squared(preferred.xz())
                .total_cmp(&right.translation.xz().distance_squared(preferred.xz()))
        };
        let pose = candidates()
            .filter(|pose| {
                depth_matches
                    && clear(pose)
                    && placement.is_none_or(|placement| {
                        footprint_in_tank(placement, pose, |point| contains(cell_at(point)))
                    })
            })
            .min_by(nearest)
            .or_else(|| candidates().filter(clear).min_by(nearest))?;

        let persistent_id = self.allocators.get_mut(member.root).ok()?.allocate().ok()?;
        let prop = spawn_loaded_scene_prefab_as_world_instance(
            commands,
            prefab,
            handle,
            member.root,
            object.id,
            persistent_id,
            pose,
            true,
            None,
            RigidBody::Static,
        );
        commands
            .entity(prop)
            .insert((ShowTrickPropOf(action), *habitat));
        Some(prop)
    }
}

fn footprint_in_tank(
    placement: &PlaceableDefinition,
    pose: &Transform,
    contains: impl Fn(Vec2) -> bool,
) -> bool {
    let Some(turns) = calculate_authored_object_placement_eighth_turns(placement, pose) else {
        return false;
    };
    let geometry = ObjectPlacementGeometry::from(placement);
    let Some(origin) = calculate_object_placement_footprint_origin(geometry, pose, turns) else {
        return false;
    };
    geometry
        .cells(turns)
        .iter()
        .filter(|cell| cell.flags.contains_all(FootprintCellFlags::OCCUPIED))
        .all(|cell| {
            let point = Vec2::new(f32::from(cell.offset[0]), f32::from(cell.offset[1]));
            contains(
                origin.as_vec2() + rotate_object_placement_point_by_quarter_turns(point, turns / 2),
            )
        })
}
