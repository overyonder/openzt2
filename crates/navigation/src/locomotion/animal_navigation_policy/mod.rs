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

#[derive(Clone, Copy, PartialEq)]
struct CollisionLimits {
    radius_m: f32,
    maximum_ground_slope_radians: f32,
}

#[derive(Component, Clone, Copy, Default, PartialEq)]
pub(super) struct AnimalCollisionProfile(Option<CollisionLimits>);

pub(super) fn project_animal_collision_profiles(
    assets: Option<Res<Assets<SpeciesAsset>>>,
    index: Option<Res<SpeciesAssets>>,
    mut animals: Query<(Ref<AnimalVariant>, &mut AnimalCollisionProfile)>,
) {
    let available = index.as_ref().zip(assets.as_ref());
    let species = available.and_then(|(index, assets)| index.get(assets));
    let source_changed = index.as_ref().is_some_and(DetectChanges::is_changed)
        || assets.as_ref().is_some_and(DetectChanges::is_changed);
    for (variant, mut profile) in &mut animals {
        if !source_changed && !variant.is_changed() && !profile.is_added() {
            continue;
        }
        let limits = species
            .as_ref()
            .and_then(|species| species.find_variant(variant.0))
            .and_then(|variant| variant.navigation_collision_policy.as_ref())
            .filter(|policy| {
                policy.radius_m.is_finite()
                    && policy.radius_m >= 0.0
                    && policy.maximum_slope_radians.is_finite()
                    && policy.maximum_slope_radians >= 0.0
            })
            .map(|policy| CollisionLimits {
                radius_m: policy.radius_m,
                maximum_ground_slope_radians: policy.maximum_slope_radians,
            });
        profile.set_if_neq(AnimalCollisionProfile(limits));
    }
}

#[derive(SystemParam)]
pub(super) struct AnimalNavigationPolicy<'w, 's> {
    profiles: Query<'w, 's, Option<&'static AnimalCollisionProfile>, With<AnimalVariant>>,
}

impl AnimalNavigationPolicy<'_, '_> {
    pub(super) fn agent_with_collision_limits(
        &self,
        entity: Entity,
        agent: &NavAgent,
    ) -> Result<(NavAgent, Option<f32>), NavigationFailure> {
        let Ok(resolved) = self.profiles.get(entity) else {
            return Ok((*agent, None));
        };
        let limits = resolved
            .and_then(|profile| profile.0)
            .ok_or(NavigationFailure::NoRoute)?;
        Ok((
            NavAgent {
                radius_m: limits.radius_m,
                ..*agent
            },
            Some(limits.maximum_ground_slope_radians),
        ))
    }
}

/// A change to live collision limits invalidates a non-restored route.
pub(super) fn invalidate_animal_routes_after_policy_changes(
    changed: Query<Entity, (With<AnimalVariant>, Changed<AnimalCollisionProfile>)>,
    mut removed: RemovedComponents<AnimalVariant>,
    mut routes: Query<(
        &mut Destination,
        &mut Route,
        Option<&openzt2_zoo_state::locomotion::locomotion_types::RestoredNavigationRoute>,
    )>,
) {
    for entity in changed.iter().chain(removed.read()) {
        let Ok((mut destination, mut route, restored)) = routes.get_mut(entity) else {
            continue;
        };
        if restored.is_some_and(|restored| restored.request_id == destination.request_id) {
            continue;
        }
        route.clear();
        destination.set_changed();
    }
}
