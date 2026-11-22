use super::*;
use openzt2_game_data::AssetId;
use openzt2_interface_state::input::input_types::ActionSource;

fn id(byte: u8) -> AssetId {
    AssetId([byte; 16])
}

fn test_app() -> (App, Entity) {
    let mut app = App::new();
    app.add_message::<SelectionRequest>()
        .add_message::<SelectionChanged>()
        .add_observer(clear_world_ui_selection_on_loss_of_inspectability)
        .add_systems(Update, apply_world_ui_selection);
    let owner = app.world_mut().spawn(InGameUiOwner).id();
    (app, owner)
}

#[test]
fn selection_uses_last_valid_request_and_emits_once() {
    let (mut app, owner) = test_app();
    let valid = app
        .world_mut()
        .spawn(Inspectable { definition: id(1) })
        .id();
    let invalid = app.world_mut().spawn_empty().id();
    app.world_mut().write_message(SelectionRequest {
        entity: Some(valid),
        source: ActionSource::KeyboardMouse,
    });
    app.world_mut().write_message(SelectionRequest {
        entity: Some(invalid),
        source: ActionSource::KeyboardMouse,
    });
    app.update();

    assert_eq!(
        app.world()
            .get::<SelectedSubject>(owner)
            .map(|selection| selection.0),
        Some(valid)
    );
    let events = app.world().resource::<Messages<SelectionChanged>>();
    assert_eq!(events.len(), 1);
}

#[test]
fn selecting_native_crate_uses_its_inspectable_animal() {
    use openzt2_world_state::world_spawn::world_entity_crating::AnimalCrate;
    let (mut app, owner) = test_app();
    let crate_entity = app
        .world_mut()
        .spawn(Inspectable { definition: id(2) })
        .id();
    let animal = app
        .world_mut()
        .spawn((Inspectable { definition: id(1) }, AnimalCrate(crate_entity)))
        .id();
    app.world_mut().write_message(SelectionRequest {
        entity: Some(crate_entity),
        source: ActionSource::KeyboardMouse,
    });
    app.update();
    assert_eq!(
        app.world()
            .get::<SelectedSubject>(owner)
            .map(|selection| selection.0),
        Some(animal)
    );
}

#[test]
fn removing_selected_inspectable_clears_it_once() {
    let (mut app, owner) = test_app();
    let subject = app
        .world_mut()
        .spawn(Inspectable { definition: id(1) })
        .id();
    app.world_mut().write_message(SelectionRequest {
        entity: Some(subject),
        source: ActionSource::KeyboardMouse,
    });
    app.update();
    app.world_mut().entity_mut(subject).remove::<Inspectable>();
    app.update();

    assert_eq!(
        app.world()
            .get::<SelectedSubject>(owner)
            .map(|selection| selection.0),
        None
    );
    let events = app.world().resource::<Messages<SelectionChanged>>();
    assert_eq!(events.len(), 2);
}
