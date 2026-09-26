use bevy::prelude::*;

use crate::plugins::construction::construction_interaction_types::{
    CancelConstruction, CommitConstruction, ConstructionPreview, DeleteEntity, PlacementFailure,
    PlacementValidity,
};

use super::{
    ApplyPreparedObjectPlacementEditRequest, CommitObjectPlacementPreviewRequest,
    ObjectPlacementEditApplicationAcknowledged, ObjectPlacementEditPreparationRejected,
    PlacedObjectDefinitionReference, PlacementRotationRequest, RelocatingPlacedObject,
    RemovePlacedObjectRequest,
};

pub(super) fn route_valid_object_placement_requests_to_construction(
    mut requests: MessageReader<CommitObjectPlacementPreviewRequest>,
    previews: Query<&ConstructionPreview>,
    mut commits: MessageWriter<CommitConstruction>,
) {
    for request in requests.read() {
        if previews
            .get(request.preview)
            .is_ok_and(|preview| matches!(preview.validity, PlacementValidity::Valid { .. }))
        {
            commits.write(CommitConstruction {
                preview: request.preview,
            });
        }
    }
}

pub(super) fn route_placed_object_removal_requests_to_construction(
    mut requests: MessageReader<RemovePlacedObjectRequest>,
    placed: Query<(), With<PlacedObjectDefinitionReference>>,
    mut deletions: MessageWriter<DeleteEntity>,
) {
    for request in requests.read() {
        if placed.contains(request.entity) {
            deletions.write(DeleteEntity(request.entity));
        }
    }
}

pub(super) fn cancel_all_active_object_relocations(
    mut cancellations: MessageReader<CancelConstruction>,
    mut commands: Commands,
    relocating: Query<Entity, With<RelocatingPlacedObject>>,
) {
    if cancellations.read().next().is_none() {
        return;
    }
    for entity in &relocating {
        commands
            .entity(entity)
            .remove::<RelocatingPlacedObject>()
            .remove::<PlacementRotationRequest>();
    }
}

pub(super) fn reject_object_placement_edit_preparation(
    rejected: &mut MessageWriter<ObjectPlacementEditPreparationRejected>,
    transaction: Entity,
    reason: PlacementFailure,
) {
    rejected.write(ObjectPlacementEditPreparationRejected {
        transaction,
        reason,
    });
}

pub(super) fn acknowledge_object_placement_edit_application(
    acknowledged: &mut MessageWriter<ObjectPlacementEditApplicationAcknowledged>,
    request: &ApplyPreparedObjectPlacementEditRequest,
    accepted: bool,
) {
    acknowledged.write(ObjectPlacementEditApplicationAcknowledged {
        transaction: request.transaction,
        application: request.application,
        accepted,
    });
}
