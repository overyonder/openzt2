//! Scene-prefab asset registration.

mod authored_physical_scene_loading;
pub mod transform_animation_lowering;

use bevy::{
    asset::{AssetApp, LoadContext},
    gltf::Gltf,
    prelude::*,
};
use openzt2_game_data::{
    scene_prefab::{
        PrefabAnimationStart, PrefabRenderLodPolicy, PrefabRenderLodSource, PrefabRenderable,
        ScenePrefabAssetDependencyKind, ScenePrefabDocument,
    },
    AssetId,
};
use std::collections::BTreeMap;

use crate::{
    animation::animation_set_asset_types::AnimationSetAsset, effect::ParticleEffectDocumentAsset,
};
use transform_animation_lowering::{lower_prefab_motion, PrefabMotion};

pub fn select_prefab_render_lod_tier(
    policy: &PrefabRenderLodPolicy,
    distance_squared: f32,
    threshold_scale_squared: f32,
    switch_overrides_squared: &[f32],
    minimum_overrides_squared: &[f32],
) -> u16 {
    if !distance_squared.is_finite() || !threshold_scale_squared.is_finite() {
        return policy.initial_tier;
    }
    let threshold = |tier, overrides: &[f32], defaults: &[f32]| {
        overrides
            .get(tier)
            .or_else(|| defaults.get(tier))
            .copied()
            .unwrap_or(0.0)
    };
    (0..policy.tier_count)
        .rev()
        .find(|&tier| {
            let index = usize::from(tier);
            let scaled = threshold(
                index,
                switch_overrides_squared,
                &policy.switch_distance_squared,
            ) * threshold_scale_squared;
            let minimum = threshold(
                index,
                minimum_overrides_squared,
                &policy.minimum_switch_distance_squared,
            );
            distance_squared >= scaled.max(minimum)
        })
        .unwrap_or_else(|| match policy.source {
            PrefabRenderLodSource::BlueFang => 0,
            PrefabRenderLodSource::NetImmerseBone => policy.tier_count.saturating_sub(1),
        })
}

#[derive(Asset, TypePath, Debug)]
pub struct ScenePrefabAsset {
    document: ScenePrefabDocument,
    model_source_digest: Option<[u8; 32]>,
    attachment_transforms: BTreeMap<AssetId, Option<Mat4>>,
    model_paths: Box<[(AssetId, Box<str>)]>,
    collider_models: Box<[(AssetId, Handle<Gltf>)]>,
    material_paths: Box<[(AssetId, Box<str>)]>,
    effects: Box<[(AssetId, Handle<ParticleEffectDocumentAsset>)]>,
    #[dependency]
    animations: BTreeMap<usize, Vec<Option<ScenePrefabAnimationBinding>>>,
    #[dependency]
    motion: Vec<PrefabMotion>,
    ground_fit_height_offset_m: Option<f32>,
    #[dependency]
    terrain_decal: Option<ScenePrefabTerrainDecal>,
}

#[derive(Debug, bevy::asset::VisitAssetDependencies)]
struct ScenePrefabAnimationBinding {
    #[dependency]
    set: Handle<AnimationSetAsset>,
    start: Option<PrefabAnimationStart>,
}

#[derive(Debug, bevy::asset::VisitAssetDependencies)]
pub struct ScenePrefabTerrainDecal {
    pub texture: AssetId,
    pub lighting_colors: [Option<[u8; 3]>; 4],
    #[dependency]
    pub image: Handle<Image>,
}

impl ScenePrefabAsset {
    #[cfg(any(test, feature = "testing"))]
    pub fn from_test_document(document: ScenePrefabDocument) -> Self {
        Self {
            attachment_transforms: collect_prefab_attachment_transforms(&document),
            document,
            model_source_digest: None,
            model_paths: Box::new([]),
            collider_models: Box::new([]),
            material_paths: Box::new([]),
            effects: Box::new([]),
            animations: BTreeMap::new(),
            motion: Vec::new(),
            ground_fit_height_offset_m: None,
            terrain_decal: None,
        }
    }

    pub fn canonical_scene_prefab_document(&self) -> &ScenePrefabDocument {
        &self.document
    }

    /// Identifies the source content of a prefab published by a model loader.
    pub fn model_source_digest(&self) -> Option<[u8; 32]> {
        self.model_source_digest
    }

    pub fn with_model_source_digest(mut self, digest: [u8; 32]) -> Self {
        self.model_source_digest = Some(digest);
        self
    }

    pub fn prefab_attachment_transform(&self, attachment_id: AssetId) -> Option<Mat4> {
        if attachment_id == AssetId::default() {
            return Some(Mat4::IDENTITY);
        }
        self.attachment_transforms
            .get(&attachment_id)
            .copied()
            .flatten()
    }

    pub fn loaded_model_asset_handle(&self, asset_id: AssetId) -> Option<&Handle<Gltf>> {
        find_loaded_dependency_handle_by_asset_id(&self.collider_models, asset_id)
    }

    pub fn model_asset_path(&self, asset_id: AssetId) -> Option<&str> {
        self.model_paths
            .binary_search_by_key(&asset_id.0, |(candidate, _)| candidate.0)
            .ok()
            .map(|index| self.model_paths[index].1.as_ref())
    }

    pub fn material_asset_path(&self, asset_id: AssetId) -> Option<&str> {
        self.material_paths
            .binary_search_by_key(&asset_id.0, |(candidate, _)| candidate.0)
            .ok()
            .map(|index| self.material_paths[index].1.as_ref())
    }

    pub fn loaded_particle_effect_asset_handle(
        &self,
        asset_id: AssetId,
    ) -> Option<&Handle<ParticleEffectDocumentAsset>> {
        find_loaded_dependency_handle_by_asset_id(&self.effects, asset_id)
    }

    pub fn ground_fit_height_offset_m(&self) -> Option<f32> {
        self.ground_fit_height_offset_m
    }

    pub fn terrain_decal_texture(&self, texture: AssetId) -> Option<&Handle<Image>> {
        self.terrain_decal
            .as_ref()
            .filter(|decal| decal.texture == texture)
            .map(|decal| &decal.image)
    }

    pub fn terrain_decal_lighting(&self, texture: AssetId) -> Option<&[Option<[u8; 3]>; 4]> {
        self.terrain_decal
            .as_ref()
            .filter(|decal| decal.texture == texture)
            .map(|decal| &decal.lighting_colors)
    }

    pub fn bind_animation_set_to_renderable(
        &mut self,
        row: usize,
        renderable: usize,
        animation_set: Handle<AnimationSetAsset>,
        start: Option<PrefabAnimationStart>,
    ) {
        let bindings = self.animations.entry(row).or_default();
        bindings.resize_with(self.document.entities[row].renderables.len(), || None);
        bindings[renderable] = Some(ScenePrefabAnimationBinding {
            set: animation_set,
            start,
        });
    }

    fn renderable_index(&self, row: usize, renderable: &PrefabRenderable) -> Option<usize> {
        self.document
            .entities
            .get(row)?
            .renderables
            .iter()
            .position(|candidate| std::ptr::eq(candidate, renderable))
    }

    pub fn loaded_animation_set_asset_handles_for_renderable(
        &self,
        row: usize,
        renderable: &PrefabRenderable,
    ) -> impl Iterator<Item = &Handle<AnimationSetAsset>> {
        self.renderable_index(row, renderable)
            .and_then(|index| self.animations.get(&row)?.get(index)?.as_ref())
            .map(|binding| &binding.set)
            .into_iter()
    }

    pub fn authored_animation_start_for_renderable(
        &self,
        row: usize,
        renderable: &openzt2_game_data::scene_prefab::PrefabRenderable,
    ) -> Option<&openzt2_game_data::scene_prefab::PrefabAnimationStart> {
        let index = self.renderable_index(row, renderable)?;
        match self
            .animations
            .get(&row)
            .and_then(|bindings| bindings.get(index)?.as_ref())
        {
            Some(binding) => binding.start.as_ref(),
            None => self.document.entities[row].animation_start.as_ref(),
        }
    }

    pub fn motion_for_row(&self, index: usize) -> Option<&PrefabMotion> {
        let position = self
            .motion
            .binary_search_by_key(&index, |motion| motion.row)
            .ok()?;
        self.motion.get(position)
    }
}

pub fn create_scene_prefab_asset_and_load_dependencies(
    mut document: ScenePrefabDocument,
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
    let motion = document
        .entities
        .iter_mut()
        .enumerate()
        .filter_map(|(index, row)| lower_prefab_motion(index, row, load_context))
        .collect();
    ScenePrefabAsset {
        attachment_transforms: collect_prefab_attachment_transforms(&document),
        document,
        model_source_digest: None,
        model_paths,
        collider_models,
        material_paths,
        effects,
        animations: BTreeMap::new(),
        motion,
        ground_fit_height_offset_m: None,
        terrain_decal: None,
    }
}

fn collect_prefab_attachment_transforms(
    prefab: &ScenePrefabDocument,
) -> BTreeMap<AssetId, Option<Mat4>> {
    let mut attachments = BTreeMap::new();
    for (index, entity) in prefab.entities.iter().enumerate() {
        if entity.attachment_id == AssetId::default() {
            continue;
        }
        attachments.entry(entity.attachment_id).or_insert_with(|| {
            let mut index = index;
            let mut transform = Mat4::IDENTITY;
            for _ in 0..prefab.entities.len() {
                let entity = prefab.entities.get(index)?;
                let local = &entity.transform;
                transform = Mat4::from_scale_rotation_translation(
                    Vec3::from_array(local.scale),
                    Quat::from_array(local.rotation_xyzw),
                    Vec3::from_array(local.translation_m),
                ) * transform;
                if entity.parent == u32::MAX {
                    return Some(transform);
                }
                index = entity.parent as usize;
            }
            None
        });
    }
    attachments
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
            | openzt2_game_data::scene_prefab::PrefabColliderSource::Capsule { .. }
            | openzt2_game_data::scene_prefab::PrefabColliderSource::HalfSpace { .. }
            | openzt2_game_data::scene_prefab::PrefabColliderSource::ConvexHull { .. } => None,
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
        app.init_asset::<ScenePrefabAsset>()
            .init_asset_loader::<authored_physical_scene_loading::AuthoredPhysicalSceneLoader>();
    }
}
