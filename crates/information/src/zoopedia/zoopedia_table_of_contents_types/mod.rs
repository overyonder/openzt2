use std::collections::HashSet;

use bevy::prelude::*;
use openzt2_game_data::AssetId;

/// Interaction state for one projected Zoopedia table of contents. Stable
/// identifiers are retained while hierarchy and labels remain asset-owned.
#[derive(Component)]
pub(crate) struct ZoopediaTableOfContentsState {
    pub(super) expanded_subjects: HashSet<AssetId>,
}

#[derive(Component, Default, PartialEq, Eq)]
pub(crate) struct VisibleZoopediaEntries(pub(super) Vec<VisibleZoopediaEntry>);

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct VisibleZoopediaEntry {
    pub(super) subject: AssetId,
    pub(super) title_key: AssetId,
    pub(super) depth: u16,
    pub(super) expandable: bool,
}

/// Presentation identity for one reusable table-of-contents row.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ZoopediaTableOfContentsRow {
    pub(super) subject: AssetId,
    pub(super) zoopedia_page_entity: Entity,
}
