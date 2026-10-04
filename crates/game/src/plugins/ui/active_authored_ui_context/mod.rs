//! Read-only input scope derived from the projected Bevy UI hierarchy.

use bevy::{ecs::system::SystemParam, prelude::*};

use super::{
    authored_modal_presentation::UiAuthoredModalPresentation,
    authored_reusable_list_and_table_runtime_types::UiListRow,
    authored_ui_focus_state::UiFocusable,
    authored_ui_interaction_enabled_state::UiInteractionEnabled,
    authored_ui_node_projection_components::{UiDocumentOwner, UiNodeId},
};

#[derive(SystemParam)]
pub(crate) struct ActiveAuthoredUiContext<'w, 's> {
    stack: Res<'w, bevy::ui::UiStack>,
    parents: Query<'w, 's, &'static ChildOf>,
    owners: Query<'w, 's, &'static UiDocumentOwner>,
    rows: Query<'w, 's, &'static UiListRow>,
    enabled: Query<'w, 's, &'static UiInteractionEnabled>,
    focusables: Query<'w, 's, &'static UiFocusable>,
    visibility: Query<'w, 's, &'static InheritedVisibility>,
    modals: Query<'w, 's, &'static UiAuthoredModalPresentation>,
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

    pub(crate) fn visible_and_enabled(&self, entity: Entity) -> bool {
        self.visibility
            .get(entity)
            .is_ok_and(|visible| visible.get())
            && self.ancestors(entity).all(|ancestor| {
                !self.enabled.get(ancestor).is_ok_and(|enabled| !enabled.0)
                    && !self
                        .focusables
                        .get(ancestor)
                        .is_ok_and(|focusable| !focusable.enabled)
            })
    }

    pub(crate) fn top_modal(&self) -> Option<Entity> {
        self.stack.uinodes.iter().rev().copied().find(|entity| {
            self.modals.get(*entity).is_ok_and(|modal| modal.is_modal())
                && self.visible_and_enabled(*entity)
        })
    }

    /// A reusable row is a document for assets, but participates in its list's
    /// input scope. Follow the actual row/list relation instead of retaining a
    /// second focus tree or treating each template instance as a modal.
    pub(crate) fn document_scope(&self, mut owner: Entity) -> Entity {
        while let Some(list_owner) = self.ancestors(owner).find_map(|ancestor| {
            self.rows
                .get(ancestor)
                .ok()
                .and_then(|row| self.owners.get(row.list).ok())
                .map(|owner| owner.0)
        }) {
            if list_owner == owner {
                break;
            }
            owner = list_owner;
        }
        owner
    }

    pub(crate) fn active_scope(&self) -> Option<Entity> {
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

    pub(crate) fn eligible_in_scope(
        &self,
        entity: Entity,
        scope: Entity,
        modal: Option<Entity>,
    ) -> bool {
        self.visible_and_enabled(entity)
            && self
                .owners
                .get(entity)
                .is_ok_and(|owner| self.document_scope(owner.0) == scope)
            && modal.is_none_or(|modal| self.ancestors(entity).any(|node| node == modal))
    }

    pub(crate) fn is_in_modal(&self, entity: Entity, modal: Entity) -> bool {
        self.ancestors(entity).any(|ancestor| ancestor == modal)
    }

    pub(crate) fn navigation_order(&self, entity: Entity) -> u32 {
        self.stack
            .uinodes
            .iter()
            .position(|node| *node == entity)
            .unwrap_or(0) as u32
    }

    pub(crate) fn hotkey_receiver_is_eligible(
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

/// Input-frame capture survives a modal being hidden or despawned by the same
/// cancel press. Gameplay must not consume that press later in the frame.
#[derive(Resource, Default)]
pub(crate) struct AuthoredModalInputCapture(pub(crate) Option<Entity>);

pub(crate) fn capture_authored_modal_for_input_frame(
    context: ActiveAuthoredUiContext,
    mut capture: ResMut<AuthoredModalInputCapture>,
) {
    capture.0 = context.top_modal();
}
