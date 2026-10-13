use super::*;

#[test]
fn replacement_definition_hides_old_quantity_before_the_registry_is_available() {
    let mut app = App::new();
    app.init_resource::<Assets<WorldDefinitionAsset>>()
        .init_resource::<WorldDefinitions>()
        .add_systems(
            Update,
            hydrate_food_and_drink_container_quantities_for_new_world_objects,
        );
    let original = AssetId::from_key("original-food-container");
    let replacement = AssetId::from_key("replacement-container");
    let entity = app
        .world_mut()
        .spawn((
            DefinitionId(original),
            ContainerQuantityResolved {
                definition_id: original,
                definitions_revision: 0,
            },
            FoodContainer {
                food: original,
                amount_q16: 25 << 16,
                capacity_q16: AUTHORED_CONTAINER_CAPACITY_Q16,
            },
        ))
        .id();
    app.update();
    assert!(app.world().get::<FoodContainer>(entity).is_some());

    app.world_mut()
        .entity_mut(entity)
        .insert(DefinitionId(replacement));
    app.update();
    assert!(app.world().get::<FoodContainer>(entity).is_none());
    assert!(app.world().get::<DrinkContainer>(entity).is_none());
    assert!(app
        .world()
        .get::<ContainerQuantityResolved>(entity)
        .is_none());
}
