use bevy::prelude::*;
use openzt2_game_data::ui_document::action::UiTrigger;

use super::authored_button_runtime_policy::UiButtonPolicy;
use super::authored_ui_change_activation_dispatch::UiPreviousSelection;
use super::authored_ui_selection_binding::UiSelectedBinding;
use super::authored_ui_selection_state::UiSelected;
use crate::plugins::ui::authored_ui_activation_contracts::UiNodeActivated;

/// Runtime policy retained by an authored toggle group after projection.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UiAuthoredToggleGroupSelectionPolicy {
    allow_selected_member_repress: bool,
}

impl UiAuthoredToggleGroupSelectionPolicy {
    pub(super) const fn new(allow_selected_member_repress: bool) -> Self {
        Self {
            allow_selected_member_repress,
        }
    }
}

/// Projects an authored toggle-set press into the presentation-only selected
/// facts of its direct children.
///
/// A selected binding marks the bound domain as authoritative. Those members
/// still emit their ordinary typed activation, but UI never writes their
/// selected fact speculatively.
pub(super) fn apply_authored_toggle_group_selection_transitions_from_presses(
    mut commands: Commands,
    mut activated: MessageReader<UiNodeActivated>,
    parents: Query<&ChildOf>,
    groups: Query<&UiAuthoredToggleGroupSelectionPolicy>,
    children: Query<&Children>,
    mut toggles: Query<(
        &mut UiSelected,
        Has<UiSelectedBinding>,
        Option<&mut UiPreviousSelection>,
    )>,
) {
    for activation in activated.read() {
        if activation.trigger != UiTrigger::Press {
            continue;
        }
        let Ok(parent) = parents.get(activation.node) else {
            continue;
        };
        let group_entity = parent.parent();
        let Ok(policy) = groups.get(group_entity) else {
            continue;
        };
        let Ok((selected, bound, _)) = toggles.get(activation.node) else {
            continue;
        };
        if bound {
            continue;
        }
        if selected.0 && !policy.allow_selected_member_repress {
            continue;
        }
        let Ok(members) = children.get(group_entity) else {
            continue;
        };
        // Tabs can share presentation containers. Hide the losing tab before
        // the winning tab shows them, regardless of their authored child order.
        for member in members
            .iter()
            .filter(|member| *member != activation.node)
            .chain(std::iter::once(activation.node))
        {
            let Ok((mut selected, bound, previous)) = toggles.get_mut(member) else {
                continue;
            };
            let next = member == activation.node;
            let repressed =
                member == activation.node && selected.0 && policy.allow_selected_member_repress;
            let transitioned = selected.0 != next || repressed;
            if transitioned && (!bound || member == activation.node) {
                if !bound {
                    selected.0 = next;
                    // This owner emits the transition below. Do not let the
                    // generic change detector emit it again in query order.
                    if let Some(mut previous) = previous {
                        *previous = UiPreviousSelection::from_current_selection(next);
                    }
                }
                let transition = UiNodeActivated {
                    node: member,
                    source: activation.source,
                    trigger: if next { UiTrigger::On } else { UiTrigger::Off },
                };
                commands.queue(move |world: &mut World| {
                    world.write_message(transition);
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::input::input_types::ActionSource;

    #[test]
    fn switching_to_an_earlier_tab_deactivates_the_old_tab_before_activating_the_new_one() {
        let mut app = App::new();
        app.add_message::<UiNodeActivated>().add_systems(Update, (
            apply_authored_toggle_group_selection_transitions_from_presses,
            super::super::authored_ui_change_activation_dispatch::dispatch_authored_selection_change_activations,
        ).chain());
        let group = app
            .world_mut()
            .spawn(UiAuthoredToggleGroupSelectionPolicy::new(false))
            .id();
        let construction = app
            .world_mut()
            .spawn((
                UiSelected(false),
                UiPreviousSelection::from_current_selection(false),
                ChildOf(group),
            ))
            .id();
        let staff = app
            .world_mut()
            .spawn((
                UiSelected(true),
                UiPreviousSelection::from_current_selection(true),
                ChildOf(group),
            ))
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

/// Applies the authored standalone-toggle transition to its visual selected
/// fact. A non-sticky toggle always changes state when pressed; `repress`
/// controls whether an already-selected sticky button may run its activation
/// again. A non-sticky toggle can always turn off.
pub(super) fn apply_authored_standalone_toggle_selection_transitions_from_presses(
    mut commands: Commands,
    mut activated: MessageReader<UiNodeActivated>,
    parents: Query<&ChildOf>,
    groups: Query<(), With<UiAuthoredToggleGroupSelectionPolicy>>,
    mut buttons: Query<(
        &UiButtonPolicy,
        &mut UiSelected,
        Has<UiSelectedBinding>,
        Option<&mut UiPreviousSelection>,
    )>,
) {
    for activation in activated.read() {
        if activation.trigger != UiTrigger::Press {
            continue;
        }
        let Ok((policy, mut selected, bound, previous)) = buttons.get_mut(activation.node) else {
            continue;
        };
        if !policy.selectable
            || parents
                .get(activation.node)
                .is_ok_and(|parent| groups.contains(parent.parent()))
        {
            continue;
        }
        let next = policy.sticky || !selected.0;
        if selected.0 != next || (selected.0 && policy.repress) {
            if !bound {
                selected.0 = next;
                if let Some(mut previous) = previous {
                    *previous = UiPreviousSelection::from_current_selection(next);
                }
            }
            let transition = UiNodeActivated {
                node: activation.node,
                source: activation.source,
                trigger: if next { UiTrigger::On } else { UiTrigger::Off },
            };
            commands.queue(move |world: &mut World| {
                world.write_message(transition);
            });
        }
    }
}
