use bevy::{ecs::system::SystemParam, prelude::*};
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use openzt2_game_data::{
    AssetId,
    world_definitions::object_placement::{PlaceableDefinition, placement_stomp_rules::PlacementStompAction},
};
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use crate::topology::topology_graph_types::{
    PathTile, TopologyGrid, TopologyIndex, TopologyProtected,
};
use crate::topology::topology_edit_types::TopologySnapshot;

const PATH_TILE_EDGE_CONTACT_TOLERANCE_M: f32 = 0.001;

#[derive(SystemParam)]
pub struct GroundPathPlacementStomp<'w, 's> {
    roots: Query<'w, 's, (&'static TopologyGrid, Ref<'static, TopologyIndex>)>,
    paths: Query<
        'w,
        's,
        (
            &'static PathTile,
            Option<&'static PersistentId>,
            Has<TopologyProtected>,
        ),
    >,
}

pub enum GroundPathPlacementAction {
    Allow,
    Prevent,
    Delete(Entity, TopologySnapshot),
}

pub struct GroundPathFootprintOverlaps<'a, 'w, 's> {
    paths: &'a GroundPathPlacementStomp<'w, 's>,
    definition: &'a PlaceableDefinition,
    catalogue: WorldDefinitionsView<'a>,
    grid_and_topology: Option<(&'a TopologyGrid, &'a TopologyIndex)>,
    search_reach: Option<f32>,
}

impl<'w, 's> GroundPathPlacementStomp<'w, 's> {
    pub fn topology_changed(&self, root: Entity) -> bool {
        self.roots
            .get(root)
            .is_ok_and(|(_, topology)| topology.is_changed())
    }

    pub fn overlaps<'a>(
        &'a self,
        root: Entity,
        definition: &'a PlaceableDefinition,
        catalogue: WorldDefinitionsView<'a>,
    ) -> GroundPathFootprintOverlaps<'a, 'w, 's> {
        let grid_and_topology = self
            .roots
            .get(root)
            .ok()
            .map(|(grid, topology)| (grid, topology.into_inner()));
        let search_reach = grid_and_topology
            .filter(|(grid, _)| {
                definition.stomp.grid_snap && grid.spacing_m.is_finite() && grid.spacing_m > 0.0
            })
            .map(|_| f32::from(catalogue.maximum_ground_path_width_cm()) * 0.005);
        GroundPathFootprintOverlaps {
            paths: self,
            definition,
            catalogue,
            grid_and_topology,
            search_reach,
        }
    }
}

impl GroundPathFootprintOverlaps<'_, '_, '_> {
    pub fn actions_at(&self, cell: IVec2) -> impl Iterator<Item = GroundPathPlacementAction> + '_ {
        let minimum = cell.as_vec2();
        let maximum = minimum + Vec2::ONE;
        self.grid_and_topology
            .zip(self.search_reach)
            .into_iter()
            .flat_map(move |((grid, topology), reach)| {
                let first = ((minimum - Vec2::splat(reach) - grid.origin) / grid.spacing_m)
                    .floor()
                    .as_ivec2();
                let last = ((maximum + Vec2::splat(reach) - grid.origin) / grid.spacing_m)
                    .ceil()
                    .as_ivec2();
                (first.x..=last.x).flat_map(move |x| {
                    (first.y..=last.y).map(move |y| (grid, topology, IVec3::new(x, y, 0)))
                })
            })
            .filter_map(move |(grid, topology, cell)| {
                let entity = *topology.paths.get(&cell)?;
                let (tile, id, protected) = self.paths.paths.get(entity).ok()?;
                let path = self.catalogue.find_path(tile.definition)?;
                let half_width =
                    f32::from(path.width_cm).mul_add(0.005, -PATH_TILE_EDGE_CONTACT_TOLERANCE_M);
                let centre = grid.cell_translation(tile.cell).xz();
                if path.elevated
                    || !(centre - Vec2::splat(half_width)).cmplt(maximum).all()
                    || !(centre + Vec2::splat(half_width)).cmpgt(minimum).all()
                {
                    return None;
                }
                let fallback = [
                    AssetId::from_key("entity"),
                    AssetId::from_key("path"),
                    path.id,
                ];
                let target = self.catalogue.placement_target_facts(path.object);
                let types = target.map_or(fallback.as_slice(), |target| target.types.as_slice());
                let action = self
                    .definition
                    .stomp
                    .action_for_target(types, target.map(|target| &target.properties))
                    .unwrap_or(PlacementStompAction::Prevent);
                Some(match (action, id, protected) {
                    (PlacementStompAction::Allow, _, _) => GroundPathPlacementAction::Allow,
                    (PlacementStompAction::Delete, Some(id), false) => {
                        GroundPathPlacementAction::Delete(
                            entity,
                            TopologySnapshot::Path {
                                id: *id,
                                definition: tile.definition,
                                cell: tile.cell,
                            },
                        )
                    }
                    _ => GroundPathPlacementAction::Prevent,
                })
            })
    }

    pub fn prevents_placement(&self, cell: IVec2) -> bool {
        self.actions_at(cell)
            .any(|action| matches!(action, GroundPathPlacementAction::Prevent))
    }
}
