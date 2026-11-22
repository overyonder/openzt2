use bevy::{asset::AssetId as BevyAssetId, platform::collections::HashMap, prelude::*};
use openzt2_game_assets::texture::source_image_asset_path_selection::select_bevy_image_asset_path_for_blue_fang_source_image;
use openzt2_game_assets::world_definitions::{
    world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
};
use openzt2_game_data::{world_definitions::animation_binder_rig::AnimationBinderRig, AssetId};
use openzt2_world_state::{
    animation_graph::animation_presentation_relationship_types::{
        AnimationPresentationBinder, AnimationPresentationOwner,
    },
    animation_playback::animation_playback_controller_types::AnimationPlaybackController,
    world_spawn::world_membership_types::DefinitionId,
};
use openzt2_zoo_state::animal_lifecycle::types::AnimalVariant;

use super::{
    authored_decal_texture_cycles::{AnimationDecalFrames, DecalCyclePlayback},
    procedural_ground_fit::GroundFitPose,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct AnimationBinderRigSource {
    pub(super) asset: BevyAssetId<WorldDefinitionAsset>,
    pub(super) row: usize,
}

impl AnimationBinderRigSource {
    pub(super) fn get(self, assets: &Assets<WorldDefinitionAsset>) -> Option<&AnimationBinderRig> {
        assets
            .get(self.asset)?
            .animation_binder_rigs()
            .get(self.row)
    }
}

#[derive(Component)]
pub(super) struct BoundAnimationBinderRig {
    definition: AssetId,
    binder: AssetId,
    pub(super) source: Option<AnimationBinderRigSource>,
}

pub(super) fn bind_authored_animation_binder_rigs(
    mut commands: Commands,
    catalogue: Res<WorldDefinitions>,
    server: Res<AssetServer>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    owners: Query<(Option<&DefinitionId>, Option<&AnimalVariant>)>,
    mut controllers: Query<
        (
            Entity,
            &AnimationPresentationOwner,
            Option<&AnimationPresentationBinder>,
            Option<&mut BoundAnimationBinderRig>,
            Has<GroundFitPose>,
        ),
        With<AnimationPlaybackController>,
    >,
) {
    if !catalogue.is_complete() {
        return;
    }
    let main_object = AssetId::from_key("mainobj");
    let definitions = catalogue.get(&assets);
    let mut rig_sources = None;
    for (entity, owner, binder, binding, has_pose) in &mut controllers {
        let Ok((definition, variant)) = owners.get(owner.gameplay_entity) else {
            continue;
        };
        let Some(definition) = variant
            .map(|variant| variant.0)
            .or_else(|| definition.map(|id| id.0))
        else {
            continue;
        };
        let binder = binder.map_or(main_object, |binder| binder.0);
        if binding.as_ref().is_some_and(|bound| {
            bound.definition == definition
                && bound.binder == binder
                && bound.source.is_none_or(|source| {
                    source
                        .get(&assets)
                        .is_some_and(|rig| rig.definition == definition && rig.binder == binder)
                })
        }) {
            continue;
        }
        let sources =
            rig_sources.get_or_insert_with(|| {
                assets
                    .iter()
                    .flat_map(|(asset, definitions)| {
                        definitions.animation_binder_rigs().iter().enumerate().map(
                            move |(row, rig)| {
                                (
                                    (rig.definition, rig.binder),
                                    AnimationBinderRigSource { asset, row },
                                )
                            },
                        )
                    })
                    .collect::<HashMap<_, _>>()
            });
        let source = sources.get(&(definition, binder)).copied();
        let next = BoundAnimationBinderRig {
            definition,
            binder,
            source,
        };
        let mut controller = commands.entity(entity);
        if let Some(mut binding) = binding {
            *binding = next;
        } else {
            controller.try_insert(next);
        }
        let rig = source.and_then(|source| source.get(&assets));
        if !has_pose
            && rig.is_some_and(|rig| {
                rig.quadruped_ground_fit.is_some()
                    || rig.biped_ground_fit.is_some()
                    || rig.spine_bend.is_some()
            })
        {
            controller.try_insert(GroundFitPose::default());
        }
        let decals = rig.map_or(&[][..], |rig| rig.decal_cycles.as_slice());
        if decals.is_empty() {
            controller.try_remove::<(DecalCyclePlayback, AnimationDecalFrames)>();
        } else {
            controller.try_insert(DecalCyclePlayback::new(decals, |path| {
                definitions
                    .and_then(|definitions| {
                        definitions.texture_image(AssetId::from_virtual_path(path))
                    })
                    .unwrap_or_else(|| {
                        server.load(select_bevy_image_asset_path_for_blue_fang_source_image(
                            path,
                        ))
                    })
            }));
        }
    }
}
