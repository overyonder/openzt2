//! Read-only input scope derived from the projected Bevy UI hierarchy.
use openzt2_interface_state::ui::input_frame_capture_types::AuthoredModalInputCapture;
use openzt2_interface_state::ui::input_frame_capture_types::OverheadModeInputCapture;

use bevy::{ecs::system::SystemParam, prelude::*};

use super::{
    authored_modal_presentation::UiAuthoredModalPresentation,
    authored_reusable_list_and_table_runtime_types::UiListRow,
    authored_ui_focus_state::UiFocusable,
    authored_ui_interaction_enabled_state::UiInteractionEnabled,
    authored_ui_node_projection_components::{UiDocumentOwner, UiNodeId},
};

#[derive(SystemParam)]
pub struct ActiveAuthoredUiContext<'w, 's> {
    stack: Res<'w, bevy::ui::UiStack>,
    parents: Query<'w, 's, &'static ChildOf>,
    owners: Query<'w, 's, &'static UiDocumentOwner>,
    rows: Query<'w, 's, &'static UiListRow>,
    enabled: Query<'w, 's, &'static UiInteractionEnabled>,
    focusables: Query<'w, 's, &'static UiFocusable>,
    visibility: Query<'w, 's, &'static InheritedVisibility>,
    modals: Query<'w, 's, (Entity, &'static UiAuthoredModalPresentation)>,
    stack_indices: Query<'w, 's, &'static bevy::ui::ComputedStackIndex>,
    nodes: Query<
        'w,
        's,
        (Entity, &'static UiDocumentOwner, &'static UiNodeId),
        With<InheritedVisibility>,
    >,
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
            && self.enabled(entity)
    }

    /// No disabled node on the path to the root, whether or not it is shown.
    pub fn enabled(&self, entity: Entity) -> bool {
        self.ancestors(entity).all(|ancestor| {
            !self.enabled.get(ancestor).is_ok_and(|enabled| !enabled.0)
                && !self
                    .focusables
                    .get(ancestor)
                    .is_ok_and(|focusable| !focusable.enabled)
        })
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

    pub fn active_scope(&self) -> Option<Entity> {
        let modal = self.top_modal();
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

    pub fn hotkey_receiver_is_eligible(
        &self,
        owner: Entity,
        receiver_index: u32,
        scope: Option<Entity>,
        modal: Option<Entity>,
    ) -> bool {
        self.nodes.iter().any(|(entity, node_owner, node_id)| {
            node_owner.0 == owner
                && node_id.index == receiver_index
                && self.visible_and_enabled(entity)
                && scope.is_none_or(|scope| self.document_scope(owner) == scope)
                && modal.is_none_or(|modal| self.ancestors(entity).any(|node| node == modal))
        })
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
