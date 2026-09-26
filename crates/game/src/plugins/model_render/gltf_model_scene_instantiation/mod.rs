use bevy::{
    gltf::{Gltf, GltfMaterialName, GltfMesh, GltfNode},
    prelude::*,
    world_serialization::WorldInstance,
};

use crate::{
    assets::material::runtime::effect_pass_gpu_data::EffectPassMaterial,
    plugins::world_spawn::{
        prefab_presentation_types::PrefabMaterialOverrides, prefab_presentation_types::PrefabModel,
        prefab_presentation_types::PrefabRenderable,
    },
};

use super::{
    authored_model_material_pass_projection::{
        AdditionalAuthoredModelMaterialPass, AuthoredModelMaterialPassesProjected,
    },
    ModelExpanded,
};

#[derive(Component)]
pub(super) struct HiddenPrefabModelWorldAssetInstanceReleasePending;

pub(super) fn instantiate_visible_gltf_model_scenes(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    models: Res<Assets<Gltf>>,
    nodes: Res<Assets<GltfNode>>,
    meshes: Res<Assets<GltfMesh>>,
    mut gltf_asset_events: MessageReader<AssetEvent<Gltf>>,
    mut roots: Query<
        (
            Entity,
            Mut<PrefabModel>,
            &mut PrefabMaterialOverrides,
            Ref<InheritedVisibility>,
        ),
        (With<PrefabRenderable>, Without<ModelExpanded>),
    >,
) {
    let gltf_assets_changed = gltf_asset_events.read().next().is_some();
    if !gltf_assets_changed
        && !roots
            .iter()
            .any(|(_, model, _, visibility)| model.is_changed() || visibility.is_changed())
    {
        return;
    }
    for (entity, mut model, mut material_overrides, visibility) in &mut roots {
        if !visibility.get() {
            continue;
        }
        if model.handle.is_none() {
            model.handle = Some(asset_server.load(model.model_path.to_string()));
        }
        for material_override in &mut material_overrides.0 {
            if material_override.handle.is_none() {
                // A physical .bfmat filename may contain '#'. Only generated
                // native-material references use Bevy's subasset delimiter.
                let material_path = material_override.material_path.as_ref();
                let path = if material_path.to_ascii_lowercase().ends_with(".bfmat") {
                    bevy::asset::AssetPath::from_path_buf(material_path.into())
                } else {
                    bevy::asset::AssetPath::from(material_path.to_owned())
                };
                material_override.handle = Some(asset_server.load(path));
            }
        }
        let Some(handle) = model.handle.as_ref() else {
            continue;
        };
        let Some(gltf) = models.get(handle) else {
            continue;
        };
        if let Some((mesh, material, material_name)) =
            directly_projectable_static_primitive(&model, gltf, &nodes, &meshes, &asset_server)
        {
            commands.entity(entity).insert((
                Mesh3d(mesh),
                MeshMaterial3d(material),
                GltfMaterialName(material_name),
                ModelExpanded,
            ));
            continue;
        }
        let scene = model.scene_name.as_deref().map_or_else(
            || {
                gltf.default_scene
                    .clone()
                    .or_else(|| gltf.scenes.first().cloned())
            },
            |name| gltf.named_scenes.get(name).cloned(),
        );
        let Some(scene) = scene else {
            continue;
        };
        commands
            .entity(entity)
            .insert((WorldAssetRoot(scene), ModelExpanded));
    }
}

pub(super) fn complete_hidden_prefab_model_asset_release_after_world_instance_despawn(
    mut commands: Commands,
    mut pending: Query<
        (Entity, &mut PrefabModel, &mut PrefabMaterialOverrides),
        With<HiddenPrefabModelWorldAssetInstanceReleasePending>,
    >,
    children: Query<&Children>,
    additional_material_passes: Query<Entity, With<AdditionalAuthoredModelMaterialPass>>,
) {
    for (entity, mut model, mut material_overrides) in &mut pending {
        release_hidden_prefab_model_assets_and_render_components(
            &mut commands,
            entity,
            &mut model,
            &mut material_overrides,
            &children,
            &additional_material_passes,
        );
    }
}

pub(super) fn request_hidden_prefab_model_asset_release(
    mut commands: Commands,
    mut world_instance_spawner: ResMut<WorldInstanceSpawner>,
    mut hidden_models: Query<
        (
            Entity,
            &mut PrefabModel,
            &mut PrefabMaterialOverrides,
            &InheritedVisibility,
            Option<&WorldInstance>,
        ),
        (
            With<ModelExpanded>,
            Without<HiddenPrefabModelWorldAssetInstanceReleasePending>,
        ),
    >,
    children: Query<&Children>,
    additional_material_passes: Query<Entity, With<AdditionalAuthoredModelMaterialPass>>,
) {
    for (entity, mut model, mut material_overrides, visibility, world_instance) in
        &mut hidden_models
    {
        if visibility.get() {
            continue;
        }
        if let Some(world_instance) = world_instance {
            world_instance_spawner.despawn_instance(**world_instance);
            commands
                .entity(entity)
                .insert(HiddenPrefabModelWorldAssetInstanceReleasePending);
            continue;
        }
        release_hidden_prefab_model_assets_and_render_components(
            &mut commands,
            entity,
            &mut model,
            &mut material_overrides,
            &children,
            &additional_material_passes,
        );
    }
}

fn release_hidden_prefab_model_assets_and_render_components(
    commands: &mut Commands,
    entity: Entity,
    model: &mut PrefabModel,
    material_overrides: &mut PrefabMaterialOverrides,
    children: &Query<&Children>,
    additional_material_passes: &Query<Entity, With<AdditionalAuthoredModelMaterialPass>>,
) {
    model.handle = None;
    for material_override in &mut material_overrides.0 {
        material_override.handle = None;
    }
    for descendant in children.iter_descendants_depth_first::<Children>(entity) {
        if additional_material_passes.contains(descendant) {
            commands.entity(descendant).despawn();
        }
    }
    commands.entity(entity).remove::<(
        WorldAssetRoot,
        WorldInstance,
        Mesh3d,
        MeshMaterial3d<StandardMaterial>,
        MeshMaterial3d<EffectPassMaterial>,
        GltfMaterialName,
        ModelExpanded,
        AuthoredModelMaterialPassesProjected,
        HiddenPrefabModelWorldAssetInstanceReleasePending,
    )>();
}

fn directly_projectable_static_primitive(
    model: &PrefabModel,
    gltf: &Gltf,
    nodes: &Assets<GltfNode>,
    meshes: &Assets<GltfMesh>,
    asset_server: &AssetServer,
) -> Option<(Handle<Mesh>, Handle<StandardMaterial>, String)> {
    if !gltf.animations.is_empty() || !gltf.skins.is_empty() {
        return None;
    }
    let node_handle = match model.scene_name.as_deref() {
        Some(name) => gltf.named_nodes.get(name),
        None if gltf.scenes.len() == 1 && gltf.nodes.len() == 1 => gltf.nodes.first(),
        None => None,
    }?;
    let node = nodes.get(node_handle)?;
    if node.transform != Transform::IDENTITY || node.skin.is_some() || !node.children.is_empty() {
        return None;
    }
    let mesh = meshes.get(node.mesh.as_ref()?)?;
    let primitive = mesh
        .primitives
        .first()
        .filter(|_| mesh.primitives.len() == 1)?;
    let material = primitive.material.as_ref()?;
    let material_name = gltf
        .named_materials
        .iter()
        .find_map(|(name, handle)| (handle == material).then(|| name.to_string()))?;
    let standard_material_path = material
        .path()?
        .clone()
        .with_label(format!("{}/std", material.path()?.label()?));
    let standard_material = asset_server.get_handle(standard_material_path)?;
    Some((primitive.mesh.clone(), standard_material, material_name))
}
