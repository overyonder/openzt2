use bevy::prelude::*;
use openzt2_game_assets::species::species_asset_types::{SpeciesAsset, SpeciesAssets};
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::{
    animal_lifecycle::types::AnimalVariant,
    locomotion::locomotion_types::{AnimalTerrainWaterContact, LocomotionMode, SwimLayer},
    terrain::dry_surface_eligibility::DrySurfaceEligibility,
};

pub(super) fn project_animal_water_contact(
    mut commands: Commands,
    species_assets: Res<Assets<SpeciesAsset>>,
    species_index: Res<SpeciesAssets>,
    surfaces: DrySurfaceEligibility,
    mut animals: Query<
        (
            Entity,
            &WorldMember,
            &AnimalVariant,
            &mut Transform,
            Option<&mut LocomotionMode>,
            Option<&mut AnimalTerrainWaterContact>,
        ),
        (
            Without<openzt2_zoo_state::animal_health::types::Dead>,
            Without<openzt2_world_state::world_spawn::world_entity_crating::WorldEntityIsCrated>,
        ),
    >,
) {
    let Some(species) = species_index.get(&species_assets) else {
        return;
    };
    for (entity, member, variant, mut transform, current_mode, contact) in &mut animals {
        let position = transform.translation.xz();
        let Some(ground) = surfaces.ground_height(member.root, position) else {
            continue;
        };
        let surface = surfaces.water_surface_height(member.root, position);
        let depth_m = surface.map(|height| height - ground);
        if let Some(contact) = contact {
            contact
                .map_unchanged(|contact| &mut contact.depth_m)
                .set_if_neq(depth_m);
        } else {
            commands
                .entity(entity)
                .insert(AnimalTerrainWaterContact { depth_m });
        }
        let Some(policy) = species
            .find_variant(variant.0)
            .and_then(|variant| variant.surface_water_locomotion.as_ref())
        else {
            continue;
        };
        let current = current_mode.as_deref().copied().unwrap_or_default();
        if !matches!(current, LocomotionMode::Ground | LocomotionMode::Swim) {
            continue;
        }
        let threshold = if current == LocomotionMode::Swim {
            policy.water_exit_depth_m
        } else {
            policy.water_entry_depth_m
        };
        let next = if depth_m.is_some_and(|depth| depth >= threshold) {
            LocomotionMode::Swim
        } else {
            LocomotionMode::Ground
        };
        // Motion fits a swimmer to the ground under a tank, whose water is not
        // terrain water; a leap above the surface stays where it is.
        if let Some(surface) = surface.filter(|_| next == LocomotionMode::Swim) {
            if transform.translation.y < surface {
                transform.translation.y = surface;
            }
        }
        if current_mode.is_none() || next != current {
            if let Some(mut mode) = current_mode {
                *mode = next;
            } else {
                commands.entity(entity).insert(next);
            }
            if next == LocomotionMode::Swim {
                commands.entity(entity).insert(SwimLayer::Surface);
            } else {
                commands.entity(entity).remove::<SwimLayer>();
            }
        }
    }
}
