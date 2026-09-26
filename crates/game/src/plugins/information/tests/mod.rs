use bevy::prelude::*;
use openzt2_game_data::ui_document::action::UiTrigger;
use openzt2_game_data::AssetId;

use super::{
    catalogue_types::{PurchaseChoice, SelectedCatalogueEntry},
    entity_selection_types::{Inspectable, SelectedEntity, SelectionChanged, SelectionRequest},
    selection::{apply_selection_requests, clear_removed_selection},
};
use crate::plugins::animal_lifecycle::animal_adoption_contracts::BeginAnimalAdoptionPlacement;
use crate::plugins::input::input_types::ActionSource;

fn id(byte: u8) -> AssetId {
    AssetId([byte; 16])
}

fn test_app() -> App {
    let mut app = App::new();
    app.init_resource::<SelectedEntity>()
        .init_resource::<SelectedCatalogueEntry>()
        .add_message::<SelectionRequest>()
        .add_message::<SelectionChanged>()
        .add_message::<PurchaseChoice>()
        .add_message::<BeginAnimalAdoptionPlacement>()
        .add_systems(
            Update,
            (apply_selection_requests, clear_removed_selection).chain(),
        );
    app
}

#[test]
fn selection_uses_last_valid_request_and_emits_once() {
    let mut app = test_app();
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

    assert_eq!(app.world().resource::<SelectedEntity>().0, Some(valid));
    let events = app.world().resource::<Messages<SelectionChanged>>();
    assert_eq!(events.len(), 1);
}

#[test]
fn removing_selected_inspectable_clears_it_once() {
    let mut app = test_app();
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

    assert_eq!(app.world().resource::<SelectedEntity>().0, None);
    let events = app.world().resource::<Messages<SelectionChanged>>();
    assert_eq!(events.len(), 2);
}

#[test]
fn authored_selection_action_clears_without_subject_and_selects_row_ancestor() {
    use bevy::ecs::system::SystemState;
    use openzt2_game_data::ui_document::action::information::UiInformationAction;

    use super::{
        entity_selection_types::InformationEntitySource,
        information_selection_action_operations::apply_authored_information_selection_action,
        information_selection_action_types::InformationSelectionActionTargets,
    };
    use crate::plugins::camera::camera_control_message_types::SetCameraMode;

    let mut app = test_app();
    app.add_message::<SetCameraMode>();
    let subject = app
        .world_mut()
        .spawn(Inspectable { definition: id(1) })
        .id();
    app.world_mut().resource_mut::<SelectedEntity>().0 = Some(subject);
    let close_button = app.world_mut().spawn_empty().id();
    let row = app.world_mut().spawn(InformationEntitySource(subject)).id();
    let row_button = app.world_mut().spawn(ChildOf(row)).id();
    let mut action_targets = SystemState::<InformationSelectionActionTargets>::new(app.world_mut());

    for (button, expected) in [(close_button, None), (row_button, Some(subject))] {
        apply_authored_information_selection_action(
            &UiInformationAction::SelectEntityFromSource,
            button,
            ActionSource::KeyboardMouse,
            &mut action_targets
                .get_mut(app.world_mut())
                .expect("selection action resources are installed"),
        );
        action_targets.apply(app.world_mut());
        app.update();
        assert_eq!(app.world().resource::<SelectedEntity>().0, expected);
    }
}

#[test]
fn adoption_initialization_dispatches_purchase_off_and_adoption_on_once() {
    assert_animal_catalogue_mode_initialization(
        crate::game_session_types::WorldSessionMode::Challenge,
    );
    assert_animal_catalogue_mode_initialization(
        crate::game_session_types::WorldSessionMode::Campaign,
    );
}

#[test]
fn freeform_initialization_uses_purchase_catalogue_instead_of_limited_offers() {
    assert_animal_catalogue_mode_initialization(
        crate::game_session_types::WorldSessionMode::Freeform,
    );
}

fn assert_animal_catalogue_mode_initialization(mode: crate::game_session_types::WorldSessionMode) {
    use super::catalogue::adoption_list_operations::apply_authored_adoption_catalogue_tab_initialization;
    use crate::plugins::ui::{
        authored_reusable_list_and_table_runtime_types::UiTablePolicy,
        authored_ui_activation_contracts::UiNodeActivated,
        authored_ui_node_projection_components::UiDocumentOwner,
        authored_ui_selection_state::UiSelected,
    };

    let mut app = App::new();
    app.add_message::<UiNodeActivated>()
        .add_systems(Update, apply_authored_adoption_catalogue_tab_initialization);
    app.world_mut().spawn((
        crate::plugins::world_spawn::world_membership_types::WorldRoot { scenario: id(1) },
        crate::plugins::world_spawn::selected_world_identity::SelectedWorldIdentity {
            requested: id(1),
            map: id(1),
            start: id(1),
            profile: id(1),
            mode,
        },
    ));
    let owner = app.world_mut().spawn_empty().id();
    app.world_mut()
        .spawn((UiTablePolicy::AdoptionList, UiDocumentOwner(owner)));
    let purchase = app
        .world_mut()
        .spawn((
            Name::new("Buy Animal Tab"),
            UiDocumentOwner(owner),
            Visibility::Inherited,
            UiSelected(true),
        ))
        .id();
    let adoption = app
        .world_mut()
        .spawn((
            Name::new("Adopt Animal Tab"),
            UiDocumentOwner(owner),
            Visibility::Hidden,
            UiSelected(false),
        ))
        .id();
    app.update();
    let actions = app
        .world_mut()
        .resource_mut::<Messages<UiNodeActivated>>()
        .drain()
        .map(|event| (event.node, event.trigger))
        .collect::<Vec<_>>();
    let freeform = mode == crate::game_session_types::WorldSessionMode::Freeform;
    assert!(actions.contains(&(
        purchase,
        if freeform {
            UiTrigger::On
        } else {
            UiTrigger::Off
        }
    )));
    assert!(actions.contains(&(
        adoption,
        if freeform {
            UiTrigger::Off
        } else {
            UiTrigger::On
        }
    )));
    assert_eq!(
        *app.world()
            .get::<Visibility>(purchase)
            .expect("purchase tab"),
        if freeform {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        }
    );
    app.update();
    assert!(app
        .world()
        .resource::<Messages<UiNodeActivated>>()
        .is_empty());
}
