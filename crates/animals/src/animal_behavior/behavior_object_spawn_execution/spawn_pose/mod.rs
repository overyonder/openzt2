use bevy::{ecs::system::SystemParam, prelude::*};
use openzt2_game_assets::{
    terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset,
    world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView,
};
use openzt2_game_data::world_definitions::object_placement::placement_stomp_rules::PlacementStompAction;
use openzt2_game_data::{behavior::action::spawn::BehaviorSpawnPosePolicy, AssetId};
use openzt2_world_state::simulation_time::deterministic_random_stream::DeterministicRng;
use openzt2_world_state::world_spawn::world_membership_types::{DefinitionId, WorldMember};
use openzt2_zoo_state::topology::topology_graph_types::FenceSegment;
use openzt2_zoo_state::{
    construction::construction_interaction_types::PlacementValidity,
    habitat::{
        habitat_membership_and_containment::locate_habitat_at_world_position,
        habitat_types::{HabitatCellOf, HabitatIndex},
    },
    locomotion::locomotion_types::{NavAgent, NavFlags},
    placement::{
        ground_path_placement_stomp::{GroundPathPlacementAction, GroundPathPlacementStomp},
        object_placement_validation::{
            object_placement_stomp_action, validate_object_placement_footprint_cells,
            ObjectPlacementAuthoritativeFacts,
        },
        placement_occupancy_index::PlacedObjectFootprintQueries,
    },
    terrain::{
        dry_surface_eligibility::DrySurfaceEligibility,
        terrain_chunk_types::{EditedTerrainSamples, TerrainChunk, TerrainIndex},
        terrain_world_sampling::{sample_terrain, terrain_chunk_at},
    },
    topology::{
        topology_edit_types::{TopologyChanged, TopologySnapshot},
        topology_graph_types::{TopologyGrid, TopologyIndex, TopologyNode},
    },
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(SystemParam)]
pub(super) struct SpawnPoseValidation<'w, 's> {
    paths: GroundPathPlacementStomp<'w, 's>,
    surfaces: DrySurfaceEligibility<'w, 's>,
    fences: Query<'w, 's, FenceSegment>,
    nodes: Query<'w, 's, &'static TopologyNode>,
    roots: Query<
        'w,
        's,
        (
            &'static TopologyGrid,
            &'static TopologyIndex,
            &'static HabitatIndex,
            &'static TerrainIndex,
        ),
    >,
    occupancy: PlacedObjectFootprintQueries<'w, 's>,
    cell_owners: Query<'w, 's, &'static HabitatCellOf>,
    occupants: Query<'w, 's, (&'static DefinitionId, &'static WorldMember)>,
    agents: Query<'w, 's, &'static NavAgent>,
    transforms: Query<'w, 's, &'static GlobalTransform>,
    terrain_assets: Res<'w, Assets<TerrainAsset>>,
    terrain_chunks: Query<'w, 's, (&'static TerrainChunk, Option<&'static EditedTerrainSamples>)>,
}

pub(super) struct BehaviorSpawnPlacement {
    pub(super) transform: Transform,
    stomped_objects: BTreeSet<Entity>,
    stomped_paths: BTreeMap<Entity, IVec2>,
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
            if let Some(placement) = self.validate_pose(actor, world, object, &candidate, catalogue)
            {
                return placement;
            }
        }
        BehaviorSpawnPlacement::new(actor_pose)
    }

    fn validate_pose(
        &self,
        actor: Entity,
        world: WorldMember,
        object: AssetId,
        pose: &Transform,
        catalogue: WorldDefinitionsView<'_>,
    ) -> Option<BehaviorSpawnPlacement> {
        if !pose.translation.is_finite() {
            return None;
        }
        let (grid, topology, habitats, terrain_index) = self.roots.get(world.root).ok()?;
        let point = pose.translation.xz();
        // The spawn clearance probe excludes fence segments within two metres.
        let spacing = grid.spacing_m;
        if !spacing.is_finite() || spacing <= 0.0 {
            return None;
        }
        let min = ((point - Vec2::splat(2.0) - grid.origin) / spacing)
            .floor()
            .as_ivec2();
        let max = ((point + Vec2::splat(2.0) - grid.origin) / spacing)
            .ceil()
            .as_ivec2();
        let nearby_fences = topology.fences_within(min, max);
        if self.fences.iter_many(nearby_fences).flatten().any(|edge| {
            let (Ok(a), Ok(b)) = (self.nodes.get(edge.a()), self.nodes.get(edge.b())) else {
                return false;
            };
            let a = grid.cell_translation(a.cell).xz();
            let b = grid.cell_translation(b.cell).xz();
            let delta = b - a;
            let along =
                ((point - a).dot(delta) / delta.length_squared().max(f32::EPSILON)).clamp(0.0, 1.0);
            point.distance_squared(a + delta * along) <= 4.0
        }) {
            return None;
        }
        let on_land = self.surfaces.on_land(world.root, point)?;
        if self
            .agents
            .get(actor)
            .is_ok_and(|agent| !agent.capabilities.contains(NavFlags::AMPHIBIOUS))
            && !on_land
        {
            return None;
        }
        let mut placement = BehaviorSpawnPlacement::new(*pose);
        let Some(placeable) = catalogue.find_placeable(object) else {
            return Some(placement);
        };
        let terrain = |position| {
            let (chunk, edited) = self
                .terrain_chunks
                .get(terrain_chunk_at(terrain_index, position)?)
                .ok()?;
            let mut sample = sample_terrain(
                chunk,
                self.terrain_assets.get(&chunk.asset)?,
                edited,
                position,
            )?;
            sample.water_height_m = self.surfaces.water_surface_height(world.root, position);
            Some(sample)
        };
        let path_overlaps = self.paths.overlaps(world.root, placeable, catalogue);
        matches!(validate_object_placement_footprint_cells(placeable, &placeable.footprint, pose, terrain,
            |cell| {
                for action in path_overlaps.actions_at(cell) {
                    match action {
                        GroundPathPlacementAction::Prevent => return true,
                        GroundPathPlacementAction::Delete(entity, TopologySnapshot::Path { cell, .. }) => {
                            placement.stomped_paths.insert(entity, cell.xy());
                        }
                        _ => {}
                    }
                }
                self.occupancy.entities_occupying_cell(world.root, cell).filter(|entity| *entity != actor).any(|occupant| {
                let Ok((definition, member)) = self.occupants.get(occupant) else {
                    return true;
                };
                match object_placement_stomp_action(placeable, catalogue, Some(definition.0)) {
                    PlacementStompAction::Allow => false,
                    PlacementStompAction::Delete if member.root == world.root => {
                        placement.stomped_objects.insert(occupant);
                        false
                    }
                    PlacementStompAction::Delete | PlacementStompAction::Prevent => true,
                }
                })
            },
            ObjectPlacementAuthoritativeFacts {
                habitat: locate_habitat_at_world_position(habitats, &self.cell_owners, point, *grid),
                unlocked: true, affordable: true,
                topology_valid: !placeable.constraints.contains_any(
                    openzt2_game_data::world_definitions::object_placement::PlacementConstraints::REQUIRE_PATH
                        .with_additional_flags(openzt2_game_data::world_definitions::object_placement::PlacementConstraints::REQUIRE_WALL)),
                headroom_valid: placeable.minimum_headroom_metres <= 0.0,
                surface_valid: true,
            }), PlacementValidity::Valid { .. }).then_some(placement)
    }
}

impl BehaviorSpawnPlacement {
    fn new(transform: Transform) -> Self {
        Self {
            transform,
            stomped_objects: BTreeSet::new(),
            stomped_paths: BTreeMap::new(),
        }
    }

    pub(super) fn apply_stomps(self, commands: &mut Commands) {
        for occupant in self.stomped_objects {
            commands.entity(occupant).try_despawn();
        }
        let mut cells = self.stomped_paths.into_iter();
        let Some((first_path, first_cell)) = cells.next() else {
            return;
        };
        commands.entity(first_path).try_despawn();
        let mut bounds = IRect::from_corners(first_cell, first_cell);
        for (path, cell) in cells {
            commands.entity(path).try_despawn();
            bounds.min = bounds.min.min(cell);
            bounds.max = bounds.max.max(cell);
        }
        commands.write_message(TopologyChanged {
            transaction: None,
            bounds,
        });
    }
}
