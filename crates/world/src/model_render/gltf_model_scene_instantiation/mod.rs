use bevy::ecs::{entity_disabling::Disabled, query::Allow};
use bevy::gltf::Gltf;
use bevy::gltf::GltfMaterialName;
use bevy::gltf::GltfMesh;
use bevy::gltf::GltfNode;
use bevy::prelude::*;
use bevy::world_serialization::WorldInstanceReady;
use openzt2_world_state::world_spawn::prefab_presentation_types::PrefabMaterialOverrides;
use openzt2_world_state::world_spawn::prefab_presentation_types::PrefabModel;
use openzt2_world_state::world_spawn::prefab_presentation_types::PrefabRenderable;

use openzt2_world_state::model_render::ModelExpanded;

use crate::world_spawn::prefab_world_instance_spawning::ReleasePrefabRenderable;

use super::world_entity_model_suppression::{ModelRenderVisibilityGate, ModelRenderingSuppressed};

#[derive(Component)]
#[require(ModelRenderVisibilityGate)]
pub(super) struct ResidentPrefabModel;

#[derive(Component)]
#[relationship(relationship_target = ResidentSceneEntities)]
struct ResidentSceneEntityOf(Entity);

#[derive(Component)]
#[relationship_target(relationship = ResidentSceneEntityOf, linked_spawn)]
struct ResidentSceneEntities(Vec<Entity>);

pub(super) fn bind_prefab_model_scene_ownership_on_ready(
    ready: On<WorldInstanceReady>,
    residents: Query<(), With<ResidentPrefabModel>>,
    world_instances: Res<WorldInstanceSpawner>,
    mut commands: Commands,
) {
    if !residents.contains(ready.entity) {
        return;
    }
    for entity in world_instances.iter_instance_entities(ready.instance_id) {
        commands
            .entity(entity)
            .try_insert(ResidentSceneEntityOf(ready.entity));
    }
}

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
            &GlobalTransform,
            Has<ModelRenderingSuppressed>,
        ),
        (
            With<PrefabRenderable>,
            Without<ModelExpanded>,
            Without<ReleasePrefabRenderable>,
        ),
    >,
) {
    let gltf_assets_changed = gltf_asset_events.read().count() != 0;
    if !gltf_assets_changed
        && !roots
            .iter()
            .any(|(_, model, _, visibility, _, _)| model.is_changed() || visibility.is_changed())
    {
        return;
    }
    for (entity, mut model, mut material_overrides, visibility, transform, suppressed) in &mut roots
    {
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
            let gate =
                spawn_prefab_model_visibility_root(&mut commands, entity, *transform, suppressed);
            commands.spawn((
                Mesh3d(mesh),
                MeshMaterial3d(material),
                GltfMaterialName(material_name),
                ChildOf(gate),
                *transform,
                if suppressed {
                    InheritedVisibility::HIDDEN
                } else {
                    InheritedVisibility::VISIBLE
                },
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
        let gate =
            spawn_prefab_model_visibility_root(&mut commands, entity, *transform, suppressed);
        commands.entity(gate).insert(WorldAssetRoot(scene));
    }
}

fn spawn_prefab_model_visibility_root(
    commands: &mut Commands,
    owner: Entity,
    transform: GlobalTransform,
    suppressed: bool,
) -> Entity {
    commands.entity(owner).insert(ModelExpanded);
    commands
        .spawn((
            ResidentPrefabModel,
            transform,
            if suppressed {
                Visibility::Hidden
            } else {
                Visibility::Inherited
            },
            if suppressed {
                InheritedVisibility::HIDDEN
            } else {
                InheritedVisibility::VISIBLE
            },
            ChildOf(owner),
        ))
        .id()
}

pub(super) fn retire_hidden_prefab_model_assets(
    mut commands: Commands,
    mut models: Query<
        (
            Entity,
            &mut PrefabModel,
            &mut PrefabMaterialOverrides,
            &InheritedVisibility,
            Option<&Children>,
            Has<ModelExpanded>,
            Has<ReleasePrefabRenderable>,
        ),
        (
            With<PrefabRenderable>,
            Allow<Disabled>,
            Or<(
                Changed<InheritedVisibility>,
                Changed<PrefabModel>,
                Added<ReleasePrefabRenderable>,
            )>,
        ),
    >,
    residents: Query<(), With<ResidentPrefabModel>>,
) {
    for (entity, mut model, mut overrides, visibility, children, expanded, rebuilding) in
        &mut models
    {
        if visibility.get() && !rebuilding {
            continue;
        }
        if let Some(children) = children {
            for child in children.iter().filter(|&child| residents.contains(child)) {
                commands.entity(child).try_despawn();
            }
        }
        if model.handle.is_some() {
            model.handle = None;
        }
        for material in &mut overrides.0 {
            material.handle = None;
        }
        if expanded {
            commands.entity(entity).remove::<ModelExpanded>();
        }
    }
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
