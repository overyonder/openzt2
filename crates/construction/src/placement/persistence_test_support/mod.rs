use bevy::{ecs::system::SystemState, prelude::*};
use openzt2_game_assets::{
    scene_prefab::ScenePrefabAsset,
    world_definitions::{
        world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
        world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
    },
};
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_zoo_state::{
    construction::construction_tool_and_placement_policy_types::ConstructionTool,
    economy::account_transaction_types::TransactionSettlement,
    placement::{
        placed_object_types::{PlacedObjectDefinitionReference, PlacedObjectFootprintOccupancy},
        placement_occupancy_index::{
            PlacedObjectFootprintOccupancyIndex, PlacedObjectFootprintQueries,
        },
        placement_preview_types::ObjectPlacementCellCollectionScratch,
        placement_transaction_types::{ObjectPlacementCommitted, PreparedObjectPlacementMutation},
    },
};

pub fn add_placement_persistence_test_systems(app: &mut App) {
    app.register_required_components::<openzt2_world_state::world_spawn::world_membership_types::WorldRoot, PlacedObjectFootprintOccupancyIndex>()
        .init_resource::<ObjectPlacementCellCollectionScratch>()
        .init_resource::<ConstructionTool>()
        .add_message::<ObjectPlacementCommitted>()
        .init_resource::<openzt2_zoo_state::placement::object_placement_geometry::ScenePlacementFootprints>()
        .add_message::<openzt2_zoo_state::placement::object_placement_geometry::ScenePlacementFootprintChanged>()
        .add_systems(Update, (
            super::placed_object_footprint_occupancy_index_synchronization::hydrate_placed_object_placement_components,
            super::placed_object_footprint_occupancy_index_synchronization::synchronize_placed_footprints,
        ).chain());
}

pub fn apply_settled_placement_purchase_for_test(
    world: &mut World,
    root: Entity,
    transaction: Entity,
    mut edit: PreparedObjectPlacementMutation,
) -> Entity {
    assert!(world
        .get::<TransactionSettlement>(transaction)
        .is_some_and(|settlement| settlement.is_settled_in_domain(true)));
    let mut state = SystemState::<(
        Commands,
        Res<WorldDefinitions>,
        Res<Assets<WorldDefinitionAsset>>,
        PlacedObjectFootprintQueries,
        ResMut<ObjectPlacementCellCollectionScratch>,
        Res<Assets<ScenePrefabAsset>>,
        Res<Assets<bevy::gltf::Gltf>>,
        Query<(
            &PersistentId,
            &PlacedObjectDefinitionReference,
            &Transform,
            &PlacedObjectFootprintOccupancy,
        )>,
        ResMut<ConstructionTool>,
        MessageWriter<ObjectPlacementCommitted>,
    )>::new(world);
    let (
        mut commands,
        active,
        assets,
        occupancy,
        mut scratch,
        prefabs,
        models,
        placed,
        mut tool,
        mut events,
    ) = state.get_mut(world).unwrap();
    assert!(
        super::placed_object_creation_edit_application::apply_prepared_placed_object_creation_edit(
            &mut commands,
            transaction,
            &mut edit,
            active.get(&assets).unwrap(),
            root,
            &occupancy,
            &mut scratch,
            &openzt2_zoo_state::placement::object_placement_geometry::ScenePlacementFootprints::default(),
            &prefabs,
            &models,
            &placed,
            &[],
            false,
            &mut tool,
            &mut events,
        )
    );
    let entity = edit.applied_entity.unwrap();
    state.apply(world);
    entity
}
