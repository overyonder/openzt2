use bevy::{
    asset::{embedded_asset, load_embedded_asset},
    prelude::*,
    ui::Val2,
};
use openzt2_game_assets::{
    application_lifecycle::GamePhase,
    ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset,
};
use openzt2_game_data::ui_document::action::construction::UiConstructionAction;
use openzt2_interface_state::ui::pointer_capture_types::UiPointerInteraction;
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
use openzt2_zoo_state::construction::construction_edit_history_types::ConstructionEditHistory;

pub(super) struct RedoButtonPlugin;

#[derive(Resource)]
struct RedoButtonEnabled(bool);

#[derive(Component)]
pub(super) struct RedoButtonCheckbox;

#[derive(Component)]
struct RedoButton {
    undo: Entity,
}

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
    mut checkboxes: Query<
        (&UiPointerInteraction, &mut Text),
        (With<RedoButtonCheckbox>, Changed<UiPointerInteraction>),
    >,
) {
    for (interaction, mut label) in &mut checkboxes {
        if *interaction == UiPointerInteraction::Pressed {
            enabled.0 = !enabled.0;
            label.0 = format!(
                "[{}] Redo button (built-in)",
                if enabled.0 { 'x' } else { ' ' }
            );
        }
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
            RedoButton { undo },
            ChildOf(parent.parent()),
            ZIndex(z.0 + 1),
            bevy::ui_widgets::Button,
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
    mut commands: Commands,
    enabled: Res<RedoButtonEnabled>,
    history: Res<ConstructionEditHistory>,
    undo_controls: Query<(&Node, &Visibility), Without<RedoButton>>,
    mut buttons: Query<(
        Entity,
        &RedoButton,
        &UiPointerInteraction,
        &mut Node,
        &mut Visibility,
        &mut ImageNode,
        &mut UiInteractionEnabled,
    )>,
) {
    let available = history.entries.get(history.cursor).is_some();
    for (entity, button, interaction, mut node, mut visibility, mut image, mut interactive) in
        &mut buttons
    {
        let Ok((undo_layout, undo_visibility)) = undo_controls.get(button.undo) else {
            commands.entity(entity).despawn();
            continue;
        };
        if *node != *undo_layout {
            *node = undo_layout.clone();
        }
        visibility.set_if_neq(if enabled.0 {
            *undo_visibility
        } else {
            Visibility::Hidden
        });
        interactive.set_if_neq(UiInteractionEnabled(available && enabled.0));
        let x = match (available, interaction) {
            (false, _) => 64.0,
            (true, UiPointerInteraction::None) => 0.0,
            (true, UiPointerInteraction::Hovered) => 128.0,
            (true, UiPointerInteraction::Pressed) => 192.0,
        };
        let rect = Some(Rect::new(x, 0.0, x + 64.0, 64.0));
        if image.rect != rect {
            image.rect = rect;
        }
    }
}

fn activate(
    buttons: Query<
        (&UiPointerInteraction, &UiInteractionEnabled),
        (With<RedoButton>, Changed<UiPointerInteraction>),
    >,
    mut actions: MessageWriter<ActionRequest>,
) {
    if buttons
        .iter()
        .any(|(interaction, enabled)| enabled.0 && *interaction == UiPointerInteraction::Pressed)
    {
        actions.write(ActionRequest {
            action: GameAction::Redo,
            source: ActionSource::System,
        });
    }
}
