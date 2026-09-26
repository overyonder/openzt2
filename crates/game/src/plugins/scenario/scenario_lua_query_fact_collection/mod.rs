use std::collections::BTreeMap;

use bevy::prelude::*;

use crate::plugins::world_spawn::world_membership_types::DefinitionId;

pub(super) fn count_live_world_entities_by_definition(
    definitions: &Query<&DefinitionId>,
) -> BTreeMap<openzt2_game_data::AssetId, u32> {
    definitions.iter().fold(BTreeMap::new(), |mut counts, id| {
        *counts.entry(id.0).or_default() += 1;
        counts
    })
}
