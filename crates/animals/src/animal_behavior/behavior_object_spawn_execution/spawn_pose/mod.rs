use bevy::{ecs::system::SystemParam, prelude::*};
use openzt2_game_assets::{
    terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset,
    world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView,
};
use openzt2_game_data::world_definitions::object_placement::placement_stomp_rules::PlacementStompAction;
use openzt2_game_data::{behavior::action::spawn::BehaviorSpawnPosePolicy, AssetId};
use openzt2_world_state::simulation_time::deterministic_random_stream::DeterministicRng;
use openzt2_world_state::world_spawn::world_membership_types::{DefinitionId, WorldMember};
use openzt2_zoo_state::{
    construction::construction_interaction_types::PlacementValidity,
    habitat::{
        habitat_membership_and_containment::locate_habitat_at_world_position,
        habitat_types::HabitatIndex,
    },
    locomotion::locomotion_types::{NavAgent, NavFlags},
    placement::{
        object_placement_validation::{
            object_placement_stomp_action, validate_object_placement_footprint_cells,
            ObjectPlacementAuthoritativeFacts,
        },
        placement_occupancy_index::PlacedObjectFootprintOccupancyIndex,
    },
    terrain::{
        dry_surface_eligibility::DrySurfaceEligibility,
        terrain_chunk_types::{EditedTerrainSamples, TerrainChunk, TerrainIndex},
        terrain_world_sampling::{sample_terrain, terrain_chunk_at},
    },
    topology::topology_graph_types::{FenceEdge, TopologyGrid, TopologyNode},
};
use std::collections::BTreeSet;

#[derive(SystemParam)]
pub(super) struct SpawnPoseValidation<'w, 's> {
    surfaces: DrySurfaceEligibility<'w, 's>,
    fences: Query<'w, 's, &'static FenceEdge>,
    nodes: Query<'w, 's, &'static TopologyNode>,
    grid: Res<'w, TopologyGrid>,
    habitats: Res<'w, HabitatIndex>,
    occupancy: Res<'w, PlacedObjectFootprintOccupancyIndex>,
    occupants: Query<'w, 's, (&'static DefinitionId, &'static WorldMember)>,
    agents: Query<'w, 's, &'static NavAgent>,
    transforms: Query<'w, 's, &'static GlobalTransform>,
    terrain_index: Res<'w, TerrainIndex>,
    terrain_assets: Res<'w, Assets<TerrainAsset>>,
    terrain_chunks: Query<'w, 's, (&'static TerrainChunk, Option<&'static EditedTerrainSamples>)>,
}

pub(super) struct BehaviorSpawnPlacement {
    pub(super) transform: Transform,
    pub(super) stomped_objects: BTreeSet<Entity>,
}

impl SpawnPoseValidation<'_, '_> {
    pub(super) fn actor_pose(&self, actor: Entity) -> Option<Transform> {
        self.transforms
            .get(actor)
            .ok()
            .map(GlobalTransform::compute_transform)
    }
    pub(super) fn select_spawn_pose(
        &self,
        actor: Entity,
        world: WorldMember,
        object: AssetId,
        node: Transform,
        actor_pose: Transform,
        policy: BehaviorSpawnPosePolicy,
        catalogue: WorldDefinitionsView<'_>,
        random: &mut DeterministicRng,
    ) -> BehaviorSpawnPlacement {
        for _ in 0..50 {
            let mut candidate = node;
            if policy.random_position_offset_m > 0.0 {
                candidate.translation.x +=
                    (random.unit_f32() * 2.0 - 1.0) * policy.random_position_offset_m;
                candidate.translation.z +=
                    (random.unit_f32() * 2.0 - 1.0) * policy.random_position_offset_m;
            }
            if policy.random_rotation_offset_radians > 0.0 {
                candidate.rotation = Quat::from_rotation_y(
                    (random.unit_f32() * 2.0 - 1.0) * policy.random_rotation_offset_radians,
                );
            }
            if let Some(stomped_objects) =
                self.stomped_objects_for_valid_pose(actor, world, object, &candidate, catalogue)
            {
                return BehaviorSpawnPlacement {
                    transform: candidate,
                    stomped_objects,
                };
            }
        }
        BehaviorSpawnPlacement {
            transform: actor_pose,
            stomped_objects: BTreeSet::new(),
        }
    }

    fn stomped_objects_for_valid_pose(
        &self,
        actor: Entity,
        world: WorldMember,
        object: AssetId,
        pose: &Transform,
        catalogue: WorldDefinitionsView<'_>,
    ) -> Option<BTreeSet<Entity>> {
        if !pose.translation.is_finite() {
            return None;
        }
        let point = pose.translation.xz();
        // The spawn clearance probe excludes fence segments within two metres.
        if self.fences.iter().any(|edge| {
            let (Ok(a), Ok(b)) = (self.nodes.get(edge.a), self.nodes.get(edge.b)) else {
                return false;
            };
            let a = self.grid.cell_translation(a.cell).xz();
            let b = self.grid.cell_translation(b.cell).xz();
            let delta = b - a;
            let along =
                ((point - a).dot(delta) / delta.length_squared().max(f32::EPSILON)).clamp(0.0, 1.0);
            point.distance_squared(a + delta * along) <= 4.0
        }) {
            return None;
        }
        let on_land = self.surfaces.on_land(point)?;
        if self
            .agents
            .get(actor)
            .is_ok_and(|agent| !agent.capabilities.contains(NavFlags::AMPHIBIOUS))
            && !on_land
        {
            return None;
        }
        let mut stomped_objects = BTreeSet::new();
        let Some(placeable) = catalogue.find_placeable(object) else {
            return Some(stomped_objects);
        };
        let terrain = |world| {
            let (chunk, edited) = self
                .terrain_chunks
                .get(terrain_chunk_at(&self.terrain_index, world)?)
                .ok()?;
            let mut sample =
                sample_terrain(chunk, self.terrain_assets.get(&chunk.asset)?, edited, world)?;
            sample.water_height_m = self.surfaces.water_surface_height(world);
            Some(sample)
        };
        matches!(validate_object_placement_footprint_cells(placeable, &placeable.footprint, pose, terrain,
            |cell| self.occupancy.entities_occupying_cell(cell).filter(|entity| *entity != actor).any(|occupant| {
                let Ok((definition, member)) = self.occupants.get(occupant) else {
                    return true;
                };
                match object_placement_stomp_action(placeable, catalogue, Some(definition.0)) {
                    PlacementStompAction::Allow => false,
                    PlacementStompAction::Delete if member.root == world.root => {
                        stomped_objects.insert(occupant);
                        false
                    }
                    PlacementStompAction::Delete | PlacementStompAction::Prevent => true,
                }
            }),
            ObjectPlacementAuthoritativeFacts {
                habitat: locate_habitat_at_world_position(&self.habitats, point, *self.grid),
                unlocked: true, affordable: true,
                topology_valid: !placeable.constraints.contains_any(
                    openzt2_game_data::world_definitions::object_placement::PlacementConstraints::REQUIRE_PATH
                        .with_additional_flags(openzt2_game_data::world_definitions::object_placement::PlacementConstraints::REQUIRE_WALL)),
                headroom_valid: placeable.minimum_headroom_metres <= 0.0,
                surface_valid: true,
            }), PlacementValidity::Valid { .. }).then_some(stomped_objects)
    }
}
