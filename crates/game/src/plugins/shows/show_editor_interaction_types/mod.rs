use bevy::prelude::*;

/// The stage currently presented by one show-editor controller.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct EditedShow(pub Entity);

/// Present only while the controller permits schedule mutation.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct ShowEditing;

/// Present while one of the editor's performer/trick droplists is expanded.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ShowMixerDropdown(pub Entity);

/// Installed while the authored add-show confirmation is pending.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct AddShowConfirmationPending {
    pub stage: Entity,
}

/// Installed while the authored delete-show confirmation is pending.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct DeleteShowConfirmationPending {
    pub entry: Entity,
}

/// Installed by the construction/delete owner only after it has identified a
/// show platform whose authored confirmation is required.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ShowPlatformDeletionPending {
    pub stage: Entity,
}
