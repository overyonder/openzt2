//! Deterministic per-employee look drawn from a role's authored variants.
//!
//! The draw depends only on the employee's persistent id, so a reloaded world
//! redraws the same body, head and textures without storing the choice.

use bevy::prelude::*;
use openzt2_game_data::world_definitions::staff_management::{
    StaffPresentationVariant, StaffRoleDefinition,
};
use openzt2_game_data::world_definitions::world_objects::WorldObjectTextureReplacementSet;
use openzt2_game_data::AssetId;

use openzt2_game_assets::scene_prefab::ScenePrefabAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use openzt2_world_state::animation_playback::model_joint_attachment_binding::PendingModelJointAttachment;
use openzt2_world_state::world_spawn::prefab_model_level_of_detail_visibility_range::SimpleLodSwitching;
use openzt2_world_state::world_spawn::prefab_presentation_types::PrefabPresentation;
use openzt2_world_state::world_spawn::prefab_texture_replacements::PrefabTextureReplacements;

#[cfg(test)]
mod tests;

/// Animation set of the employee's drawn body, which may differ from the
/// role's default body (for example the other sex).
#[derive(Component, Debug, Clone, Copy)]
pub struct StaffModelAnimationSet(pub AssetId);

/// Static head model riding a body joint; the body's actor animations do not
/// apply to it.
#[derive(Component, Debug, Clone, Copy)]
pub struct StaffAttachedHead;

/// SplitMix64 finalizer: spreads sequential persistent ids across variants.
const fn mix(bits: u64) -> u64 {
    let bits = (bits ^ (bits >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    let bits = (bits ^ (bits >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    bits ^ (bits >> 31)
}

/// The drawn look and its body scene. A look whose body scene is not
/// registered is not drawn, so the head and textures never land on the
/// role's fallback body.
pub fn staff_presentation_variant<'a>(
    role: &'a StaffRoleDefinition,
    persistent_id: u64,
    definitions: WorldDefinitionsView<'_>,
) -> Option<(&'a StaffPresentationVariant, Handle<ScenePrefabAsset>)> {
    let count = role.presentation_variants.len() as u64;
    let variant = role
        .presentation_variants
        .get((mix(persistent_id) % count.max(1)) as usize)?;
    Some((variant, definitions.scene(variant.prefab)?))
}

/// The staff role a world definition names, either directly or by its object.
pub fn staff_role_for_definition<'a>(
    definitions: WorldDefinitionsView<'a>,
    definition: AssetId,
) -> Option<&'a StaffRoleDefinition> {
    definitions
        .find_staff(definition)
        .or_else(|| definitions.find_staff_by_object(definition))
}

/// The body scene an employee spawns with: the drawn look's body, or the
/// role object's prefab when the role has no drawable look.
pub fn staff_body_prefab(
    role: &StaffRoleDefinition,
    persistent_id: u64,
    definitions: WorldDefinitionsView<'_>,
) -> Option<Handle<ScenePrefabAsset>> {
    staff_presentation_variant(role, persistent_id, definitions)
        .map(|(_, prefab)| prefab)
        .or_else(|| definitions.scene(definitions.find_object(role.object)?.prefab))
}

/// One weighted group index per replacement set.
fn drawn_texture_replacement_groups(
    sets: &[WorldObjectTextureReplacementSet],
    persistent_id: u64,
) -> impl Iterator<Item = usize> + '_ {
    let mut bits = mix(persistent_id ^ 0x7465_7874_7572_6573);
    sets.iter().map(move |set| {
        bits = mix(bits);
        let total = set
            .groups
            .iter()
            .map(|group| f64::from(group.weight))
            .sum::<f64>();
        let mut remaining = (bits >> 11) as f64 / (1_u64 << 53) as f64 * total;
        set.groups
            .iter()
            .position(|group| {
                remaining -= f64::from(group.weight);
                remaining < 0.0
            })
            .unwrap_or(set.groups.len().saturating_sub(1))
    })
}

/// Attaches the drawn head and texture replacements beneath a spawned body.
pub fn attach_staff_presentation_variant(
    commands: &mut Commands,
    staff: Entity,
    variant: &StaffPresentationVariant,
    persistent_id: u64,
    definitions: WorldDefinitionsView<'_>,
) {
    commands
        .entity(staff)
        .insert(StaffModelAnimationSet(variant.model_animation_set));
    dress_staff(commands, staff, variant, persistent_id, definitions, false);
}

/// What an employee currently wears: its wetsuit or its own look, and the
/// head it spawned for it.
#[derive(Component, Debug, Clone, Copy)]
pub struct StaffDress {
    pub wet: bool,
    pub head: Option<Entity>,
}

/// Spawns the look's head and sets its textures, or in water its wetsuit's
/// head and textures drawn over the look's own, as `ZTWetsuitComponent` swaps
/// them. A look without a wetsuit always wears its own.
pub fn dress_staff(
    commands: &mut Commands,
    staff: Entity,
    variant: &StaffPresentationVariant,
    persistent_id: u64,
    definitions: WorldDefinitionsView<'_>,
    wet: bool,
) {
    let wetsuit = variant.wetsuit.as_ref().filter(|_| wet);
    let head = wetsuit
        .map_or(variant.head.as_ref(), |wetsuit| wetsuit.head.as_ref())
        .and_then(|head| Some((head, definitions.scene(head.prefab)?)))
        .map(|(head, prefab)| {
            commands
                .spawn((
                    Transform::IDENTITY,
                    Visibility::Inherited,
                    ChildOf(staff),
                    PrefabPresentation::new(prefab),
                    PendingModelJointAttachment::new(staff, &head.joint),
                    StaffAttachedHead,
                    // A head is a `BFSimpleLODComponent` model.
                    SimpleLodSwitching(head.simple_lod_distances_m.clone()),
                ))
                .id()
        });
    // The first replacement for a material wins, so the wetsuit's go first.
    let textures = wetsuit
        .map(|wetsuit| wetsuit.texture_replacement_sets.as_slice())
        .into_iter()
        .chain([variant.texture_replacement_sets.as_slice()])
        .flat_map(|sets| {
            sets.iter()
                .zip(drawn_texture_replacement_groups(sets, persistent_id))
                .flat_map(|(set, group)| &set.groups[group].items)
        })
        .filter_map(|item| {
            Some((
                item.material.clone().into_boxed_str(),
                definitions.texture_image(item.image)?,
            ))
        })
        .collect::<Box<[_]>>();
    let mut staff = commands.entity(staff);
    staff.insert(StaffDress {
        wet: wetsuit.is_some(),
        head,
    });
    if textures.is_empty() {
        staff.remove::<PrefabTextureReplacements>();
    } else {
        staff.insert(PrefabTextureReplacements(textures));
    }
}
