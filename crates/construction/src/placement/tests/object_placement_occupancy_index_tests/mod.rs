use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::topology::topology_graph_types::TopologyIndex;
use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use bevy::prelude::*;
use openzt2_game_data::AssetId;

use openzt2_zoo_state::construction::construction_interaction_types::ConstructionPreview;
use openzt2_zoo_state::construction::construction_interaction_types::PlacementValidity;
use openzt2_zoo_state::economy::money_types::Money;

use crate::placement::object_placement_preview_validation::{
    mark_changed_object_placement_previews_dirty, ObjectPlacementPreviewDirty,
};
use openzt2_zoo_state::placement::object_placement_geometry::ScenePlacementFootprintChanged;
use openzt2_zoo_state::placement::placement_occupancy_index::PlacedObjectFootprintOccupancyIndex;
use openzt2_zoo_state::placement::placement_preview_types::ObjectPlacementPreviewPermissionFacts;

#[test]
fn occupancy_index_change_revalidates_an_unchanged_continuing_preview() {
    let mut app = App::new();
    app.init_resource::<Assets<TerrainAsset>>()
        .init_resource::<Assets<WorldDefinitionAsset>>()
        .init_resource::<WorldDefinitions>()
        .add_message::<openzt2_zoo_state::terrain::terrain_edit_types::TerrainChanged>()
        .add_message::<openzt2_zoo_state::habitat::habitat_types::ContainmentChanged>()
        .add_message::<ScenePlacementFootprintChanged>()
        .add_systems(Update, mark_changed_object_placement_previews_dirty);
    let root = app
        .world_mut()
        .spawn((
            PlacedObjectFootprintOccupancyIndex::default(),
            TopologyIndex::default(),
        ))
        .id();
    let preview = app
        .world_mut()
        .spawn((
            WorldMember { root },
            ConstructionPreview {
                definition: AssetId([3; 16]),
                transform: Transform::IDENTITY,
                validity: PlacementValidity::Valid { cost: Money(100) },
            },
            ObjectPlacementPreviewPermissionFacts {
                unlocked: true,
                affordable: true,
                topology_valid: true,
                headroom_valid: true,
                surface_valid: true,
                prefab_ready: true,
            },
        ))
        .id();
    app.update();
    assert!(app
        .world()
        .get::<ObjectPlacementPreviewDirty>(preview)
        .is_some());
    app.world_mut()
        .entity_mut(preview)
        .remove::<ObjectPlacementPreviewDirty>();
    app.update();
    assert!(app
        .world()
        .get::<ObjectPlacementPreviewDirty>(preview)
        .is_none());
    app.world_mut().spawn((
        WorldMember { root },
        openzt2_zoo_state::placement::placed_object_types::IndexedObjectPlacementCells(Box::new([
            IVec2::ZERO,
        ])),
    ));

    app.update();

    assert!(app
        .world()
        .get::<ObjectPlacementPreviewDirty>(preview)
        .is_some());
}
