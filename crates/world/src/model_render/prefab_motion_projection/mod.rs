use bevy::{
    app::{HierarchyPropagatePlugin, Propagate, PropagateSet},
    asset::asset_changed::AssetChanged,
    camera::{
        primitives::Aabb,
        visibility::{NoAutoAabb, VisibilitySystems},
    },
    mesh::{
        morph::{MeshMorphWeights, MorphWeights},
        VertexAttributeValues,
    },
    prelude::*,
};
use openzt2_game_assets::scene_prefab::transform_animation_lowering::PrefabMotionVisibility;
use openzt2_world_state::model_render::{
    prefab_render_lod_tier::PrefabRenderLodSelection, ModelRenderPresentationSet,
};

use super::prefab_model_level_of_detail_projection::PrefabLodVisibleCameras;

#[derive(Component, Clone, Copy, PartialEq)]
pub(crate) struct PrefabMorphWeightsOwner {
    pub(crate) entity: Entity,
    pub(crate) refresh_bound: bool,
}

pub(super) fn register_prefab_motion_projection(app: &mut App) {
    app.add_plugins(HierarchyPropagatePlugin::<PrefabMorphWeightsOwner>::new(PostUpdate))
        .add_systems(PostUpdate, (
            project_prefab_motion_visibility
                .after(ModelRenderPresentationSet::LevelOfDetail)
                .before(VisibilitySystems::VisibilityPropagate),
            project_prefab_mesh_morph_weights
                .after(PropagateSet::<PrefabMorphWeightsOwner>::default())
                .after(bevy::app::AnimationSystems)
                .before(super::authored_model_material_pass_projection::synchronize_additional_pass_geometry),
        ));
}

fn project_prefab_motion_visibility(
    mut rows: Query<
        (
            &PrefabMotionVisibility,
            Option<&PrefabRenderLodSelection>,
            Option<&Propagate<PrefabLodVisibleCameras>>,
            &mut Visibility,
        ),
        Or<(
            Changed<PrefabMotionVisibility>,
            Changed<PrefabRenderLodSelection>,
            Changed<Propagate<PrefabLodVisibleCameras>>,
        )>,
    >,
) {
    for (sample, tier, distance, mut visibility) in &mut rows {
        let show = sample.sample > 0.0
            && tier.is_none_or(|tier| tier.0)
            && distance.is_none_or(|distance| !distance.0 .0.is_empty());
        visibility.set_if_neq(if show {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
}

fn project_prefab_mesh_morph_weights(
    mut commands: Commands,
    meshes: Res<Assets<Mesh>>,
    owners: Query<Ref<MorphWeights>>,
    mut draws: Query<(
        Entity,
        Ref<Mesh3d>,
        Ref<PrefabMorphWeightsOwner>,
        Option<&mut MeshMorphWeights>,
        Option<&mut Aabb>,
        Has<NoAutoAabb>,
    )>,
    changed_meshes: Query<Entity, AssetChanged<Mesh3d>>,
) {
    for (entity, handle, owner, binding, aabb, manual_bound) in &mut draws {
        let Ok(weights) = owners.get(owner.entity) else {
            continue;
        };
        let mesh_changed = handle.is_changed() || changed_meshes.contains(entity);
        if !mesh_changed && !owner.is_changed() && !(owner.refresh_bound && weights.is_changed()) {
            continue;
        }
        let Some(mesh) = meshes.get(&handle.0) else {
            continue;
        };
        let Some(targets) = mesh.get_morph_targets() else {
            continue;
        };
        let Some(VertexAttributeValues::Float32x3(positions)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            continue;
        };
        if let Some(mut binding) = binding {
            if !matches!(*binding, MeshMorphWeights::Reference(current) if current == owner.entity)
            {
                *binding = MeshMorphWeights::Reference(owner.entity);
            }
        } else {
            commands
                .entity(entity)
                .try_insert(MeshMorphWeights::Reference(owner.entity));
        }
        if !owner.refresh_bound || positions.is_empty() {
            continue;
        }
        let count = positions.len();
        if targets.len() / count != weights.weights().len() || targets.len() % count != 0 {
            continue;
        }
        let mut minimum = Vec3::splat(f32::INFINITY);
        let mut maximum = Vec3::splat(f32::NEG_INFINITY);
        for (index, position) in positions.iter().enumerate() {
            let mut point = Vec3::from_array(*position);
            for (target, weight) in targets.chunks_exact(count).zip(weights.weights()) {
                point += target[index].position * *weight;
            }
            minimum = minimum.min(point);
            maximum = maximum.max(point);
        }
        let bound = Aabb::from_min_max(minimum, maximum);
        if let Some(mut current) = aabb {
            current.set_if_neq(bound);
        } else {
            commands.entity(entity).try_insert(bound);
        }
        if !manual_bound {
            commands.entity(entity).try_insert(NoAutoAabb);
        }
    }
}
