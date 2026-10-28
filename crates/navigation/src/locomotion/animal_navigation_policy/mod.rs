//! Animal collision rules used by route planning.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use openzt2_game_assets::species::species_asset_types::SpeciesAsset;
use openzt2_game_assets::species::species_asset_types::SpeciesAssets;
use openzt2_zoo_state::animal_lifecycle::types::AnimalVariant;

use openzt2_zoo_state::locomotion::locomotion_types::Destination;
use openzt2_zoo_state::locomotion::locomotion_types::NavAgent;
use openzt2_zoo_state::locomotion::locomotion_types::NavigationFailure;
use openzt2_zoo_state::locomotion::locomotion_types::Route;

#[derive(SystemParam)]
pub(super) struct AnimalNavigationPolicy<'w> {
    assets: Option<Res<'w, Assets<SpeciesAsset>>>,
    index: Option<Res<'w, SpeciesAssets>>,
}

impl AnimalNavigationPolicy<'_> {
    pub(super) fn agent_with_authored_collision_limits(
        &self,
        agent: &NavAgent,
        animal: Option<&AnimalVariant>,
    ) -> Result<(NavAgent, Option<f32>), NavigationFailure> {
        let Some(animal) = animal else {
            return Ok((*agent, None));
        };
        let policy = self
            .index
            .as_ref()
            .zip(self.assets.as_ref())
            .and_then(|(index, assets)| index.get(assets))
            .and_then(|species| species.find_variant(animal.0))
            .and_then(|variant| variant.navigation_collision_policy.as_ref())
            .filter(|policy| {
                policy.radius_m.is_finite()
                    && policy.radius_m >= 0.0
                    && policy.maximum_slope_radians.is_finite()
                    && policy.maximum_slope_radians >= 0.0
            })
            .ok_or(NavigationFailure::NoRoute)?;
        Ok((
            NavAgent {
                radius_m: policy.radius_m,
                ..*agent
            },
            Some(policy.maximum_slope_radians),
        ))
    }
}

/// An asset reload or variant replacement invalidates routes planned with the
/// previous collision limits. The destination planner replaces them on its next pass.
pub(super) fn invalidate_animal_routes_after_policy_changes(
    policy: AnimalNavigationPolicy,
    mut animals: Query<(
        Ref<AnimalVariant>,
        &mut Destination,
        &mut Route,
        Option<&openzt2_zoo_state::locomotion::locomotion_types::RestoredNavigationRoute>,
    )>,
) {
    let source_changed = policy
        .assets
        .as_ref()
        .is_some_and(|assets| assets.is_changed())
        || policy
            .index
            .as_ref()
            .is_some_and(|index| index.is_changed());
    for (variant, mut destination, mut route, restored) in &mut animals {
        if restored.is_some_and(|restored| restored.request_id == destination.request_id) {
            continue;
        }
        if source_changed || variant.is_changed() {
            route.clear();
            destination.set_changed();
        }
    }
}
