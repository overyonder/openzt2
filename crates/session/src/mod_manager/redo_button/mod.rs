use bevy::{
    asset::{embedded_asset, load_embedded_asset},
    prelude::*,
    ui::Val2,
};
use bevy::{
    picking::{events::PointerPress, hover::DirectlyHovered, pointer::PointerButton},
    ui::Pressed,
};
use openzt2_construction::construction::{
    construction_history::ConstructionHistory, construction_lifecycle::ConstructionActive,
};
use openzt2_game_assets::{
    application_lifecycle::GamePhase,
    ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset,
};
use openzt2_game_data::ui_document::action::construction::UiConstructionAction;
use openzt2_interface_state::{
    input::input_types::{ActionRequest, ActionSource, GameAction},
    ui::{
        authored_ui_action_projection_components::{UiActionSource, UiConstructionActions},
        authored_ui_interaction_enabled_state::UiInteractionEnabled,
        authored_ui_node_projection_components::{UiDocumentOwner, UiDocumentRoot},
        UiSet,
    },
};
use openzt2_world_state::application_schedule::GameSet;
use openzt2_zoo_state::construction::construction_edit_history_types::ConstructionEditDirection;

pub(super) struct RedoButtonPlugin;

#[derive(Resource)]
struct RedoButtonEnabled(bool);

#[derive(Component)]
pub(super) struct RedoButtonCheckbox;

#[derive(Component)]
#[relationship(relationship_target = RedoButtons)]
struct RedoButton(Entity);

#[derive(Component)]
#[relationship_target(relationship = RedoButton, linked_spawn)]
struct RedoButtons(Vec<Entity>);

impl Plugin for RedoButtonPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "button.png");
        app.insert_resource(RedoButtonEnabled(true))
            .add_systems(
                Update,
                toggle
                    .in_set(UiSet::Interaction)
                    .run_if(in_state(GamePhase::MainMenu)),
            )
            .add_systems(
                Update,
                (attach, update).chain().in_set(UiSet::DomainProjection),
            )
            .add_systems(
                Update,
                activate
                    .in_set(GameSet::Input)
                    .run_if(in_state(GamePhase::InGame)),
            );
    }
}

fn toggle(
    mut enabled: ResMut<RedoButtonEnabled>,
    mut presses: MessageReader<PointerPress>,
    mut checkboxes: Query<&mut Text, With<RedoButtonCheckbox>>,
) {
    for press in presses.read() {
        if !press.pointer.id.is_mouse() || press.button != PointerButton::Primary {
            continue;
        }
        let Ok(mut label) = checkboxes.get_mut(press.entity) else {
            continue;
        };
        enabled.0 = !enabled.0;
        label.0 = format!(
            "[{}] Redo button (built-in)",
            if enabled.0 { 'x' } else { ' ' }
        );
    }
}

fn attach(
    mut commands: Commands,
    assets: Res<AssetServer>,
    documents: Res<Assets<UiDocumentAsset>>,
    roots: Query<&UiDocumentRoot>,
    controls: Query<
        (
            Entity,
            &UiConstructionActions,
            &UiDocumentOwner,
            &ChildOf,
            &ZIndex,
        ),
        Added<UiConstructionActions>,
    >,
) {
    for (undo, actions, owner, parent, z) in &controls {
        if !matches!(actions.0, UiActionSource::Node(_)) {
            continue;
        }
        let Some(document) = roots
            .get(owner.0)
            .ok()
            .and_then(|root| documents.get(&root.document))
        else {
            continue;
        };
        if !actions
            .authored_action_records(document)
            .any(|record| matches!(record.action, UiConstructionAction::Undo))
        {
            continue;
        }
        commands.spawn((
            Name::new("Redo"),
            RedoButton(undo),
            ChildOf(parent.parent()),
            ZIndex(z.0 + 1),
            bevy::ui_widgets::Button,
            DirectlyHovered::default(),
            UiInteractionEnabled(false),
            Visibility::Hidden,
            ImageNode::new(load_embedded_asset!(&*assets, "button.png")),
            UiTransform {
                // Shrink toward the lower-left corner where Redo meets Undo.
                translation: Val2::percent(18.75, -35.75),
                scale: Vec2::splat(0.375),
                ..default()
            },
        ));
    }
}

fn update(
    enabled: Res<RedoButtonEnabled>,
    history: ConstructionHistory,
    pending_edits: Query<(), With<ConstructionActive>>,
    undo_controls: Query<(Ref<Node>, Ref<Visibility>, Ref<ZIndex>), Without<RedoButton>>,
    mut buttons: Query<(
        Entity,
        Ref<RedoButton>,
        Ref<DirectlyHovered>,
        Option<Ref<Pressed>>,
        &mut Node,
        &mut Visibility,
        &mut ImageNode,
        &mut UiInteractionEnabled,
        &mut ZIndex,
    )>,
    active_changed: Query<(), Added<ConstructionActive>>,
    changed_edits: Query<
        (),
        Changed<openzt2_zoo_state::construction::construction_transaction_types::EditTransaction>,
    >,
    mut finished_edits: RemovedComponents<ConstructionActive>,
    mut removed_edits: RemovedComponents<
        openzt2_zoo_state::construction::construction_transaction_types::EditTransaction,
    >,
    mut released: RemovedComponents<Pressed>,
) {
    let finished = finished_edits.read().count() != 0;
    let edits_removed = removed_edits.read().count() != 0;
    let availability_changed =
        finished || edits_removed || !active_changed.is_empty() || !changed_edits.is_empty();
    let released: bevy::ecs::entity::EntityHashSet = released.read().collect();
    let mut available = None;
    for (
        entity,
        button,
        hovered,
        pressed,
        mut node,
        mut visibility,
        mut image,
        mut interactive,
        mut stack,
    ) in &mut buttons
    {
        let Ok((undo_layout, undo_visibility, z)) = undo_controls.get(button.0) else {
            continue;
        };
        if (button.is_changed() || undo_layout.is_changed()) && *node != *undo_layout {
            (*node).clone_from(&undo_layout);
        }
        if button.is_changed() || z.is_changed() {
            stack.set_if_neq(ZIndex(z.0 + 1));
        }
        if button.is_changed() || enabled.is_changed() || undo_visibility.is_changed() {
            visibility.set_if_neq(if enabled.0 {
                *undo_visibility
            } else {
                Visibility::Hidden
            });
        }
        if !(button.is_changed()
            || enabled.is_changed()
            || availability_changed
            || hovered.is_changed()
            || pressed.as_ref().is_some_and(DetectChanges::is_added)
            || released.contains(&entity))
        {
            continue;
        }
        let available = *available.get_or_insert_with(|| {
            pending_edits.is_empty() && history.next(ConstructionEditDirection::Redo).is_some()
        });
        interactive.set_if_neq(UiInteractionEnabled(available && enabled.0));
        let x = match (available, hovered.get(), pressed.is_some()) {
            (false, _, _) => 64.0,
            (true, false, false) => 0.0,
            (true, true, false) => 128.0,
            (true, _, true) => 192.0,
        };
        let rect = Some(Rect::new(x, 0.0, x + 64.0, 64.0));
        if image.rect != rect {
            image.rect = rect;
        }
    }
}

fn activate(
    mut presses: MessageReader<PointerPress>,
    buttons: Query<&UiInteractionEnabled, With<RedoButton>>,
    mut actions: MessageWriter<ActionRequest>,
) {
    let pressed = presses.read().any(|press| {
        press.pointer.id.is_mouse()
            && press.button == PointerButton::Primary
            && buttons.get(press.entity).is_ok_and(|enabled| enabled.0)
    });
    presses.clear();
    if pressed {
        actions.write(ActionRequest {
            action: GameAction::Redo,
            source: ActionSource::System,
        });
    }
}
