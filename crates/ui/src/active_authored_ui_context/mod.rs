//! Read-only input scope derived from the projected Bevy UI hierarchy.
use openzt2_interface_state::ui::input_frame_capture_types::AuthoredModalInputCapture;
use openzt2_interface_state::ui::input_frame_capture_types::OverheadModeInputCapture;

use bevy::input_focus::InputFocus;
use bevy::{ecs::system::SystemParam, prelude::*};

use super::{
    authored_modal_presentation::UiAuthoredModalPresentation,
    authored_reusable_list_and_table_runtime_types::UiListRow,
    authored_ui_focus_state::UiFocusable,
    authored_ui_interaction_enabled_state::UiInteractionEnabled,
    authored_ui_node_projection_components::UiDocumentOwner,
};

#[derive(Resource, Default, PartialEq, Eq)]
pub(super) struct AuthoredFocusScope {
    pub(super) modal: Option<Entity>,
    pub(super) document: Option<Entity>,
    pub(super) focused_control_is_eligible: bool,
}

#[allow(clippy::type_complexity)]
pub(super) fn resolve_authored_focus_scope(
    context: ActiveAuthoredUiContext,
    focus: Res<InputFocus>,
    changed: Query<
        (),
        (
            With<Node>,
            Or<(
                Changed<bevy::ui::ComputedStackIndex>,
                Changed<InheritedVisibility>,
                Changed<UiInteractionEnabled>,
                Changed<UiAuthoredModalPresentation>,
                Changed<UiDocumentOwner>,
                Changed<ChildOf>,
                Changed<UiListRow>,
                Added<UiFocusable>,
            )>,
        ),
    >,
    changed_layouts: Query<
        (),
        (
            With<super::authored_ui_layout_participation::UiAuthoredLayoutDisplay>,
            Changed<Node>,
        ),
    >,
    (
        mut removed_controls,
        mut removed_modals,
        mut removed_enabled,
        mut removed_parent,
        mut removed_stack,
    ): (
        RemovedComponents<UiFocusable>,
        RemovedComponents<UiAuthoredModalPresentation>,
        RemovedComponents<UiInteractionEnabled>,
        RemovedComponents<ChildOf>,
        RemovedComponents<bevy::ui::ComputedStackIndex>,
    ),
    mut resolved: ResMut<AuthoredFocusScope>,
    ui_nodes: Query<(), With<Node>>,
) {
    let hierarchy_detached = removed_parent
        .read()
        .any(|entity| ui_nodes.contains(entity));
    let removed = !removed_controls.is_empty()
        || !removed_modals.is_empty()
        || !removed_enabled.is_empty()
        || hierarchy_detached
        || !removed_stack.is_empty();
    removed_controls.clear();
    removed_modals.clear();
    removed_enabled.clear();
    removed_parent.clear();
    removed_stack.clear();
    let (modal, document) = if removed || !changed.is_empty() || !changed_layouts.is_empty() {
        let modal = context.top_modal();
        (modal, context.active_scope_with_modal(modal))
    } else if focus.is_changed() {
        (resolved.modal, resolved.document)
    } else {
        return;
    };
    let focused_control_is_eligible = focus.get().is_some_and(|focused| {
        document.is_some_and(|document| context.eligible_in_scope(focused, document, modal))
    });
    resolved.set_if_neq(AuthoredFocusScope {
        modal,
        document,
        focused_control_is_eligible,
    });
}

#[allow(clippy::type_complexity)]
pub(super) fn project_native_widget_interaction_from_authored_enablement(
    mut commands: Commands,
    context: ActiveAuthoredUiContext,
    changed_ancestors: Query<
        Entity,
        (
            With<Node>,
            Or<(Changed<UiInteractionEnabled>, Changed<ChildOf>)>,
        ),
    >,
    mut removed_enabled: RemovedComponents<UiInteractionEnabled>,
    mut detached: RemovedComponents<ChildOf>,
    children: Query<&Children>,
    new_widgets: Query<
        Entity,
        (
            With<UiInteractionEnabled>,
            Or<(
                Added<bevy::ui_widgets::Button>,
                Added<bevy::ui_widgets::Slider>,
                Added<bevy::ui_widgets::TextInput>,
            )>,
        ),
    >,
    widgets: Query<
        Has<bevy::ui::InteractionDisabled>,
        (
            With<UiInteractionEnabled>,
            Or<(
                With<bevy::ui_widgets::Button>,
                With<bevy::ui_widgets::Slider>,
                With<bevy::ui_widgets::TextInput>,
            )>,
        ),
    >,
) {
    let mut dirty = bevy::ecs::entity::EntityHashSet::default();
    dirty.extend(new_widgets.iter());
    for ancestor in changed_ancestors
        .iter()
        .chain(removed_enabled.read())
        .chain(detached.read())
    {
        if widgets.contains(ancestor) {
            dirty.insert(ancestor);
        }
        dirty.extend(
            children
                .iter_descendants::<Children>(ancestor)
                .filter(|node| widgets.contains(*node)),
        );
    }
    for node in dirty {
        let Ok(disabled) = widgets.get(node) else {
            continue;
        };
        match (disabled, context.enabled(node)) {
            (true, true) => {
                commands
                    .entity(node)
                    .remove::<bevy::ui::InteractionDisabled>();
            }
            (false, false) => {
                commands.entity(node).insert(bevy::ui::InteractionDisabled);
            }
            _ => {}
        }
    }
}

#[derive(SystemParam)]
pub struct ActiveAuthoredUiContext<'w, 's> {
    stack: Res<'w, bevy::ui::UiStack>,
    parents: Query<'w, 's, &'static ChildOf>,
    owners: Query<'w, 's, &'static UiDocumentOwner>,
    rows: Query<'w, 's, &'static UiListRow>,
    enabled: Query<'w, 's, &'static UiInteractionEnabled>,
    focusables: Query<'w, 's, &'static UiFocusable>,
    visibility: Query<'w, 's, &'static InheritedVisibility>,
    layouts: Query<'w, 's, &'static Node>,
    modals: Query<'w, 's, (Entity, &'static UiAuthoredModalPresentation)>,
    stack_indices: Query<'w, 's, &'static bevy::ui::ComputedStackIndex>,
}

impl ActiveAuthoredUiContext<'_, '_> {
    fn ancestors(&self, entity: Entity) -> impl Iterator<Item = Entity> + '_ {
        std::iter::successors(Some(entity), |entity| {
            self.parents.get(*entity).ok().map(ChildOf::parent)
        })
    }

    pub fn visible_and_enabled(&self, entity: Entity) -> bool {
        self.visibility
            .get(entity)
            .is_ok_and(|visible| visible.get())
            && self.ancestors(entity).all(|node| {
                !self.enabled.get(node).is_ok_and(|enabled| !enabled.0)
                    && !self
                        .layouts
                        .get(node)
                        .is_ok_and(|node| node.display == Display::None)
            })
    }

    /// No disabled node on the path to the root, whether or not it is shown.
    pub fn enabled(&self, entity: Entity) -> bool {
        self.ancestors(entity)
            .all(|ancestor| !self.enabled.get(ancestor).is_ok_and(|enabled| !enabled.0))
    }

    pub fn top_modal(&self) -> Option<Entity> {
        self.modals
            .iter()
            .filter(|(entity, modal)| modal.is_modal() && self.visible_and_enabled(*entity))
            .filter_map(|(entity, _)| Some((self.stack_position(entity)?, entity)))
            .max_by_key(|(position, _)| *position)
            .map(|(_, entity)| entity)
    }

    /// A reusable row is a document for assets, but participates in its list's
    /// input scope. Follow the actual row/list relation instead of retaining a
    /// second focus tree or treating each template instance as a modal.
    pub fn document_scope(&self, mut owner: Entity) -> Entity {
        while let Some(list_owner) = self.ancestors(owner).find_map(|ancestor| {
            self.rows
                .get(ancestor)
                .ok()
                .and_then(|_| self.parents.get(ancestor).ok())
                .and_then(|parent| self.owners.get(parent.parent()).ok())
                .map(|owner| owner.0)
        }) {
            if list_owner == owner {
                break;
            }
            owner = list_owner;
        }
        owner
    }

    pub(super) fn active_scope_with_modal(&self, modal: Option<Entity>) -> Option<Entity> {
        self.stack.uinodes.iter().rev().find_map(|entity| {
            (self.focusables.contains(*entity)
                && self.visible_and_enabled(*entity)
                && modal.is_none_or(|modal| self.ancestors(*entity).any(|node| node == modal)))
            .then(|| {
                self.owners
                    .get(*entity)
                    .ok()
                    .map(|owner| self.document_scope(owner.0))
            })
            .flatten()
        })
    }

    pub fn eligible_in_scope(&self, entity: Entity, scope: Entity, modal: Option<Entity>) -> bool {
        self.visible_and_enabled(entity)
            && self
                .owners
                .get(entity)
                .is_ok_and(|owner| self.document_scope(owner.0) == scope)
            && modal.is_none_or(|modal| self.ancestors(entity).any(|node| node == modal))
    }

    pub fn is_in_modal(&self, entity: Entity, modal: Entity) -> bool {
        self.ancestors(entity).any(|ancestor| ancestor == modal)
    }

    fn stack_position(&self, entity: Entity) -> Option<usize> {
        if let Ok(index) = self.stack_indices.get(entity) {
            let position = index.0 as usize;
            return (self.stack.uinodes.get(position) == Some(&entity)).then_some(position);
        }
        self.stack.uinodes.iter().position(|node| *node == entity)
    }

    pub fn navigation_order(&self, entity: Entity) -> u32 {
        self.stack_position(entity).unwrap_or(0) as u32
    }
}

pub fn capture_authored_modal_for_input_frame(
    context: ActiveAuthoredUiContext,
    mut capture: ResMut<AuthoredModalInputCapture>,
) {
    let modal = context.top_modal();
    if capture.0 != modal {
        capture.0 = modal;
    }
}

pub fn capture_overhead_mode_for_input_frame(
    tool: Option<
        Res<openzt2_zoo_state::construction::construction_tool_and_placement_policy_types::ConstructionTool>,
    >,
    immersive_modes: Query<
        (),
        With<openzt2_interface_state::immersive_modes::immersive_mode_state_types::ActiveImmersiveMode>,
    >,
    mut capture: ResMut<OverheadModeInputCapture>,
) {
    let overhead = tool.is_some_and(|tool| {
        *tool == openzt2_zoo_state::construction::construction_tool_and_placement_policy_types::ConstructionTool::Inspect
    }) && immersive_modes.is_empty();
    if capture.0 != overhead {
        capture.0 = overhead;
    }
}
