//! Animal collision rules used by route planning.

use bevy::{ecs::system::SystemParam, prelude::*};

use crate::{
    assets::species::species_asset_types::{SpeciesAsset, SpeciesAssets},
    plugins::animal_lifecycle::types::AnimalVariant,
};

use super::locomotion_types::{Destination, NavAgent, NavigationFailure, Route};

#[derive(SystemParam)]
pub(super) struct AnimalNavigationPolicy<'w> {
    assets: Option<Res<'w, Assets<SpeciesAsset>>>,
    index: Option<Res<'w, SpeciesAssets>>,
}

impl AnimalNavigationPolicy<'_> {
    pub(super) fn agent_with_authored_radius(
        &self,
        agent: &NavAgent,
        animal: Option<&AnimalVariant>,
    ) -> Result<NavAgent, NavigationFailure> {
        let Some(animal) = animal else {
            return Ok(*agent);
        };
        let radius = self
            .index
            .as_ref()
            .zip(self.assets.as_ref())
            .and_then(|(index, assets)| index.get(assets))
            .and_then(|species| species.find_variant(animal.0))
            .and_then(|variant| variant.navigation_collision_policy.as_ref())
            .map(|policy| policy.radius_m)
            .filter(|radius| radius.is_finite() && *radius >= 0.0)
            .ok_or(NavigationFailure::NoRoute)?;
        Ok(NavAgent {
            radius_m: radius,
            ..*agent
        })
    }
}

/// An asset reload or variant replacement invalidates routes planned with the
/// previous collision radius. The destination planner replaces them on its next pass.
pub(super) fn invalidate_animal_routes_after_policy_changes(
    policy: AnimalNavigationPolicy,
    mut animals: Query<(Ref<AnimalVariant>, &mut Destination, &mut Route)>,
) {
    let source_changed = policy
        .assets
        .as_ref()
        .is_some_and(|assets| assets.is_changed())
        || policy
            .index
            .as_ref()
            .is_some_and(|index| index.is_changed());
    for (variant, mut destination, mut route) in &mut animals {
        if source_changed || variant.is_changed() {
            route.clear();
            destination.set_changed();
        }
    }
}
