use bevy::prelude::*;
use openzt2_game_data::ui_document::action::UiTrigger;
use openzt2_game_data::AssetId;

use openzt2_interface_state::information::entity_selection_types::Inspectable;
use openzt2_interface_state::information::entity_selection_types::SelectionRequest;

use openzt2_interface_state::input::input_types::ActionSource;

fn id(byte: u8) -> AssetId {
    AssetId([byte; 16])
}

#[test]
fn nested_information_row_uses_its_own_subject_until_unbound() {
    use crate::projection::information_list_row_value_projection::project_live_world_subject_values_to_information_list_rows;
    use openzt2_game_data::ui_document::node_property_binding::UiIntegerPropertyBindingSource;
    use openzt2_interface_state::information::entity_selection_types::InformationEntitySource;
    use openzt2_interface_state::ui::authored_reusable_list_and_table_runtime_types::UiListRow;
    use openzt2_interface_state::ui::authored_ui_node_projection_components::{
        UiValue, UiValueBinding,
    };
    use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
    use openzt2_zoo_state::guests::guest_simulation_types::GuestSatisfaction;

    let mut app = App::new();
    app.insert_resource(ZooClock {
        tick: 0,
        fractional_tick: default(),
        absolute_day: 0,
        tick_in_day: 0,
    })
    .add_systems(
        Update,
        project_live_world_subject_values_to_information_list_rows,
    );
    let world = app.world_mut();
    let list = world.spawn_empty().id();
    let happy = world.spawn(GuestSatisfaction(800, 0)).id();
    let unhappy = world.spawn(GuestSatisfaction(100, 0)).id();
    let row = world
        .spawn((
            UiListRow { index: 0 },
            ChildOf(list),
            InformationEntitySource(happy),
        ))
        .id();
    let nested = world
        .spawn((
            ChildOf(row),
            UiListRow { index: 0 },
            InformationEntitySource(unhappy),
        ))
        .id();
    let binding = UiValueBinding(UiIntegerPropertyBindingSource::InformationListGuestHappinessIcon);
    let unrelated = world.spawn((binding.clone(), UiValue(-1))).id();
    let value = world
        .spawn((ChildOf(nested), binding, UiValue(-1), Text::default()))
        .id();
    app.update();
    assert_eq!(app.world().get::<UiValue>(value), Some(&UiValue(0)));
    assert_eq!(app.world().get::<Text>(value).unwrap().0, "0");
    app.world_mut()
        .entity_mut(nested)
        .remove::<InformationEntitySource>();
    app.update();
    assert_eq!(app.world().get::<UiValue>(value), Some(&UiValue(1)));
    assert_eq!(app.world().get::<Text>(value).unwrap().0, "1");
    assert_eq!(app.world().get::<UiValue>(unrelated), Some(&UiValue(-1)));
}

#[test]
fn reopening_editor_rebinds_rows_after_hidden_membership_changes() {
    use crate::information_view_types::EntityEditorDataRootSurface;
    use crate::projection::entity_editor_data_root_projection::*;
    use openzt2_interface_state::information::entity_selection_types::InformationEntitySource;
    use openzt2_interface_state::ui::authored_reusable_list_and_table_runtime_types::UiListRow;
    use openzt2_world_state::world_spawn::world_membership_types::{WorldMember, WorldRoot};

    let mut app = App::new();
    app.add_observer(unlink_retired_inspectable_world_subjects)
        .add_systems(
            Update,
            (
                link_inspectable_world_subjects,
                project_inspectable_entities_to_visible_entity_editor_data_roots,
            )
                .chain(),
        );
    let world = app.world_mut();
    let root = world.spawn(WorldRoot { scenario: id(0) }).id();
    let editor = world
        .spawn((EntityEditorDataRootSurface, InheritedVisibility::VISIBLE))
        .id();
    let row = world.spawn((UiListRow { index: 0 }, ChildOf(editor))).id();
    let original = world
        .spawn((Inspectable { definition: id(1) }, WorldMember { root }))
        .id();
    app.update();
    assert_eq!(
        app.world().get::<InformationEntitySource>(row).unwrap().0,
        original
    );
    let world = app.world_mut();
    world.entity_mut(editor).insert(InheritedVisibility::HIDDEN);
    world.entity_mut(original).remove::<Inspectable>();
    let replacement = world
        .spawn((Inspectable { definition: id(2) }, WorldMember { root }))
        .id();
    app.update();
    app.world_mut()
        .entity_mut(editor)
        .insert(InheritedVisibility::VISIBLE);
    app.update();
    assert_eq!(
        app.world().get::<InformationEntitySource>(row).unwrap().0,
        replacement
    );
}

#[test]
fn authored_selection_action_clears_without_subject_and_selects_row_ancestor() {
    use bevy::ecs::system::SystemState;
    use openzt2_game_data::ui_document::action::information::UiInformationAction;

    use openzt2_interface_state::information::entity_selection_types::InformationEntitySource;

    use super::{
        information_selection_action_operations::apply_authored_information_selection_action,
        information_selection_action_types::InformationSelectionActionTargets,
    };
    use openzt2_interface_state::camera::camera_control_message_types::SetCameraMode;

    let mut app = App::new();
    app.add_message::<SetCameraMode>()
        .add_message::<SelectionRequest>();
    let subject = app
        .world_mut()
        .spawn(Inspectable { definition: id(1) })
        .id();
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
        let requests = app.world_mut().resource_mut::<Messages<SelectionRequest>>();
        let actual: Vec<_> = requests
            .into_inner()
            .drain()
            .map(|request| request.entity)
            .collect();
        assert_eq!(actual, [expected]);
    }
}

#[test]
fn adoption_initialization_dispatches_purchase_off_and_adoption_on_once() {
    assert_animal_catalogue_mode_initialization(
        openzt2_world_state::game_session_types::WorldSessionMode::Challenge,
    );
    assert_animal_catalogue_mode_initialization(
        openzt2_world_state::game_session_types::WorldSessionMode::Campaign,
    );
}

#[test]
fn freeform_initialization_uses_purchase_catalogue_instead_of_limited_offers() {
    assert_animal_catalogue_mode_initialization(
        openzt2_world_state::game_session_types::WorldSessionMode::Freeform,
    );
}

fn assert_animal_catalogue_mode_initialization(
    mode: openzt2_world_state::game_session_types::WorldSessionMode,
) {
    use super::catalogue::adoption_list_operations::apply_authored_adoption_catalogue_tab_initialization;
    use openzt2_interface_state::ui::authored_reusable_list_and_table_runtime_types::UiTablePolicy;
    use openzt2_interface_state::ui::authored_ui_activation_contracts::UiNodeActivated;
    use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentOwner;
    use openzt2_interface_state::ui::authored_ui_selection_state::UiSelected;

    let mut app = App::new();
    app.add_message::<UiNodeActivated>()
        .add_systems(Update, apply_authored_adoption_catalogue_tab_initialization);
    app.world_mut().spawn((
        openzt2_world_state::world_spawn::world_membership_types::WorldRoot { scenario: id(1) },
        openzt2_world_state::world_spawn::selected_world_identity::SelectedWorldIdentity {
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
    let freeform = mode == openzt2_world_state::game_session_types::WorldSessionMode::Freeform;
    // The selected tab's On arms placement through its type list, so world
    // entry only runs the losing tab's Off; opening the panel runs the On.
    assert_eq!(
        actions,
        [(if freeform { adoption } else { purchase }, UiTrigger::Off)]
    );
    assert_eq!(
        app.world()
            .get::<UiSelected>(purchase)
            .expect("purchase tab")
            .0,
        freeform
    );
    assert_eq!(
        app.world()
            .get::<UiSelected>(adoption)
            .expect("adoption tab")
            .0,
        !freeform
    );
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
