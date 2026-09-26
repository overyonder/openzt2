use std::collections::HashSet;

use bevy::prelude::*;
use openzt2_game_data::AssetId;

/// Interaction state for one projected Zoopedia table of contents. Stable
/// identifiers are retained while hierarchy and labels remain asset-owned.
#[derive(Component)]
pub(in crate::plugins::information) struct ZoopediaTableOfContentsState {
    pub(super) expanded_subjects: HashSet<AssetId>,
    pub(super) projected_row_count: u16,
}

/// Presentation identity for one reusable table-of-contents row.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub(in crate::plugins::information) struct ZoopediaTableOfContentsRow {
    pub(super) subject: AssetId,
    pub(super) zoopedia_page_entity: Entity,
}
