use super::authored_animation_event_time_was_crossed_by_playback;

#[test]
fn first_sample_covers_elapsed_prefix_without_treating_restart_as_wrap() {
    let keys = [0, 10, 30, 90];
    let crossed: Vec<_> = keys
        .into_iter()
        .filter(|time| {
            authored_animation_event_time_was_crossed_by_playback(*time, 20, 80, true, true, false)
        })
        .collect();
    assert_eq!(crossed, [0, 10]);
    let next: Vec<_> = keys
        .into_iter()
        .filter(|time| {
            authored_animation_event_time_was_crossed_by_playback(*time, 40, 20, false, true, false)
        })
        .collect();
    assert_eq!(next, [30]);
    let reversed: Vec<_> = keys
        .into_iter()
        .filter(|time| {
            authored_animation_event_time_was_crossed_by_playback(*time, 80, 20, true, false, false)
        })
        .collect();
    assert_eq!(reversed, [90]);
}
