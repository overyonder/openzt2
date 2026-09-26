use bevy::prelude::*;

/// One requested authored UI document whose asset loader reported failure
/// before it could be projected for its lifecycle owner.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UiDocumentAssetLoadFailed {
    pub(crate) owner: Entity,
}
