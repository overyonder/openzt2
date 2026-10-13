use bevy::{mesh::skinning::SkinnedMesh, prelude::*};

use super::{transform_is_finite, AnimalPreviewCaptureQueries};

const JOINT_SAMPLE_LIMIT: usize = 8;

#[derive(Default, Debug)]
pub(super) struct JointObservationTotals {
    skins: usize,
    joints: usize,
    missing_joint_entities: usize,
    local_transforms: usize,
    global_transforms: usize,
    nonfinite_local_transforms: usize,
    nonfinite_global_transforms: usize,
    target_associations: usize,
    player_associations: usize,
    bevy_target_ids: usize,
    matching_targets: usize,
    matching_players: usize,
    matching_target_and_player: usize,
    matching_associations_with_bevy_target: usize,
    missing_inverse_binds: usize,
    nonfinite_inverse_binds: usize,
}

impl JointObservationTotals {
    pub(super) fn add(&mut self, other: Self) {
        self.skins += other.skins;
        self.joints += other.joints;
        self.missing_joint_entities += other.missing_joint_entities;
        self.local_transforms += other.local_transforms;
        self.global_transforms += other.global_transforms;
        self.nonfinite_local_transforms += other.nonfinite_local_transforms;
        self.nonfinite_global_transforms += other.nonfinite_global_transforms;
        self.target_associations += other.target_associations;
        self.player_associations += other.player_associations;
        self.bevy_target_ids += other.bevy_target_ids;
        self.matching_targets += other.matching_targets;
        self.matching_players += other.matching_players;
        self.matching_target_and_player += other.matching_target_and_player;
        self.matching_associations_with_bevy_target += other.matching_associations_with_bevy_target;
        self.missing_inverse_binds += other.missing_inverse_binds;
        self.nonfinite_inverse_binds += other.nonfinite_inverse_binds;
    }
}

impl AnimalPreviewCaptureQueries<'_, '_> {
    pub(super) fn observe_skin(
        &self,
        mesh: Entity,
        model: Entity,
        skin: &SkinnedMesh,
        detailed: bool,
    ) -> JointObservationTotals {
        let animation = self.models.get(model).ok();
        let controller = animation
            .as_ref()
            .and_then(|animation| animation.controller)
            .map(|_| model);
        let player = animation
            .as_ref()
            .and_then(|animation| animation.player)
            .map(|_| model);
        let inverse_binds = self
            .inverse_binds
            .as_ref()
            .and_then(|assets| assets.get(&skin.inverse_bindposes));
        let mut totals = JointObservationTotals {
            skins: 1,
            joints: skin.joints.len(),
            ..default()
        };
        for (index, joint) in skin.joints.iter().copied().enumerate() {
            let node = self.nodes.get(joint).ok();
            totals.missing_joint_entities += usize::from(node.is_none());
            let local = node.as_ref().and_then(|node| node.local);
            let global = node.as_ref().and_then(|node| node.global);
            let local_finite = local.map(transform_is_finite);
            let global_finite = global.map(|global| global.to_matrix().is_finite());
            totals.local_transforms += usize::from(local.is_some());
            totals.global_transforms += usize::from(global.is_some());
            totals.nonfinite_local_transforms += usize::from(local_finite == Some(false));
            totals.nonfinite_global_transforms += usize::from(global_finite == Some(false));
            let associations = self.joints.get(joint).ok();
            let target = associations.and_then(|(target, _, _)| target);
            let animated_by = associations.and_then(|(_, animated_by, _)| animated_by);
            let bevy_target = associations.and_then(|(_, _, target)| target);
            let target_matches = controller.is_some()
                && target.map(|target| target.animation_playback_controller_entity) == controller;
            let player_matches =
                player.is_some() && animated_by.map(|animated_by| animated_by.0) == player;
            totals.target_associations += usize::from(target.is_some());
            totals.player_associations += usize::from(animated_by.is_some());
            totals.bevy_target_ids += usize::from(bevy_target.is_some());
            totals.matching_targets += usize::from(target_matches);
            totals.matching_players += usize::from(player_matches);
            totals.matching_target_and_player += usize::from(target_matches && player_matches);
            totals.matching_associations_with_bevy_target +=
                usize::from(target_matches && player_matches && bevy_target.is_some());
            let inverse_bind = inverse_binds.and_then(|binds| binds.get(index));
            totals.missing_inverse_binds += usize::from(inverse_bind.is_none());
            totals.nonfinite_inverse_binds +=
                usize::from(inverse_bind.is_some_and(|matrix| !matrix.is_finite()));
            if detailed && index < JOINT_SAMPLE_LIMIT {
                info!(target: "openzt2_verification", ?mesh, ?model, index, ?joint,
                    entity_exists = node.is_some(), name = ?node.as_ref().and_then(|node| node.name),
                    joint_key = ?target.map(|target| target.joint_asset_key.as_str()),
                    ?bevy_target,
                    target_controller = ?target.map(|target| target.animation_playback_controller_entity),
                    animated_by = ?animated_by.map(|animated_by| animated_by.0), target_matches, player_matches,
                    ?local, ?local_finite, global = ?global.map(GlobalTransform::to_matrix), ?global_finite,
                    ?inverse_bind, inverse_bind_finite = ?inverse_bind.map(|matrix| matrix.is_finite()),
                    "animal preview joint and inverse bind sample");
            }
        }
        if detailed {
            info!(target: "openzt2_verification", ?mesh, ?model, ?controller, ?player,
                inverse_binds = ?skin.inverse_bindposes.id(), inverse_binds_loaded = inverse_binds.is_some(),
                inverse_bind_path = ?self.server.get_path(skin.inverse_bindposes.id()),
                inverse_bind_load_state = ?self.server.get_load_state(skin.inverse_bindposes.id()),
                inverse_bind_count = ?inverse_binds.map(|binds| binds.len()),
                inverse_bind_count_matches_joints = ?inverse_binds.map(|binds| binds.len() == skin.joints.len()),
                joint_samples_truncated = skin.joints.len().saturating_sub(JOINT_SAMPLE_LIMIT), ?totals,
                "animal preview all-joint association totals");
        }
        totals
    }
}
