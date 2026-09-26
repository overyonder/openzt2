//! Scene-prefab asset registration.

use bevy::{
    asset::{AssetApp, LoadContext},
    gltf::Gltf,
    prelude::*,
};
use openzt2_game_data::{
    scene_prefab::{ScenePrefabAssetDependencyKind, ScenePrefabDocument},
    AssetId,
};

use crate::assets::{
    animation::animation_set_asset_types::AnimationSetAsset, effect::ParticleEffectDocumentAsset,
};

#[derive(Asset, TypePath, Debug)]
pub struct ScenePrefabAsset {
    document: ScenePrefabDocument,
    model_paths: Box<[(AssetId, Box<str>)]>,
    collider_models: Box<[(AssetId, Handle<Gltf>)]>,
    material_paths: Box<[(AssetId, Box<str>)]>,
    effects: Box<[(AssetId, Handle<ParticleEffectDocumentAsset>)]>,
    animations: Box<[(AssetId, Handle<AnimationSetAsset>)]>,
}

impl ScenePrefabAsset {
    pub fn canonical_scene_prefab_document(&self) -> &ScenePrefabDocument {
        &self.document
    }

    pub fn loaded_model_asset_handle(&self, asset_id: AssetId) -> Option<&Handle<Gltf>> {
        find_loaded_dependency_handle_by_asset_id(&self.collider_models, asset_id)
    }

    pub(crate) fn model_asset_path(&self, asset_id: AssetId) -> Option<&str> {
        self.model_paths
            .binary_search_by_key(&asset_id.0, |(candidate, _)| candidate.0)
            .ok()
            .map(|index| self.model_paths[index].1.as_ref())
    }

    pub(crate) fn material_asset_path(&self, asset_id: AssetId) -> Option<&str> {
        self.material_paths
            .binary_search_by_key(&asset_id.0, |(candidate, _)| candidate.0)
            .ok()
            .map(|index| self.material_paths[index].1.as_ref())
    }

    pub(crate) fn loaded_particle_effect_asset_handle(
        &self,
        asset_id: AssetId,
    ) -> Option<&Handle<ParticleEffectDocumentAsset>> {
        find_loaded_dependency_handle_by_asset_id(&self.effects, asset_id)
    }

    pub fn loaded_animation_set_asset_handles(
        &self,
    ) -> impl Iterator<Item = &Handle<AnimationSetAsset>> {
        self.animations.iter().map(|(_, handle)| handle)
    }
}

pub(crate) fn create_scene_prefab_asset_and_load_dependencies(
    document: ScenePrefabDocument,
    load_context: &mut LoadContext<'_>,
) -> ScenePrefabAsset {
    let (model_paths, collider_models) =
        register_scene_prefab_model_dependencies_and_retain_only_collider_models(
            load_context,
            &document,
        );
    let material_paths = collect_scene_prefab_dependency_paths_of_asset_kind(
        &document,
        ScenePrefabAssetDependencyKind::Material,
    );
    let effects = load_scene_prefab_dependencies_of_asset_kind(
        load_context,
        &document,
        ScenePrefabAssetDependencyKind::Effect,
    );
    let animations = load_scene_prefab_dependencies_of_asset_kind(
        load_context,
        &document,
        ScenePrefabAssetDependencyKind::Animation,
    );
    ScenePrefabAsset {
        document,
        model_paths,
        collider_models,
        material_paths,
        effects,
        animations,
    }
}

fn collect_scene_prefab_dependency_paths_of_asset_kind(
    prefab: &ScenePrefabDocument,
    asset_kind: ScenePrefabAssetDependencyKind,
) -> Box<[(AssetId, Box<str>)]> {
    let mut values = prefab
        .dependencies
        .iter()
        .filter(|dependency| dependency.asset_kind == asset_kind)
        .map(|dependency| {
            (
                dependency.asset_id,
                dependency.asset_path.clone().into_boxed_str(),
            )
        })
        .collect::<Vec<_>>();
    values.sort_unstable_by_key(|(id, _)| id.0);
    values.into_boxed_slice()
}

fn register_scene_prefab_model_dependencies_and_retain_only_collider_models(
    load_context: &mut LoadContext<'_>,
    prefab: &ScenePrefabDocument,
) -> (Box<[(AssetId, Box<str>)]>, Box<[(AssetId, Handle<Gltf>)]>) {
    let collider_model_identifiers = prefab
        .entities
        .iter()
        .flat_map(|entity| &entity.colliders)
        .filter_map(|collider| match &collider.source {
            openzt2_game_data::scene_prefab::PrefabColliderSource::Model { model, .. } => {
                Some(*model)
            }
            openzt2_game_data::scene_prefab::PrefabColliderSource::Box { .. }
            | openzt2_game_data::scene_prefab::PrefabColliderSource::Capsule { .. } => None,
        })
        .collect::<std::collections::BTreeSet<_>>();
    let mut model_paths = Vec::new();
    let mut collider_models = Vec::new();
    for dependency in prefab
        .dependencies
        .iter()
        .filter(|dependency| dependency.asset_kind == ScenePrefabAssetDependencyKind::Model)
    {
        model_paths.push((
            dependency.asset_id,
            dependency.asset_path.clone().into_boxed_str(),
        ));
        if collider_model_identifiers.contains(&dependency.asset_id) {
            collider_models.push((
                dependency.asset_id,
                load_context.load::<Gltf>(dependency.asset_path.clone()),
            ));
        }
    }
    model_paths.sort_unstable_by_key(|(id, _)| id.0);
    collider_models.sort_unstable_by_key(|(id, _)| id.0);
    (
        model_paths.into_boxed_slice(),
        collider_models.into_boxed_slice(),
    )
}

fn find_loaded_dependency_handle_by_asset_id<T: Asset>(
    dependencies: &[(AssetId, Handle<T>)],
    asset_id: AssetId,
) -> Option<&Handle<T>> {
    dependencies
        .binary_search_by_key(&asset_id.0, |(candidate, _)| candidate.0)
        .ok()
        .map(|index| &dependencies[index].1)
}

fn load_scene_prefab_dependencies_of_asset_kind<T: Asset>(
    load_context: &mut LoadContext<'_>,
    prefab: &ScenePrefabDocument,
    asset_kind: ScenePrefabAssetDependencyKind,
) -> Box<[(AssetId, Handle<T>)]> {
    let mut values = prefab
        .dependencies
        .iter()
        .filter(|dependency| dependency.asset_kind == asset_kind)
        .map(|dependency| {
            (
                dependency.asset_id,
                load_context.load::<T>(dependency.asset_path.clone()),
            )
        })
        .collect::<Vec<_>>();
    values.sort_unstable_by_key(|(id, _)| id.0);
    values.into_boxed_slice()
}

pub struct ScenePrefabAssetPlugin;

impl Plugin for ScenePrefabAssetPlugin {
    fn build(&self, app: &mut App) {
        app.init_asset::<ScenePrefabAsset>();
    }
}
