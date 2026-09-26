//! Maps placement mutation kinds and history directions to entity creation or removal.

use crate::plugins::construction::construction_transaction_types::EditApplication;

use super::placement_transaction_types::PreparedObjectPlacementEditMutationKind;

pub(super) const fn object_placement_edit_application_creates_entity(
    mutation: PreparedObjectPlacementEditMutationKind,
    application: EditApplication,
    entity_exists: bool,
) -> bool {
    match (mutation, application) {
        (
            PreparedObjectPlacementEditMutationKind::Create,
            EditApplication::InitialCommit | EditApplication::Redo,
        ) => true,
        (PreparedObjectPlacementEditMutationKind::Remove, EditApplication::Undo) => true,
        (PreparedObjectPlacementEditMutationKind::Relocate, _) => false,
        (_, EditApplication::FailureRollback) => !entity_exists,
        _ => false,
    }
}
