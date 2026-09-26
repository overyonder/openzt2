//! Assembly of scene-prefab entities and dependency tables.

use std::{collections::BTreeMap, path::Path};

use openzt2_game_data::{
    scene_prefab::{
        PrefabBillboard, PrefabCollider, PrefabColliderSource, PrefabEffect, PrefabEntity,
        PrefabLight, PrefabLod, PrefabRenderable, PrefabRenderableMaterialOverride,
        PrefabRotationCycle, PrefabTransform, PrefabTransformAnimation, ScenePrefabAssetDependency,
        ScenePrefabAssetDependencyKind, ScenePrefabColliderLayerFlags, ScenePrefabDocument,
        ScenePrefabEntityFlags, ScenePrefabRenderableVisibilityFlags,
    },
    AssetId,
};

use super::native_scene_lowering_error::NativeSceneLoweringError;

type Result<T> = std::result::Result<T, NativeSceneLoweringError>;

struct PendingLod {
    child: i32,
    group: u32,
    ordinal: u16,
    center_m: [f32; 3],
    near_m: f32,
    far_m: f32,
    active_without_range: bool,
}

pub(super) struct ScenePrefabDocumentAssembler {
    model_path: String,
    entities: Vec<PrefabEntity>,
    entity_keys: Vec<String>,
    names: BTreeMap<String, u32>,
    nif_entities: BTreeMap<i32, u32>,
    renderables: Vec<(u32, PrefabRenderable)>,
    colliders: Vec<(u32, PrefabCollider)>,
    rotation_cycles: Vec<(u32, PrefabRotationCycle)>,
    billboards: Vec<(u32, PrefabBillboard)>,
    lights: Vec<(u32, PrefabLight)>,
    effects: Vec<(u32, PrefabEffect)>,
    lods: Vec<(u32, PrefabLod)>,
    pending_lods: Vec<PendingLod>,
    dependencies: BTreeMap<(AssetId, u8), ScenePrefabAssetDependency>,
}

impl ScenePrefabDocumentAssembler {
    pub(super) fn new(model_path: &str) -> Self {
        Self {
            model_path: model_path.to_owned(),
            entities: Vec::new(),
            entity_keys: Vec::new(),
            names: BTreeMap::new(),
            nif_entities: BTreeMap::new(),
            renderables: Vec::new(),
            colliders: Vec::new(),
            rotation_cycles: Vec::new(),
            billboards: Vec::new(),
            lights: Vec::new(),
            effects: Vec::new(),
            lods: Vec::new(),
            pending_lods: Vec::new(),
            dependencies: BTreeMap::new(),
        }
    }
    pub(super) fn entity(
        &mut self,
        key: &str,
        parent_key: Option<&str>,
        transform: PrefabTransform,
        visible: bool,
    ) -> Result<u32> {
        self.entity_with_distinct_stable_and_authored_attachment_keys(
            key, key, parent_key, transform, visible,
        )
    }

    pub(super) fn entity_with_distinct_stable_and_authored_attachment_keys(
        &mut self,
        stable_key: &str,
        authored_attachment_key: &str,
        parent_stable_key: Option<&str>,
        transform: PrefabTransform,
        visible: bool,
    ) -> Result<u32> {
        let parent = parent_stable_key
            .and_then(|key| self.names.get(key).copied())
            .unwrap_or(u32::MAX);
        let index = u32::try_from(self.entities.len()).map_err(|_| {
            NativeSceneLoweringError::new(
                &self.model_path,
                "scene contains more than u32::MAX entities",
            )
        })?;
        let normalized_authored_attachment_key =
            authored_attachment_key.trim().to_ascii_lowercase();
        self.entities.push(PrefabEntity {
            stable_id: AssetId::from_key(stable_key),
            attachment_id: AssetId::from_key(&normalized_authored_attachment_key),
            model_joint_binding: None,
            parent,
            transform,
            children: Vec::new(),
            flags: if visible {
                ScenePrefabEntityFlags::VISIBLE | ScenePrefabEntityFlags::ACTIVE
            } else {
                ScenePrefabEntityFlags::ACTIVE
            },
            rotation_cycles: Vec::new(),
            transform_animations: Vec::new(),
            renderables: Vec::new(),
            colliders: Vec::new(),
            lods: Vec::new(),
            billboards: Vec::new(),
            lights: Vec::new(),
            effects: Vec::new(),
        });
        self.entity_keys.push(stable_key.to_owned());
        self.names.insert(stable_key.to_owned(), index);
        if parent != u32::MAX {
            self.entities[parent as usize].children.push(index);
        }
        Ok(index)
    }
    pub(super) fn anonymous_child(
        &mut self,
        parent: u32,
        transform: PrefabTransform,
    ) -> Result<u32> {
        self.entity(
            &format!(
                "{}.__collision/{}",
                self.entity_keys[parent as usize],
                self.entities.len()
            ),
            Some(&self.entity_keys[parent as usize].clone()),
            transform,
            true,
        )
    }
    pub(super) fn entity_index(&self, key: &str) -> Option<u32> {
        self.names.get(key).copied()
    }
    pub(super) fn is_empty(&self) -> bool {
        self.entities.is_empty()
    }
    pub(super) fn entity_key(&self, entity: u32) -> &str {
        &self.entity_keys[entity as usize]
    }
    pub(super) fn contains_netimmerse_entity(&self, block: i32) -> bool {
        self.nif_entities.contains_key(&block)
    }
    pub(super) fn remember_netimmerse_entity(&mut self, block: i32, entity: u32) {
        self.nif_entities.insert(block, entity);
    }
    pub(super) fn renderable(&mut self, entity: u32, scene_name: String) {
        let model_path = self.model_path.clone();
        self.renderable_from(entity, &model_path, scene_name);
    }
    pub(super) fn renderable_from(&mut self, entity: u32, model_path: &str, scene_name: String) {
        let model = AssetId::from_virtual_path(model_path);
        self.renderables.push((
            entity,
            PrefabRenderable {
                model,
                scene_name,
                material_overrides: Vec::new(),
                // Native model formats do not author a mesh-shadow flag.
                // Blue Fang entities attach their separate blob or projected
                // shadow component at the binder layer when one is wanted.
                visibility: ScenePrefabRenderableVisibilityFlags::VISIBLE
                    | ScenePrefabRenderableVisibilityFlags::RECEIVE_SHADOW
                    | ScenePrefabRenderableVisibilityFlags::REFLECTION_VISIBLE,
            },
        ));
        self.dependency(
            model,
            model_path.to_owned(),
            ScenePrefabAssetDependencyKind::Model,
        );
    }
    pub(super) fn blue_fang_model_material_override(&mut self, material_name: &str) {
        let material = AssetId::from_key(material_name);
        let material_file_name = if material_name.to_ascii_lowercase().ends_with(".bfmat") {
            material_name.to_owned()
        } else {
            format!("{material_name}.bfmat")
        };
        let path = Path::new(&self.model_path)
            .parent()
            .unwrap_or_else(|| Path::new(""))
            .join("Materials")
            .join(material_file_name)
            .to_string_lossy()
            .into_owned();
        self.material_override_from(material, path);
    }
    pub(super) fn material_override_from(&mut self, material: AssetId, path: String) {
        self.renderables
            .last_mut()
            .expect("material override follows its renderable")
            .1
            .material_overrides
            .push(PrefabRenderableMaterialOverride { slot: 0, material });
        self.dependency(material, path, ScenePrefabAssetDependencyKind::Material);
    }
    pub(super) fn collider(&mut self, entity: u32, source: PrefabColliderSource) {
        self.colliders.push((
            entity,
            PrefabCollider {
                source,
                layer: ScenePrefabColliderLayerFlags::WORLD,
                mask: ScenePrefabColliderLayerFlags::WORLD
                    | ScenePrefabColliderLayerFlags::ANIMAL
                    | ScenePrefabColliderLayerFlags::GUEST
                    | ScenePrefabColliderLayerFlags::STAFF
                    | ScenePrefabColliderLayerFlags::PLACEMENT,
            },
        ));
    }
    pub(super) fn rotation_cycle(&mut self, entity: u32, rotation_cycle: PrefabRotationCycle) {
        self.rotation_cycles.push((entity, rotation_cycle));
    }
    pub(super) fn transform_animation(
        &mut self,
        entity: u32,
        transform_animation: PrefabTransformAnimation,
    ) {
        self.entities[entity as usize]
            .transform_animations
            .push(transform_animation);
    }
    pub(super) fn billboard(&mut self, entity: u32, billboard: PrefabBillboard) {
        self.billboards.push((entity, billboard));
    }
    pub(super) fn light(&mut self, entity: u32, light: PrefabLight) {
        self.lights.push((entity, light));
    }
    pub(super) fn effect(&mut self, entity: u32, effect: PrefabEffect) {
        self.effects.push((entity, effect));
    }

    pub(super) fn bind_model_joint(&mut self, entity: u32, joint: &str) {
        self.entities[entity as usize].model_joint_binding = Some(joint.to_owned());
    }
    pub(super) fn lod(&mut self, entity: u32, lod: PrefabLod) {
        self.lods.push((entity, lod));
    }
    pub(super) fn pending_netimmerse_lod(
        &mut self,
        child: i32,
        group: u32,
        ordinal: u16,
        center_m: [f32; 3],
        near_m: f32,
        far_m: f32,
        active_without_range: bool,
    ) {
        self.pending_lods.push(PendingLod {
            child,
            group,
            ordinal,
            center_m,
            near_m,
            far_m,
            active_without_range,
        });
    }
    pub(super) fn dependency(
        &mut self,
        asset_id: AssetId,
        asset_path: String,
        asset_kind: ScenePrefabAssetDependencyKind,
    ) {
        self.dependencies.insert(
            (asset_id, asset_kind as u8),
            ScenePrefabAssetDependency {
                asset_id,
                asset_path,
                asset_kind,
            },
        );
    }
    pub(super) fn finish(mut self) -> Result<ScenePrefabDocument> {
        let pending_lods = std::mem::take(&mut self.pending_lods);
        let mut lods = pending_lods
            .into_iter()
            .map(|lod| {
                self.nif_entities
                    .get(&lod.child)
                    .copied()
                    .map(|entity| {
                        (
                            entity,
                            PrefabLod {
                                group_entity: lod.group,
                                ordinal: lod.ordinal,
                                center_m: lod.center_m,
                                near_m: lod.near_m,
                                far_m: lod.far_m,
                                active_without_range: lod.active_without_range,
                            },
                        )
                    })
                    .ok_or_else(|| {
                        NativeSceneLoweringError::new(
                            &self.model_path,
                            format!("NIF LOD references non-scene child {}", lod.child),
                        )
                    })
            })
            .collect::<Result<Vec<_>>>()?;
        lods.append(&mut self.lods);
        for (entity, value) in self.rotation_cycles {
            self.entities[entity as usize].rotation_cycles.push(value);
        }
        for (entity, value) in self.renderables {
            self.entities[entity as usize].renderables.push(value);
        }
        for (entity, value) in self.colliders {
            self.entities[entity as usize].colliders.push(value);
        }
        for (entity, value) in lods {
            self.entities[entity as usize].lods.push(value);
        }
        for (entity, value) in self.billboards {
            self.entities[entity as usize].billboards.push(value);
        }
        for (entity, value) in self.lights {
            self.entities[entity as usize].lights.push(value);
        }
        for (entity, value) in self.effects {
            self.entities[entity as usize].effects.push(value);
        }
        Ok(ScenePrefabDocument {
            entities: self.entities,
            automatic_placement_bounds_xz: None,
            rail_camera: None,
            dependencies: self.dependencies.into_values().collect(),
        })
    }
}
