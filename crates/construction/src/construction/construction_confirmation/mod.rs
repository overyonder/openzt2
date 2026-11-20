use bevy::prelude::*;
use openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use openzt2_game_data::ui_document::action::{
    construction::{UiConstructionAction, UiConstructionConfirmation},
    transportation::UiTransportAction,
};
use openzt2_interface_state::{
    information::entity_selection_types::WorldUiSelection,
    ui::{
        authored_ui_action_projection_components::{UiConstructionActions, UiTransportActions},
        authored_ui_node_projection_components::{UiDocumentOwner, UiDocumentRoot},
        input_frame_capture_types::AuthoredModalInputCapture,
    },
};
use openzt2_zoo_state::{
    aquatic::aquatic_simulation_types::Tank,
    construction::construction_interaction_types::{
        CancelConstruction, CommitConstruction, ConstructionCursor, ConstructionPreview,
        DeleteEntity, DeleteHoverTarget,
    },
    topology::{topology_confirmation::*, topology_graph_types::PathTile},
    transport_tours::transport_track_sale::TransportTrackSaleQueries,
};

#[derive(Component)]
pub(super) struct CapturedTopologyConfirmation;

pub(super) fn capture_pending_construction_confirmation(
    mut commands: Commands,
    modal: Option<Res<AuthoredModalInputCapture>>,
    captured: Query<(Entity, Option<&Children>), With<CapturedTopologyConfirmation>>,
    children: Query<&Children>,
    documents: Res<Assets<UiDocumentAsset>>,
    nodes: Query<(
        Option<&UiConstructionActions>,
        Option<&UiTransportActions>,
        &UiDocumentOwner,
    )>,
    roots: Query<&UiDocumentRoot>,
    operations: Query<(), With<TopologyConfirmation>>,
    selected: WorldUiSelection,
    targets: Query<(Has<Tank>, Has<PathTile>)>,
    previews: Query<Entity, With<ConstructionPreview>>,
    delete_hover: Query<&DeleteHoverTarget, With<ConstructionCursor>>,
    transport: TransportTrackSaleQueries,
) {
    let active = modal.and_then(|modal| modal.0);
    for (entity, children) in &captured {
        if active != Some(entity) {
            for operation in children.into_iter().flat_map(|children| children.iter()) {
                if operations.contains(operation) {
                    commands.entity(operation).try_despawn();
                }
            }
            commands
                .entity(entity)
                .try_remove::<CapturedTopologyConfirmation>();
        }
    }
    let Some(modal) = active.filter(|&modal| !captured.contains(modal)) else {
        return;
    };
    let kind = std::iter::once(modal)
        .chain(children.iter_descendants(modal))
        .find_map(|node| {
            let (construction, transport, owner) = nodes.get(node).ok()?;
            let document = documents.get(&roots.get(owner.0).ok()?.document)?;
            construction
                .into_iter()
                .flat_map(|actions| actions.authored_action_records(document))
                .find_map(|record| match record.action {
                    UiConstructionAction::ResolveConfirmation { kind, .. } => {
                        Some(TopologyConfirmation::Construction(kind))
                    }
                    _ => None,
                })
                .or_else(|| {
                    transport
                        .into_iter()
                        .flat_map(|actions| actions.authored_action_records(document))
                        .any(|record| {
                            matches!(
                                record.action,
                                UiTransportAction::ResolvePendingTransportPathDeletion { .. }
                            )
                        })
                        .then_some(TopologyConfirmation::TransportDeletion)
                })
        });
    let Some(kind) = kind else { return };
    commands.entity(modal).insert(CapturedTopologyConfirmation);
    let selected = selected.subject();
    let (operand, action) = match kind {
        TopologyConfirmation::Construction(UiConstructionConfirmation::TankDeletion) => (
            selected.filter(|&entity| targets.get(entity).is_ok_and(|(tank, _)| tank)),
            ConfirmedTopologyAction::Delete,
        ),
        TopologyConfirmation::Construction(UiConstructionConfirmation::ZooGatePathDeletion) => (
            delete_hover
                .single()
                .ok()
                .map(|target| target.0)
                .or(selected)
                .filter(|&entity| targets.get(entity).is_ok_and(|(_, path)| path)),
            ConfirmedTopologyAction::Delete,
        ),
        TopologyConfirmation::Construction(UiConstructionConfirmation::TankSplit) => {
            (previews.single().ok(), ConfirmedTopologyAction::Commit)
        }
        TopologyConfirmation::Construction(UiConstructionConfirmation::TankMerge) => {
            (Some(modal), ConfirmedTopologyAction::CancelPreview)
        }
        TopologyConfirmation::TransportDeletion => (
            delete_hover
                .single()
                .ok()
                .map(|target| target.0)
                .or(selected)
                .filter(|&entity| transport.can_sell_track_at(entity)),
            ConfirmedTopologyAction::Delete,
        ),
    };
    if let Some(operand) = operand {
        commands.spawn((
            ChildOf(modal),
            kind,
            TopologyConfirmationOperand(operand),
            action,
        ));
    }
}

pub(super) fn resolve_topology_confirmation(
    resolved: On<ResolveTopologyConfirmation>,
    mut operations: Query<(&TopologyConfirmationOperand, &mut ConfirmedTopologyAction)>,
    mut commands: Commands,
) {
    let Ok((operand, mut action)) = operations.get_mut(resolved.entity) else {
        return;
    };
    let action = std::mem::replace(&mut *action, ConfirmedTopologyAction::Consumed);
    commands.entity(resolved.entity).try_despawn();
    if !resolved.accepted {
        return;
    }
    match action {
        ConfirmedTopologyAction::Delete => {
            commands.write_message(DeleteEntity(operand.0));
        }
        ConfirmedTopologyAction::Commit => {
            commands.write_message(CommitConstruction { preview: operand.0 });
        }
        ConfirmedTopologyAction::Consumed => {}
        ConfirmedTopologyAction::CancelPreview => {
            commands.write_message(CancelConstruction);
        }
    }
}
