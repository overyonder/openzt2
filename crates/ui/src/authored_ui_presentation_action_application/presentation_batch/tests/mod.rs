use super::*;
use bevy::input_focus::InputFocus;
use openzt2_game_assets::localization::{
    localization_asset_types::LocalizationAsset,
    localization_precedence_index::LocalizationPrecedenceIndex,
};
use openzt2_game_data::ui_document::document::UiDocumentRole;
use openzt2_interface_state::ui::{
    authored_ui_node_projection_components::{UiNodeId, UiSliderValueChanged},
    pointer_capture_types::UiPointerCapture,
    ui_document_lifecycle_contracts::AuthoredUiDocumentOperation,
};
use openzt2_zoo_state::audio::audio_playback_message_types::PlayAudioClip;

#[test]
fn options_message_of_the_day_hider_emits_show_then_hide_in_one_pass() {
    let mut app = App::new();
    app.init_resource::<Assets<UiDocumentAsset>>()
        .init_resource::<Assets<LocalizationAsset>>()
        .init_resource::<LocalizationPrecedenceIndex>()
        .init_resource::<bevy::ui::UiStack>()
        .init_resource::<InputFocus>()
        .init_resource::<UiPointerCapture>()
        .add_message::<UiNodeActivated>()
        .add_message::<UiSliderValueChanged>()
        .add_message::<RequestUiPresentationAction>()
        .add_message::<AuthoredUiDocumentOperation>()
        .add_message::<crate::animation::UiAnimationPlaybackRequest>()
        .add_message::<crate::authored_timed_action_sequence::UiSelectionWaitEnded>()
        .add_message::<TimedUiPresentationCommitted>()
        .add_message::<PlayAudioClip>()
        .add_systems(Update, apply_ordered_presentation_batches);

    let owner = app.world_mut().spawn_empty().id();
    let root = app
        .world_mut()
        .spawn((
            UiDocumentRoot {
                document: Handle::default(),
            },
            ChildOf(owner),
        ))
        .id();
    let target = UiDocumentRole::Options.node_id("MOTD hider");
    let hider = app
        .world_mut()
        .spawn((
            UiNodeId {
                index: 1,
                id: target,
            },
            UiDocumentOwner(root),
            Visibility::Hidden,
            ChildOf(root),
        ))
        .id();
    for visible in [true, false] {
        app.world_mut().write_message(RequestUiPresentationAction {
            document_root: root,
            action: UiPresentationAction::SetTargetNodeVisible {
                target_node: target,
                visible,
            },
        });
    }
    app.update();

    assert_eq!(
        app.world().get::<Visibility>(hider),
        Some(&Visibility::Hidden)
    );
    let transitions: Vec<_> = app
        .world_mut()
        .resource_mut::<Messages<UiNodeActivated>>()
        .drain()
        .map(|event| (event.node, event.trigger))
        .collect();
    assert_eq!(
        transitions,
        [(hider, UiTrigger::Show), (hider, UiTrigger::Hide)]
    );
}
