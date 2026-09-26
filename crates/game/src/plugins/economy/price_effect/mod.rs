//! Authored price-effect alternatives use the target's selected Buy_Item band.

use bevy::{ecs::system::SystemParam, prelude::*};
use openzt2_game_data::{behavior::scalar::BehaviorScalarQ16, AssetId};

use super::facility_economy_types::FacilityPriceIndex;
use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::world_spawn::world_membership_types::DefinitionId;

#[derive(SystemParam)]
pub(crate) struct BehaviorPriceEffectContext<'w, 's> {
    definitions: Res<'w, WorldDefinitions>,
    assets: Res<'w, Assets<WorldDefinitionAsset>>,
    objects: Query<'w, 's, (&'static DefinitionId, Option<&'static FacilityPriceIndex>)>,
}

impl BehaviorPriceEffectContext<'_, '_> {
    pub(crate) fn sample(
        &self,
        scalar: BehaviorScalarQ16,
        target: Option<Entity>,
        draw: impl FnOnce(u32) -> u32,
    ) -> Option<i32> {
        if !matches!(scalar, BehaviorScalarQ16::PriceEffectQ16(_)) {
            return scalar.sample_q16(draw);
        }
        // No target entity contributes zero.
        let Some(target) = target else { return Some(0) };
        let (definition, selected) = self.objects.get(target).ok()?;
        let definitions = self.definitions.get(&self.assets)?;
        let object = definitions.find_object(definition.0)?;
        // A missing named transaction uses index zero.
        let index = object
            .transactions
            .iter()
            .find(|transaction| transaction.name == AssetId::from_key("buy_item"))
            .map_or(0, |transaction| {
                selected.map_or(transaction.initial_cost_index, |index| usize::from(index.0))
            });
        scalar.sample_with_price_index_q16(Some(index), draw)
    }
}
