use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use bevy::prelude::*;
use openzt2_game_data::AssetId;

use openzt2_zoo_state::construction::construction_interaction_types::ConstructionPreview;
use openzt2_zoo_state::construction::construction_interaction_types::PlacementValidity;
use openzt2_zoo_state::economy::money_types::Money;

use crate::placement::object_placement_preview_validation::request_changed_object_placement_preview_evaluations;
use openzt2_zoo_state::placement::placement_occupancy_index::PlacedObjectFootprintOccupancyIndex;
use openzt2_zoo_state::placement::placement_preview_types::ObjectPlacementPreviewPermissionFacts;
use openzt2_zoo_state::placement::placement_transaction_types::EvaluateObjectPlacementPreviewRequest;

#[test]
fn occupancy_index_change_revalidates_an_unchanged_continuing_preview() {
    let mut app = App::new();
    app.init_resource::<PlacedObjectFootprintOccupancyIndex>()
        .init_resource::<openzt2_zoo_state::topology::topology_graph_types::TopologyIndex>()
        .init_resource::<Assets<TerrainAsset>>()
        .init_resource::<Assets<WorldDefinitionAsset>>()
        .init_resource::<WorldDefinitions>()
        .add_message::<openzt2_zoo_state::terrain::terrain_edit_types::TerrainChanged>()
        .add_message::<openzt2_zoo_state::habitat::habitat_types::ContainmentChanged>()
        .add_message::<EvaluateObjectPlacementPreviewRequest>()
        .add_systems(Update, request_changed_object_placement_preview_evaluations);
    app.world_mut().spawn((
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
            prefab_ready: true,
        },
    ));
    let mut cursor = app
        .world()
        .resource::<Messages<EvaluateObjectPlacementPreviewRequest>>()
        .get_cursor();
    app.update();
    assert_eq!(
        cursor
            .read(
                app.world()
                    .resource::<Messages<EvaluateObjectPlacementPreviewRequest>>()
            )
            .count(),
        1
    );
    app.update();
    assert_eq!(
        cursor
            .read(
                app.world()
                    .resource::<Messages<EvaluateObjectPlacementPreviewRequest>>()
            )
            .count(),
        0
    );
    let occupied = app.world_mut().spawn_empty().id();
    app.world_mut()
        .resource_mut::<PlacedObjectFootprintOccupancyIndex>()
        .insert_entity_into_cell(IVec2::ZERO, occupied);

    app.update();

    let messages = app
        .world()
        .resource::<Messages<EvaluateObjectPlacementPreviewRequest>>();
    assert_eq!(cursor.read(messages).count(), 1);
}
