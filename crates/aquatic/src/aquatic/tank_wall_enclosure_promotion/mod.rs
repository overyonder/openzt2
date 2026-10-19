use bevy::prelude::*;

use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_zoo_state::habitat::habitat_types::HabitatRegion;
use openzt2_zoo_state::topology::topology_graph_types::FenceEdge;

use super::aquatic_simulation_types::{
    Tank, TankEnvironment, TankGeometry, TankSurfaceHydrated, WaterQuality,
};

// ZTFenceMode's defaultTankBaseDepth and defaultTankWallHeight.
const DEFAULT_BASE_DEPTH_M: f32 = 3.0;
const DEFAULT_WALL_HEIGHT_M: f32 = 6.0;

/// Turns a habitat enclosed only by one tank wall type into a tank, as the
/// original does when a tank wall loop closes.
pub(super) fn promote_habitats_enclosed_by_tank_walls(
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    habitats: Query<(Entity, &HabitatRegion), (Changed<HabitatRegion>, Without<Tank>)>,
    fences: Query<(&FenceEdge, &GlobalTransform)>,
    mut commands: Commands,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    for (habitat, region) in &habitats {
        let Some(walls) = region
            .boundary_fence_entities
            .iter()
            .map(|fence| fences.get(*fence).ok())
            .collect::<Option<Vec<_>>>()
        else {
            continue;
        };
        let Some(definition) = walls.first().map(|(edge, _)| edge.definition) else {
            continue;
        };
        if definitions.find_tank(definition).is_none()
            || walls.iter().any(|(edge, _)| edge.definition != definition)
        {
            continue;
        }
        let ground = walls
            .iter()
            .map(|(_, transform)| transform.translation().y)
            .sum::<f32>()
            / walls.len() as f32;
        let wall_height = ground + DEFAULT_WALL_HEIGHT_M;
        let water_offset = definitions
            .tank_surface_policy()
            .map_or(0.0, |policy| policy.water_height_offset_cm as f32 / 100.0);
        let floor_height = ground - DEFAULT_BASE_DEPTH_M;
        let water_height = (wall_height - water_offset).max(floor_height);
        let area = region.area_square_metres.max(0.0);
        // The geometry is resolved here, so the surface offsets are not applied again.
        commands.entity(habitat).insert((
            Tank { definition },
            TankGeometry {
                floor_height,
                wall_height,
                water_height,
                area,
                volume: area * (water_height - floor_height),
            },
            TankEnvironment::default(),
            WaterQuality(1_000),
            TankSurfaceHydrated,
        ));
    }
}

#[cfg(test)]
mod tests {
    use openzt2_game_data::world_definitions::aquatic_habitats::TankDefinition;
    use openzt2_game_data::world_definitions::document::WorldDefinitionDocument;
    use openzt2_game_data::AssetId;

    use super::*;

    fn enclose(walls: [AssetId; 2]) -> Option<TankGeometry> {
        let tank_wall = AssetId::from_key("tankseg");
        let document = WorldDefinitionDocument {
            tanks: vec![TankDefinition {
                id: tank_wall,
                wall: tank_wall,
                floor_material: AssetId::default(),
                water_material: AssetId::default(),
                min_depth_cm: 0,
                max_depth_cm: 3_600,
                capacity_litres_per_cell: 0,
                filtration_per_day: 0,
            }],
            ..default()
        };
        let mut app = App::new();
        app.init_resource::<Assets<WorldDefinitionAsset>>()
            .add_systems(Update, promote_habitats_enclosed_by_tank_walls);
        let handle = app
            .world_mut()
            .resource_mut::<Assets<WorldDefinitionAsset>>()
            .add(WorldDefinitionAsset::from_test_document(document.clone()));
        let mut definitions = WorldDefinitions::default();
        definitions.index_test_document(handle, &document);
        app.insert_resource(definitions);
        let node = app.world_mut().spawn_empty().id();
        let boundary = walls.map(|definition| {
            app.world_mut()
                .spawn((
                    FenceEdge {
                        definition,
                        a: node,
                        b: node,
                    },
                    GlobalTransform::from_xyz(0.0, 2.0, 0.0),
                ))
                .id()
        });
        let habitat = app
            .world_mut()
            .spawn(HabitatRegion {
                topology_cells: Box::new([]),
                area_square_metres: 10.0,
                boundary_fence_entities: Box::new(boundary),
            })
            .id();
        app.update();
        app.world().get::<TankGeometry>(habitat).copied()
    }

    #[test]
    fn a_loop_of_one_tank_wall_becomes_a_tank() {
        let wall = AssetId::from_key("tankseg");
        let geometry = enclose([wall, wall]).expect("tank");
        assert_eq!(geometry.floor_height, -1.0);
        assert_eq!(geometry.wall_height, 8.0);
        assert_eq!(geometry.water_height, 8.0);
        assert_eq!(geometry.volume, 90.0);
        assert!(geometry.is_valid());
    }

    #[test]
    fn a_loop_closed_by_an_ordinary_fence_stays_a_habitat() {
        let wall = AssetId::from_key("tankseg");
        assert_eq!(enclose([wall, AssetId::from_key("chainlink")]), None);
    }
}
