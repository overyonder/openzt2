use bevy::prelude::*;
use openzt2_game_data::ui_document::action::UiTrigger;
use openzt2_game_data::ui_document::node_property_binding::UiBooleanPropertyBindingSource;

use super::authored_button_runtime_policy::UiButtonPolicy;
use super::authored_ui_selection_binding::UiSelectedBinding;
use super::authored_ui_selection_state::UiSelected;
use openzt2_interface_state::ui::authored_ui_activation_contracts::UiNodeActivated;

/// Runtime policy retained by an authored toggle group after projection.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiAuthoredToggleGroupSelectionPolicy {
    allow_selected_member_repress: bool,
}

impl UiAuthoredToggleGroupSelectionPolicy {
    pub(super) const fn new(allow_selected_member_repress: bool) -> Self {
        Self {
            allow_selected_member_repress,
        }
    }
}

/// Converts an authored Press to an ordered selection transition.
pub(super) fn transition_pressed_authored_toggles(
    mut messages: ParamSet<(
        MessageReader<UiNodeActivated>,
        MessageWriter<UiNodeActivated>,
    )>,
    parents: Query<&ChildOf>,
    children: Query<&Children>,
    groups: Query<&UiAuthoredToggleGroupSelectionPolicy>,
    mut toggles: Query<(
        &mut UiSelected,
        Option<&UiButtonPolicy>,
        Option<&UiSelectedBinding>,
    )>,
) {
    let mut output = Vec::new();
    for activation in messages
        .p0()
        .read()
        .filter(|event| event.trigger == UiTrigger::Press)
    {
        let Ok((selected, button, binding)) = toggles.get(activation.node) else {
            continue;
        };
        let mut emit = |node, selected| {
            output.push(UiNodeActivated {
                node,
                source: activation.source,
                trigger: if selected {
                    UiTrigger::On
                } else {
                    UiTrigger::Off
                },
            })
        };
        let group = parents.get(activation.node).ok().and_then(|parent| {
            Some((
                groups.get(parent.parent()).ok()?,
                children.get(parent.parent()).ok()?,
            ))
        });
        if let Some((policy, group)) = group {
            if selected.0 && !policy.allow_selected_member_repress {
                continue;
            }
            if let Some(binding) = binding {
                if matches!(
                    binding.0,
                    UiBooleanPropertyBindingSource::OverheadViewSelected
                        | UiBooleanPropertyBindingSource::FirstPersonViewSelected
                        | UiBooleanPropertyBindingSource::PhotoViewSelected
                ) {
                    emit(activation.node, true);
                }
                continue;
            }
            for member in group
                .iter()
                .filter(|node| *node != activation.node)
                .chain(std::iter::once(activation.node))
            {
                let Ok((mut selected, _, binding)) = toggles.get_mut(member) else {
                    continue;
                };
                let next = member == activation.node;
                if (selected.0 != next || next && policy.allow_selected_member_repress)
                    && (binding.is_none() || next)
                {
                    if binding.is_none() {
                        selected.set_if_neq(UiSelected(next));
                    }
                    emit(member, next);
                }
            }
        } else if let Some(button) = button.filter(|button| button.selectable) {
            let next = button.sticky || !selected.0;
            if next != selected.0 || selected.0 && button.repress {
                if binding.is_none() {
                    if let Ok((mut selected, ..)) = toggles.get_mut(activation.node) {
                        selected.set_if_neq(UiSelected(next));
                    }
                }
                emit(activation.node, next);
            }
        }
    }
    let _ = messages.p1().write_batch(output);
}

#[cfg(test)]
mod tests {
    use super::*;
    use openzt2_interface_state::input::input_types::ActionSource;

    #[test]
    fn switching_to_an_earlier_tab_deactivates_the_old_tab_before_activating_the_new_one() {
        let mut app = App::new();
        app.add_message::<UiNodeActivated>()
            .add_systems(Update, transition_pressed_authored_toggles);
        let group = app
            .world_mut()
            .spawn(UiAuthoredToggleGroupSelectionPolicy::new(false))
            .id();
        let construction = app
            .world_mut()
            .spawn((UiSelected(false), ChildOf(group)))
            .id();
        let staff = app
            .world_mut()
            .spawn((UiSelected(true), ChildOf(group)))
            .id();
        app.world_mut().write_message(UiNodeActivated {
            node: construction,
            source: ActionSource::KeyboardMouse,
            trigger: UiTrigger::Press,
        });
        app.update();
        let transitions = app
            .world_mut()
            .resource_mut::<Messages<UiNodeActivated>>()
            .drain()
            .filter(|event| event.trigger != UiTrigger::Press)
            .map(|event| (event.node, event.trigger))
            .collect::<Vec<_>>();
        assert_eq!(
            transitions,
            vec![(staff, UiTrigger::Off), (construction, UiTrigger::On)]
        );
    }
}
