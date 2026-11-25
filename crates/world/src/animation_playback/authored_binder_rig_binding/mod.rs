use bevy::{asset::AssetId as BevyAssetId, prelude::*};
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
    authored_decal_texture_cycles::{
        AnimationDecalFrames, DecalCyclePlayback, DecalCyclesAdvancing,
    },
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

#[derive(Component, PartialEq, Eq)]
pub(super) struct BoundAnimationBinderRig {
    pub(super) source: Option<AnimationBinderRigSource>,
}

pub(super) fn bind_authored_animation_binder_rigs<Lane: Component>(
    mut commands: Commands,
    catalogue: Res<WorldDefinitions>,
    server: Res<AssetServer>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    owners: Query<(Option<Ref<DefinitionId>>, Option<Ref<AnimalVariant>>)>,
    mut changed_documents: MessageReader<AssetEvent<WorldDefinitionAsset>>,
    mut controllers: Query<
        (
            Entity,
            Ref<AnimationPresentationOwner>,
            Option<Ref<AnimationPresentationBinder>>,
            Option<&mut BoundAnimationBinderRig>,
            Has<GroundFitPose>,
            Option<&mut DecalCyclePlayback>,
        ),
        (With<AnimationPlaybackController>, With<Lane>),
    >,
) {
    if !catalogue.is_complete() {
        return;
    }
    let main_object = AssetId::from_key("mainobj");
    let definitions = catalogue.get(&assets);
    let changed_documents = changed_documents
        .read()
        .filter_map(|event| match *event {
            AssetEvent::Modified { id } | AssetEvent::Removed { id } => Some(id),
            _ => None,
        })
        .collect::<bevy::platform::collections::HashSet<_>>();
    for (entity, owner, binder, binding, has_pose, decal_playback) in &mut controllers {
        let Ok((definition, variant)) = owners.get(owner.gameplay_entity) else {
            continue;
        };
        let inputs_changed = owner.is_changed()
            || definition.as_ref().is_some_and(Ref::is_changed)
            || variant.as_ref().is_some_and(Ref::is_changed)
            || binder.as_ref().is_some_and(Ref::is_changed);
        let source_changed = binding
            .as_ref()
            .and_then(|binding| binding.source)
            .is_some_and(|source| changed_documents.contains(&source.asset));
        if binding.is_some() && !catalogue.is_changed() && !inputs_changed && !source_changed {
            continue;
        }
        let Some(definition) = variant
            .map(|variant| variant.0)
            .or_else(|| definition.map(|id| id.0))
        else {
            continue;
        };
        let binder = binder.map_or(main_object, |binder| binder.0);
        let source = definitions
            .and_then(|definitions| definitions.animation_binder_rig_source(definition, binder))
            .map(|(asset, row)| AnimationBinderRigSource { asset, row });
        if !source_changed
            && binding
                .as_ref()
                .is_some_and(|binding| binding.source == source)
        {
            continue;
        }
        let next = BoundAnimationBinderRig { source };
        let mut controller = commands.entity(entity);
        if let Some(mut binding) = binding {
            binding.set_if_neq(next);
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
            controller.try_remove::<(
                DecalCyclePlayback,
                AnimationDecalFrames,
                DecalCyclesAdvancing,
            )>();
        } else {
            let playback = DecalCyclePlayback::new(decals, |path| {
                definitions
                    .and_then(|definitions| {
                        definitions.texture_image(AssetId::from_virtual_path(path))
                    })
                    .unwrap_or_else(|| {
                        server.load(select_bevy_image_asset_path_for_blue_fang_source_image(
                            path,
                        ))
                    })
            });
            if playback.is_advancing() {
                controller.try_insert(DecalCyclesAdvancing);
            } else {
                controller.try_remove::<DecalCyclesAdvancing>();
            }
            if let Some(mut current) = decal_playback {
                *current = playback;
            } else {
                controller.try_insert(playback);
            }
        }
    }
}
