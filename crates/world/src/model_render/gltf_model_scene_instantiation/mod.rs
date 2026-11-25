use crate::world_spawn::named_physical_presentation_projection::NamedPresentationNodeAttachment;
use bevy::ecs::{entity_disabling::Disabled, query::Allow};
use bevy::gltf::Gltf;
use bevy::gltf::GltfMaterialName;
use bevy::gltf::GltfMesh;
use bevy::gltf::GltfNode;
use bevy::prelude::*;
use bevy::world_serialization::WorldInstance;
use openzt2_world_state::animation_playback::model_joint_attachment_binding::ModelJointAttachment;
use openzt2_world_state::world_spawn::prefab_presentation_types::PrefabMaterialOverrides;
use openzt2_world_state::world_spawn::prefab_presentation_types::PrefabModel;
use openzt2_world_state::world_spawn::prefab_presentation_types::PrefabRenderable;

use openzt2_world_state::model_render::ModelExpanded;

use crate::world_spawn::prefab_world_instance_spawning::ReleasePrefabRenderable;

use super::world_entity_model_suppression::{ModelRenderVisibilityGate, ModelRenderingSuppressed};

#[derive(Component)]
#[require(ModelRenderVisibilityGate)]
pub(super) struct ResidentPrefabModel;

pub(super) fn retire_removed_prefab_scene_root(
    removed: On<Remove<WorldAssetRoot>>,
    mut commands: Commands,
    spawner: Option<ResMut<WorldInstanceSpawner>>,
    instances: Query<&WorldInstance, (With<ResidentPrefabModel>, Allow<Disabled>)>,
) {
    let (Some(mut spawner), Ok(instance)) = (spawner, instances.get(removed.entity)) else {
        return;
    };
    // The root's removal hook unregisters the instance. Queue its members
    // while the spawner can still identify reparented entities.
    for entity in spawner.iter_instance_entities(**instance) {
        commands.entity(entity).try_despawn();
    }
    spawner.despawn_instance(**instance);
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

#[derive(bevy::ecs::system::SystemParam)]
pub(super) struct SceneExternalAttachments<'w, 's> {
    children: Query<'w, 's, &'static Children>,
    joint_bindings: Query<'w, 's, &'static ModelJointAttachment>,
    binders: Query<'w, 's, &'static NamedPresentationNodeAttachment>,
    binder_visibility:
        Query<'w, 's, &'static mut Visibility, With<NamedPresentationNodeAttachment>>,
}

impl SceneExternalAttachments<'_, '_> {
    fn detach(&mut self, commands: &mut Commands, members: impl IntoIterator<Item = Entity>) {
        for member in members {
            let Ok(children) = self.children.get(member) else {
                continue;
            };
            for attachment in children.iter() {
                if let Ok(binding) = self.joint_bindings.get(attachment) {
                    commands
                        .entity(attachment)
                        .try_insert(ChildOf(binding.instance));
                } else if let Ok(binding) = self.binders.get(attachment) {
                    if let Ok(mut visibility) = self.binder_visibility.get_mut(attachment) {
                        visibility.set_if_neq(Visibility::Hidden);
                    }
                    commands
                        .entity(attachment)
                        .try_insert(ChildOf(binding.owner));
                }
            }
        }
    }
}

pub(super) fn detach_attachments_from_reloading_scenes(
    mut commands: Commands,
    mut assets: MessageReader<AssetEvent<WorldAsset>>,
    spawner: Res<WorldInstanceSpawner>,
    scenes: Query<(&WorldAssetRoot, &WorldInstance), With<ResidentPrefabModel>>,
    mut attachments: SceneExternalAttachments,
) {
    let reloading = assets
        .read()
        .filter_map(|event| match *event {
            AssetEvent::Modified { id } => Some(id),
            _ => None,
        })
        .collect::<bevy::platform::collections::HashSet<_>>();
    if reloading.is_empty() {
        return;
    }
    for (asset, instance) in &scenes {
        if reloading.contains(&asset.0.id()) {
            attachments.detach(&mut commands, spawner.iter_instance_entities(**instance));
        }
    }
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
    residents: Query<Option<&WorldInstance>, (With<ResidentPrefabModel>, Allow<Disabled>)>,
    spawner: Res<WorldInstanceSpawner>,
    mut attachments: SceneExternalAttachments,
) {
    for (entity, mut model, mut overrides, visibility, children, expanded, rebuilding) in
        &mut models
    {
        if visibility.get() && !rebuilding {
            continue;
        }
        if let Some(children) = children {
            for child in children.iter() {
                let Ok(instance) = residents.get(child) else {
                    continue;
                };
                if let Some(instance) = instance {
                    let instance = **instance;
                    attachments.detach(&mut commands, spawner.iter_instance_entities(instance));
                    commands.queue(move |world: &mut World| {
                        world.resource_scope(|world, mut spawner: Mut<WorldInstanceSpawner>| {
                            spawner.despawn_instance_sync(world, &instance);
                        });
                    });
                }
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
