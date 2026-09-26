use bevy::prelude::*;

use crate::plugins::input::input_types::GameAction;

use super::input_types::{
    GameActionInputBindings, GameActionInputRebindingRejection, InputChord, RebindGameActionInput,
};

pub(super) fn apply_requested_game_action_input_rebindings(
    mut rebinding_requests: MessageReader<RebindGameActionInput>,
    mut action_bindings: ResMut<GameActionInputBindings>,
    mut rebinding_rejections: MessageWriter<GameActionInputRebindingRejection>,
) {
    for rebinding_request in rebinding_requests.read().copied() {
        if let Err(rejection) = rebind_game_action_input(&mut action_bindings, rebinding_request) {
            rebinding_rejections.write(rejection);
        }
    }
}

pub(super) fn rebind_game_action_input(
    action_bindings: &mut GameActionInputBindings,
    rebinding_request: RebindGameActionInput,
) -> Result<(), GameActionInputRebindingRejection> {
    if rebinding_request.slot > 1 {
        return Err(GameActionInputRebindingRejection::InvalidSlot);
    }

    let Some(target_binding_index) = action_bindings
        .entries
        .iter()
        .position(|binding| binding.action == rebinding_request.action)
    else {
        return Err(GameActionInputRebindingRejection::InvalidSlot);
    };

    let current_chord = if rebinding_request.slot == 0 {
        Some(action_bindings.entries[target_binding_index].primary)
    } else {
        action_bindings.entries[target_binding_index].alternate
    };
    if current_chord == Some(rebinding_request.chord) {
        return Ok(());
    }

    if let Some(conflicting_binding) = action_bindings.entries.iter().find(|binding| {
        game_action_input_bindings_conflict(
            binding.action,
            binding.primary,
            rebinding_request.action,
            rebinding_request.chord,
        ) || binding.alternate.is_some_and(|alternate_chord| {
            game_action_input_bindings_conflict(
                binding.action,
                alternate_chord,
                rebinding_request.action,
                rebinding_request.chord,
            )
        })
    }) {
        return Err(GameActionInputRebindingRejection::Duplicate {
            conflicting_action: conflicting_binding.action,
        });
    }

    if rebinding_request.slot == 0 {
        action_bindings.entries[target_binding_index].primary = rebinding_request.chord;
    } else {
        action_bindings.entries[target_binding_index].alternate = Some(rebinding_request.chord);
    }
    Ok(())
}

/// Reports whether two action bindings collide in a context where both actions
/// can fire. Escape is deliberately shared by the original cancellation/menu
/// flow: focused UI consumes `Cancel`, while the otherwise-idle in-game shell
/// consumes `OpenMenu`.
pub(crate) fn game_action_input_bindings_conflict(
    left_action: GameAction,
    left_chord: InputChord,
    right_action: GameAction,
    right_chord: InputChord,
) -> bool {
    use GameAction::{Cancel, OpenMenu};

    !matches!(
        (left_action, right_action),
        (Cancel, OpenMenu) | (OpenMenu, Cancel)
    ) && input_chords_overlap(left_chord, right_chord)
}

pub(super) fn input_chords_overlap(left_chord: InputChord, right_chord: InputChord) -> bool {
    match (left_chord, right_chord) {
        (InputChord::Unbound, _) | (_, InputChord::Unbound) => false,
        (InputChord::Key(left_key), InputChord::Key(right_key)) => left_key == right_key,
        (InputChord::Key(key), InputChord::KeyPair(first_key, second_key))
        | (InputChord::KeyPair(first_key, second_key), InputChord::Key(key)) => {
            key == first_key || key == second_key
        }
        (
            InputChord::KeyPair(left_first_key, left_second_key),
            InputChord::KeyPair(right_first_key, right_second_key),
        ) => {
            left_first_key == right_first_key
                || left_first_key == right_second_key
                || left_second_key == right_first_key
                || left_second_key == right_second_key
        }
        (
            InputChord::ModifiedKey {
                key: left_key,
                shift: left_shift,
                control: left_control,
                alt: left_alt,
            },
            InputChord::ModifiedKey {
                key: right_key,
                shift: right_shift,
                control: right_control,
                alt: right_alt,
            },
        ) => {
            left_key == right_key
                && left_shift == right_shift
                && left_control == right_control
                && left_alt == right_alt
        }
        (
            InputChord::Key(key),
            InputChord::ModifiedKey {
                key: modified_key, ..
            },
        )
        | (
            InputChord::ModifiedKey {
                key: modified_key, ..
            },
            InputChord::Key(key),
        ) => key == modified_key,
        (
            InputChord::KeyPair(first_key, second_key),
            InputChord::ModifiedKey {
                key: modified_key, ..
            },
        )
        | (
            InputChord::ModifiedKey {
                key: modified_key, ..
            },
            InputChord::KeyPair(first_key, second_key),
        ) => modified_key == first_key || modified_key == second_key,
        (InputChord::Mouse(left_button), InputChord::Mouse(right_button)) => {
            left_button == right_button
        }
        (InputChord::Gamepad(left_button), InputChord::Gamepad(right_button)) => {
            left_button == right_button
        }
        (
            InputChord::Key(_)
            | InputChord::KeyPair(_, _)
            | InputChord::ModifiedKey { .. }
            | InputChord::Mouse(_),
            InputChord::Gamepad(_),
        )
        | (
            InputChord::Gamepad(_),
            InputChord::Key(_)
            | InputChord::KeyPair(_, _)
            | InputChord::ModifiedKey { .. }
            | InputChord::Mouse(_),
        )
        | (
            InputChord::Key(_) | InputChord::KeyPair(_, _) | InputChord::ModifiedKey { .. },
            InputChord::Mouse(_),
        )
        | (
            InputChord::Mouse(_),
            InputChord::Key(_) | InputChord::KeyPair(_, _) | InputChord::ModifiedKey { .. },
        ) => false,
    }
}
