use bevy::{
    ecs::system::SystemParam, gltf::Gltf, platform::collections::HashMap, prelude::*,
    world_serialization::WorldAsset,
};
use openzt2_game_assets::scene_prefab::ScenePrefabAsset;
use openzt2_game_data::world_definitions::object_placement::PlaceableDefinition;
use openzt2_world_state::world_spawn::prefab_transform_conversion::transform_from_authored;

#[derive(SystemParam)]
pub(super) struct PlacementModelHeadroom<'w, 's> {
    server: Option<Res<'w, AssetServer>>,
    models: Res<'w, Assets<Gltf>>,
    scenes: Option<Res<'w, Assets<WorldAsset>>>,
    meshes: Option<Res<'w, Assets<Mesh>>>,
    dependencies: Local<'s, HashMap<Box<str>, Handle<Gltf>>>,
}

impl PlacementModelHeadroom<'_, '_> {
    pub(super) fn request_models(
        &mut self,
        definition: &PlaceableDefinition,
        prefab: &ScenePrefabAsset,
    ) {
        if definition.minimum_headroom_metres < 0.0 || definition.apply_height_modifier {
            return;
        }
        let Some(server) = &self.server else {
            return;
        };
        for renderable in prefab
            .canonical_scene_prefab_document()
            .entities
            .iter()
            .flat_map(|entity| &entity.renderables)
        {
            if prefab.loaded_model_asset_handle(renderable.model).is_none() {
                if let Some(path) = prefab.model_asset_path(renderable.model) {
                    if !self.dependencies.contains_key(path) {
                        self.dependencies
                            .insert(path.into(), server.load(path.to_owned()));
                    }
                }
            }
        }
    }

    pub(super) fn assets_changed(&self) -> bool {
        self.models.is_changed()
            || self
                .scenes
                .as_ref()
                .is_some_and(|assets| assets.is_changed())
            || self
                .meshes
                .as_ref()
                .is_some_and(|assets| assets.is_changed())
    }

    /// Negative authored clearance disables the check. Placement without an
    /// authored height modifier uses the selected model's full vertical extent.
    pub(super) fn clearance(
        &self,
        definition: &PlaceableDefinition,
        prefab: Option<&ScenePrefabAsset>,
        pose: &Transform,
    ) -> Option<f32> {
        if definition.minimum_headroom_metres < 0.0 {
            return Some(0.0);
        }
        if definition.apply_height_modifier {
            return Some(definition.minimum_headroom_metres);
        }
        let prefab = prefab?;
        let scenes = self.scenes.as_ref()?;
        let meshes = self.meshes.as_ref()?;
        let entries = &prefab.canonical_scene_prefab_document().entities;
        let placement =
            Mat4::from_scale_rotation_translation(pose.scale, pose.rotation, Vec3::ZERO);
        let mut bounds = None::<(f32, f32)>;
        for (index, entry) in entries.iter().enumerate() {
            if entry.renderables.is_empty() {
                continue;
            }
            let mut owner = index;
            let mut local = Mat4::IDENTITY;
            for _ in 0..entries.len() {
                let node = entries.get(owner)?;
                let transform = transform_from_authored(&node.transform);
                local = Mat4::from_scale_rotation_translation(
                    transform.scale,
                    transform.rotation,
                    transform.translation,
                ) * local;
                if node.parent == u32::MAX {
                    break;
                }
                owner = node.parent as usize;
            }
            for renderable in &entry.renderables {
                let handle = prefab
                    .loaded_model_asset_handle(renderable.model)
                    .cloned()
                    .or_else(|| {
                        self.server
                            .as_ref()?
                            .get_handle(prefab.model_asset_path(renderable.model)?.to_owned())
                    })?;
                let model = self.models.get(&handle)?;
                let scene = if renderable.scene_name.is_empty() {
                    model
                        .default_scene
                        .as_ref()
                        .or_else(|| model.scenes.first())?
                } else {
                    model.named_scenes.get(renderable.scene_name.as_str())?
                };
                let scene = &scenes.get(scene)?.world;
                for entity in scene.iter_entities() {
                    let Some(mesh) = entity.get::<Mesh3d>() else {
                        continue;
                    };
                    let mut transform = Mat4::IDENTITY;
                    let mut cursor = entity.id();
                    loop {
                        if let Some(node) = scene.get::<Transform>(cursor) {
                            transform = Mat4::from_scale_rotation_translation(
                                node.scale,
                                node.rotation,
                                node.translation,
                            ) * transform;
                        }
                        let Some(parent) = scene.get::<ChildOf>(cursor) else {
                            break;
                        };
                        cursor = parent.parent();
                    }
                    let transform = placement * local * transform;
                    let positions = meshes
                        .get(&mesh.0)?
                        .attribute(Mesh::ATTRIBUTE_POSITION)?
                        .as_float3()?;
                    for position in positions {
                        let height = transform.transform_point3(Vec3::from_array(*position)).y;
                        if height.is_finite() {
                            bounds = Some(bounds.map_or((height, height), |(bottom, top)| {
                                (bottom.min(height), top.max(height))
                            }));
                        }
                    }
                }
            }
        }
        Some(bounds.map_or(0.0, |(bottom, top)| top - bottom))
    }
}
