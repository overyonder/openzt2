//! Projects default active authored child binders onto primary prefab sockets.

use bevy::prelude::*;
use openzt2_game_data::AssetId;

/// Requests the canonical default child-binder presentation for one world
/// object definition beneath an already projected primary prefab tree.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PrefabObjectPresentationAttachmentProjection(pub AssetId);

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PrefabObjectPresentationAttachmentProjectionHydrated;
