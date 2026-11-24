use super::*;
use openzt2_game_assets::animation::animation_set_asset_types::AnimationTimelineCommand;
use openzt2_game_data::animation::animation_text_key::AuthoredAnimationTextAction;

#[test]
fn reverse_playback_reverses_timestamps_and_preserves_same_time_order() {
    let keys = [(0, "a"), (10, "b"), (10, "c"), (40, "d")].map(|(milliseconds, label)| {
        AnimationTimelineKey {
            milliseconds,
            commands: Box::new([AnimationTimelineCommand {
                joint: None,
                action: std::sync::Arc::new(
                    AuthoredAnimationTextAction::RequestAnimationGraphNode(label.to_owned()),
                ),
            }]),
        }
    });
    let interval = AnimationPlaybackInterval {
        from: 0.04,
        to: 0.0,
        cycle_start: 0.0,
        cycle_end: 0.04,
        cycles: 0,
        forward: false,
        clip_entry: false,
    };
    let mut visited = Vec::new();
    visit_interval(&interval, &keys, &mut |key| {
        if let AuthoredAnimationTextAction::RequestAnimationGraphNode(label) =
            &*key.commands[0].action
        {
            visited.push(label.clone());
        }
    });
    assert_eq!(visited, ["b", "c", "a"]);
}
