use bevy::{
    animation::{animated_field, AnimationTargetId},
    ecs::system::SystemParam,
    math::Affine3A,
    prelude::*,
};
use openzt2_game_assets::animation::{
    animation_clip_source_queries::AnimationClipSources,
    animation_set_asset_types::{AnimationClipSourceAsset, AnimationSetAsset},
};
use openzt2_world_state::animation_graph::{
    animation_presentation_relationship_types::AnimationPresentations,
    model_animation_asset_attachment::resolve_animation_clip_or_graph_node_asset_key,
};
use openzt2_world_state::animation_playback::animation_playback_controller_types::AnimationPlaybackController;

#[derive(SystemParam)]
pub(super) struct DockingAnimationPose<'w, 's> {
    clips: Option<Res<'w, Assets<AnimationClip>>>,
    sources: Option<AnimationClipSources<'w>>,
    sets: Option<Res<'w, Assets<AnimationSetAsset>>>,
    presentations: Query<'w, 's, &'static AnimationPresentations>,
    controllers: Query<'w, 's, &'static AnimationPlaybackController>,
    locals: Query<
        'w,
        's,
        (
            &'static GlobalTransform,
            Option<&'static AnimationTargetId>,
            &'static ChildOf,
        ),
    >,
    globals: Query<'w, 's, &'static GlobalTransform>,
}

impl DockingAnimationPose<'_, '_> {
    pub(super) fn clip_resolved(&self, source: &Handle<AnimationClipSourceAsset>) -> bool {
        self.sources
            .as_ref()
            .is_none_or(|sources| sources.resolved(source))
    }

    pub(super) fn pending_clip(
        &self,
        owner: Entity,
        animation: &str,
    ) -> Option<Handle<AnimationClipSourceAsset>> {
        let sources = self.sources.as_ref()?;
        let sets = self.sets.as_ref()?;
        AnimationPresentations::for_subject(owner, self.presentations.get(owner).ok())
            .filter_map(|controller| self.controllers.get(controller).ok())
            .find_map(|controller| {
                let set = sets.get(&controller.animation_set_asset)?;
                let key = resolve_animation_clip_or_graph_node_asset_key(set, animation)?;
                if sources.get(set, &key).is_some() {
                    return None;
                }
                sources
                    .request(set, &key)
                    .filter(|source| !sources.resolved(source))
            })
    }

    pub(super) fn sample_node(
        &self,
        owner: Entity,
        node: Entity,
        animation: &str,
        owner_pose: &GlobalTransform,
    ) -> Option<GlobalTransform> {
        let (Some(clips), Some(sources), Some(sets)) = (&self.clips, &self.sources, &self.sets)
        else {
            return None;
        };
        let clip = AnimationPresentations::for_subject(owner, self.presentations.get(owner).ok())
            .filter_map(|controller| self.controllers.get(controller).ok())
            .find_map(|controller| {
                let set = sets.get(&controller.animation_set_asset)?;
                let key = resolve_animation_clip_or_graph_node_asset_key(set, animation)?;
                clips.get(sources.get(set, &key)?.clip)
            })?;
        let mut point = Affine3A::IDENTITY;
        let mut current = node;
        while current != owner {
            let (global, target, parent) = self.locals.get(current).ok()?;
            let mut sampled = global.reparented_to(self.globals.get(parent.parent()).ok()?);
            if let Some(&target) = target {
                sampled.translation = clip
                    .sample_clamped(animated_field!(Transform::translation), target, 0.0)
                    .unwrap_or(sampled.translation);
                sampled.rotation = clip
                    .sample_clamped(animated_field!(Transform::rotation), target, 0.0)
                    .unwrap_or(sampled.rotation);
                sampled.scale = clip
                    .sample_clamped(animated_field!(Transform::scale), target, 0.0)
                    .unwrap_or(sampled.scale);
            }
            point = sampled.compute_affine() * point;
            current = parent.parent();
        }
        let pose = owner_pose.affine() * point;
        pose.is_finite()
            .then(|| GlobalTransform::from(Transform::from_matrix(Mat4::from(pose))))
    }
}

use openzt2_game_assets::world_definitions::{
    world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
};
use openzt2_game_data::AssetId;
use openzt2_world_state::{
    animation_graph::animation_presentation_relationship_types::AnimationPresentationBinder,
    animation_playback::animation_joint_target_types::AnimationRigJoints,
    world_spawn::world_membership_types::DefinitionId,
};
use openzt2_zoo_state::{
    animal_lifecycle::types::AnimalVariant, locomotion::locomotion_types::LocomotionMode,
};

#[derive(SystemParam)]
pub(super) struct DockingBuoyancy<'w, 's> {
    definitions: Res<'w, WorldDefinitions>,
    assets: Res<'w, Assets<WorldDefinitionAsset>>,
    owners: Query<
        'w,
        's,
        (
            Option<&'static AnimalVariant>,
            Option<&'static DefinitionId>,
            &'static LocomotionMode,
        ),
    >,
    presentations: Query<'w, 's, &'static AnimationPresentations>,
    rigs: Query<
        'w,
        's,
        (
            Option<&'static AnimationPresentationBinder>,
            &'static AnimationRigJoints,
        ),
    >,
    joints: Query<'w, 's, (&'static Name, &'static GlobalTransform)>,
}

impl DockingBuoyancy<'_, '_> {
    pub(super) fn support_offset(&self, actor: Entity, root: &GlobalTransform) -> Option<Vec3> {
        let (variant, definition, mode) = self.owners.get(actor).ok()?;
        if *mode != LocomotionMode::Swim {
            return None;
        }
        let definition = variant
            .map(|variant| variant.0)
            .or_else(|| definition.map(|id| id.0))?;
        let definitions = self.definitions.get(&self.assets)?;
        let rig = AnimationPresentations::for_subject(actor, self.presentations.get(actor).ok())
            .filter_map(|controller| self.rigs.get(controller).ok())
            .find_map(|(binder, joints)| {
                let (asset, row) = definitions.animation_binder_rig_source(
                    definition,
                    binder.map_or(AssetId::from_key("mainobj"), |binder| binder.0),
                )?;
                let rig = self.assets.get(asset)?.animation_binder_rigs().get(row)?;
                let ground_fit = rig
                    .quadruped_ground_fit
                    .as_ref()
                    .or(rig.biped_ground_fit.as_ref())?;
                Some((ground_fit, joints))
            })?;
        let targets = rig.0.buoyancy;
        let mut offsets = self
            .joints
            .iter_many(rig.1.joints().iter().copied())
            .flatten()
            .filter(|(name, _)| {
                targets.contains(&Some(AssetId::from_key(
                    &name.as_str().to_ascii_lowercase(),
                )))
            })
            .map(|(_, pose)| root.affine().inverse().transform_point3(pose.translation()));
        let first = offsets.next()?;
        Some(
            offsets
                .next()
                .map_or(first, |second| (first + second) * 0.5),
        )
    }
}
