//! Animal collision rules used by route planning.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use openzt2_game_assets::animation::{
    animation_clip_source_queries::AnimationClipSources,
    animation_set_asset_types::AnimationSetAsset,
};
use openzt2_game_assets::species::species_asset_types::SpeciesAsset;
use openzt2_game_assets::species::species_asset_types::SpeciesAssets;
use openzt2_game_data::animation::animation_set::AuthoredAnimationPlaybackPolicy;
use openzt2_world_state::animation_graph::animation_presentation_relationship_types::AnimationPresentations;
use openzt2_world_state::animation_playback::animation_playback_controller_types::AnimationPlaybackController;
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

#[derive(SystemParam)]
pub(super) struct ActiveLocomotionClipPolicy<'w, 's> {
    sets: Option<Res<'w, Assets<AnimationSetAsset>>>,
    clips: Option<AnimationClipSources<'w>>,
    controllers: Query<'w, 's, &'static AnimationPlaybackController>,
    presentations: Query<'w, 's, &'static AnimationPresentations>,
}

impl ActiveLocomotionClipPolicy<'_, '_> {
    pub(super) fn get(
        &self,
        actor: Entity,
        presentation: Option<Entity>,
    ) -> Option<&AuthoredAnimationPlaybackPolicy> {
        let sets = self.sets.as_deref()?;
        let controller = presentation
            .and_then(|controller| self.controllers.get(controller).ok())
            .or_else(|| {
                AnimationPresentations::for_subject(actor, self.presentations.get(actor).ok())
                    .find_map(|controller| self.controllers.get(controller).ok())
            })?;
        let set = sets.get(&controller.animation_set_asset)?;
        self.clips
            .as_ref()?
            .get(set, &controller.animation_clip_asset_key)?
            .policy
    }

    pub(super) fn resolves_unit_contacts(&self, actor: Entity) -> bool {
        self.get(actor, None)
            .and_then(|policy| policy.resolve_unit_collisions)
            .unwrap_or(true)
    }
}

pub(super) fn advance_clip_turn(
    current: Quat,
    desired: Quat,
    rates: [Option<f32>; 3],
    step: f32,
) -> Quat {
    let (yaw, pitch, roll) = (current.inverse() * desired).to_euler(EulerRot::YXZ);
    let advance = |angle: f32, rate: Option<f32>| {
        rate.filter(|rate| rate.is_finite()).map_or(angle, |rate| {
            let bound = rate.abs() * step;
            angle.clamp(-bound, bound)
        })
    };
    current
        * Quat::from_euler(
            EulerRot::YXZ,
            advance(yaw, rates[0]),
            advance(pitch, rates[1]),
            advance(roll, rates[2]),
        )
}
