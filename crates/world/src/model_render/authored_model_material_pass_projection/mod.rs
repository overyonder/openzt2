use super::configured_model_effect_materials::{
    AuthoredModelEffectMaterial, ModelMaterialInputOwner,
};
use super::model_scene_visibility_and_shadow_projection::ProjectedModelDrawShadowPolicy;
use bevy::asset::{asset_changed::AssetChanged, AsAssetId, AssetEvent};
use bevy::camera::visibility::RenderLayers;
use bevy::gltf::GltfMaterialName;
use bevy::light::NotShadowCaster;
use bevy::light::NotShadowReceiver;
use bevy::mesh::morph::MeshMorphWeights;
use bevy::mesh::skinning::SkinnedMesh;
use bevy::platform::collections::{HashMap, HashSet};
use bevy::prelude::*;
use openzt2_game_assets::graphics_settings_types::GraphicsSettings;
use openzt2_game_assets::material::material_asset_types::MaterialAsset;
use openzt2_game_assets::material::runtime::effect_pass_gpu_data::EffectPassMaterial;
use openzt2_game_assets::material::runtime::texture_coordinate_animation::GeometryTextureCoordinateAnimationClock;
use openzt2_world_state::model_render::authored_effect_technique_pass_submission_order::AuthoredEffectTechniquePassSubmissionOrder;
use openzt2_world_state::model_render::ModelExpanded;
use openzt2_world_state::world_spawn::prefab_presentation_types::PrefabClothingTextures;
use openzt2_world_state::world_spawn::prefab_presentation_types::PrefabMaterialOverrides;
use openzt2_world_state::world_spawn::prefab_presentation_types::PrefabPrimitiveMaterial;

pub(super) fn initialize_model_geometry_texture_coordinate_clocks(
    mut commands: Commands,
    time: Res<Time>,
    geometries: Query<
        Entity,
        (
            With<Mesh3d>,
            With<GltfMaterialName>,
            Or<(Added<Mesh3d>, Added<GltfMaterialName>)>,
            Without<GeometryTextureCoordinateAnimationClock>,
        ),
    >,
) {
    let started = time.elapsed_secs();
    for entity in &geometries {
        commands
            .entity(entity)
            .insert(GeometryTextureCoordinateAnimationClock::new(started));
    }
}

#[derive(Component)]
pub(super) struct AdditionalAuthoredModelMaterialPass(usize);

#[derive(Component)]
pub(super) struct AuthoredModelStandardMaterial {
    pub(super) source: Handle<StandardMaterial>,
    pub(super) rendered_material: bevy::asset::AssetId<StandardMaterial>,
    pub(super) fading: bool,
}

impl AsAssetId for AuthoredModelStandardMaterial {
    type Asset = StandardMaterial;

    fn as_asset_id(&self) -> bevy::asset::AssetId<StandardMaterial> {
        self.source.id()
    }
}

#[derive(Component)]
pub(super) struct PendingAuthoredModelMaterialPasses(
    Box<[bevy::asset::AssetId<EffectPassMaterial>]>,
);

#[derive(bevy::ecs::system::SystemParam)]
pub(super) struct AuthoredPassInvalidation<'w, 's> {
    changed: Query<
        'w,
        's,
        Entity,
        (
            With<GltfMaterialName>,
            Without<AdditionalAuthoredModelMaterialPass>,
            Or<(
                Changed<GltfMaterialName>,
                Changed<Mesh3d>,
                Changed<SkinnedMesh>,
                Changed<ProjectedModelDrawShadowPolicy>,
                Changed<ModelMaterialInputOwner<ModelExpanded>>,
            )>,
        ),
    >,
    owners: Query<
        'w,
        's,
        Entity,
        Or<(
            Changed<PrefabMaterialOverrides>,
            Changed<PrefabClothingTextures>,
        )>,
    >,
}

pub(super) fn project_authored_model_material_passes(
    mut commands: Commands,
    invalidation: AuthoredPassInvalidation,
    mut primitives: Query<
        (
            Entity,
            Ref<GltfMaterialName>,
            &Mesh3d,
            Option<Ref<ModelMaterialInputOwner<ModelExpanded>>>,
            Option<&RenderLayers>,
            Has<NotShadowCaster>,
            Has<NotShadowReceiver>,
            Option<&SkinnedMesh>,
            Option<&MeshMorphWeights>,
            Option<&ProjectedModelDrawShadowPolicy>,
            Option<&MeshMaterial3d<StandardMaterial>>,
            Option<&AuthoredModelStandardMaterial>,
            Option<&mut PrefabPrimitiveMaterial>,
            Has<AuthoredModelEffectMaterial>,
            Option<&mut PendingAuthoredModelMaterialPasses>,
        ),
        Without<AdditionalAuthoredModelMaterialPass>,
    >,
    roots: Query<(
        Ref<PrefabMaterialOverrides>,
        Option<Ref<PrefabClothingTextures>>,
    )>,
    children: Query<&Children>,
    parents: Query<&ChildOf>,
    additional_passes: Query<&AdditionalAuthoredModelMaterialPass>,
    mut material_events: MessageReader<AssetEvent<MaterialAsset>>,
    mut pass_events: MessageReader<AssetEvent<EffectPassMaterial>>,
    mut removed_clothing: RemovedComponents<PrefabClothingTextures>,
    mut removed_overrides: RemovedComponents<PrefabMaterialOverrides>,
    mut removed_owners: RemovedComponents<ModelMaterialInputOwner<ModelExpanded>>,
    material_assets: Res<Assets<MaterialAsset>>,
    graphics_settings: Res<GraphicsSettings>,
    draw_materials: Res<Assets<EffectPassMaterial>>,
    (mut visibility, mut effect_draws, mut orderings): (
        Query<&mut Visibility, With<GltfMaterialName>>,
        ParamSet<(
            Query<&'static AuthoredModelEffectMaterial, AssetChanged<AuthoredModelEffectMaterial>>,
            Query<&'static mut AuthoredModelEffectMaterial>,
        )>,
        Query<&mut AuthoredEffectTechniquePassSubmissionOrder>,
    ),
) {
    let changed_materials = material_events
        .read()
        .filter_map(|event| match *event {
            AssetEvent::Added { id }
            | AssetEvent::Modified { id }
            | AssetEvent::Removed { id }
            | AssetEvent::LoadedWithDependencies { id } => Some(id),
            AssetEvent::Unused { .. } => None,
        })
        .collect::<HashSet<_>>();
    let removed_sources = removed_clothing
        .read()
        .chain(removed_overrides.read())
        .chain(removed_owners.read())
        .collect::<bevy::ecs::entity::EntityHashSet>();
    let changed_passes = pass_events
        .read()
        .filter_map(|event| match *event {
            AssetEvent::Added { id } | AssetEvent::Modified { id } | AssetEvent::Removed { id } => {
                Some(id)
            }
            _ => None,
        })
        .collect::<HashSet<_>>();

    let mut affected = invalidation
        .changed
        .iter()
        .collect::<bevy::ecs::entity::EntityHashSet>();
    affected.extend(effect_draws.p0().iter().map(|draw| draw.geometry));
    for owner in invalidation.owners.iter().chain(removed_sources) {
        affected.extend(std::iter::once(owner).chain(children.iter_descendants::<Children>(owner)));
    }
    if !changed_materials.is_empty() || !changed_passes.is_empty() {
        affected.extend(primitives.iter().filter_map(
            |(entity, _, _, _, _, _, _, _, _, _, _, _, material, _, waiting)| {
                let source_changed =
                    material.is_some_and(|material| changed_materials.contains(&material.0.id()));
                let pass_ready = waiting
                    .is_some_and(|waiting| waiting.0.iter().any(|id| changed_passes.contains(id)));
                (source_changed || pass_ready).then_some(entity)
            },
        ));
    }
    if graphics_settings.is_changed() {
        affected.extend(primitives.iter().map(|(entity, ..)| entity));
    }
    let mut primitives = primitives.iter_many_mut(affected).matched();
    let mut authored_effects = effect_draws.p1();
    while let Some((
        entity,
        name,
        mesh,
        owner,
        layers,
        no_shadow_cast,
        no_shadow_receive,
        skin,
        morph,
        shadow_projection,
        standard,
        authored_standard,
        previous_material,
        has_effect_binding,
        waiting,
    )) = primitives.fetch_next()
    {
        let root = owner.as_ref().and_then(|owner| roots.get(owner.0).ok());
        let override_material = root.as_ref().and_then(|(overrides, _)| {
            overrides.0.iter().find(|material_override| {
                name.0.eq_ignore_ascii_case(
                    &material_override.material.to_lowercase_hexadecimal_string(),
                )
            })
        });
        let Some(override_material) = override_material else {
            if previous_material.is_some() {
                despawn_authored_material_pass_children(
                    &mut commands,
                    entity,
                    &children,
                    &additional_passes,
                );
                let mut target = commands.entity(entity);
                target.remove::<(
                    AuthoredModelEffectMaterial,
                    MeshMaterial3d<EffectPassMaterial>,
                    PrefabPrimitiveMaterial,
                    AuthoredEffectTechniquePassSubmissionOrder,
                    AuthoredModelStandardMaterial,
                    PendingAuthoredModelMaterialPasses,
                )>();
                if let Some(standard) = authored_standard {
                    target.insert(MeshMaterial3d(standard.source.clone()));
                }
                if let Ok(mut visibility) = visibility.get_mut(entity) {
                    visibility.set_if_neq(Visibility::Inherited);
                }
            }
            continue;
        };
        let Some(handle) = override_material.handle.as_ref() else {
            continue;
        };
        let source_exists = previous_material.is_some();
        if let Some(current) = previous_material {
            current
                .map_unchanged(|material| &mut material.0)
                .set_if_neq(handle.clone());
        } else {
            commands
                .entity(entity)
                .try_insert(PrefabPrimitiveMaterial(handle.clone()));
        }
        let Some(material) = material_assets.get(handle) else {
            if source_exists && has_effect_binding {
                despawn_authored_material_pass_children(
                    &mut commands,
                    entity,
                    &children,
                    &additional_passes,
                );
                if let Ok(mut visibility) = visibility.get_mut(entity) {
                    visibility.set_if_neq(Visibility::Hidden);
                }
            }
            continue;
        };
        let Some(owner) = owner else { continue };
        let clothing = root.as_ref().and_then(|(_, clothing)| clothing.as_ref());
        let passes =
            material.evaluated_pass_material_assets_for_effect_quality(graphics_settings.effects);
        if passes
            .iter()
            .any(|pass| !draw_materials.contains(pass.id()))
        {
            let pending = passes.iter().map(Handle::id).collect::<Box<[_]>>();
            if let Some(waiting) = waiting {
                waiting
                    .map_unchanged(|waiting| &mut waiting.0)
                    .set_if_neq(pending);
            } else {
                commands
                    .entity(entity)
                    .try_insert(PendingAuthoredModelMaterialPasses(pending));
            }
            continue;
        }
        if waiting.is_some() {
            commands
                .entity(entity)
                .try_remove::<PendingAuthoredModelMaterialPasses>();
        }

        let mut retained_passes = children
            .get(entity)
            .into_iter()
            .flat_map(|children| children.iter())
            .filter_map(|child| {
                additional_passes
                    .get(child)
                    .ok()
                    .map(|pass| (pass.0, child))
            })
            .collect::<HashMap<_, _>>();
        if passes.is_empty() {
            for child in retained_passes.into_values() {
                commands.entity(child).try_despawn();
            }
            if let Ok(mut visibility) = visibility.get_mut(entity) {
                visibility.set_if_neq(Visibility::Hidden);
            }
            continue;
        }
        if let Ok(mut visibility) = visibility.get_mut(entity) {
            visibility.set_if_neq(Visibility::Inherited);
        }

        let owner_parent = parents
            .get(owner.0)
            .ok()
            .map(ChildOf::parent)
            .unwrap_or(owner.0);
        let owner_index = children
            .get(owner_parent)
            .ok()
            .and_then(|children| children.iter().position(|child| child == owner.0))
            .unwrap_or(0);
        let decal = material
            .netimmerse_texture_binding
            .as_ref()
            .and_then(|binding| {
                let texture = clothing.as_ref()?.0.iter().find(|(id, _)| {
                    name.0
                        .eq_ignore_ascii_case(&id.to_lowercase_hexadecimal_string())
                })?;
                Some((binding.decal_stage, binding.decal_uv_set, texture.1.clone()))
            });
        for (pass_index, pass) in passes.iter().enumerate() {
            let authored = AuthoredModelEffectMaterial {
                template: pass.clone(),
                clothing_decal: decal.clone(),
                geometry: entity,
            };
            let ordering = AuthoredEffectTechniquePassSubmissionOrder::new(entity, pass_index)
                .expect("the authored effect pass index fits u16")
                .with_source_hierarchy_order(owner_parent, owner_index);
            if pass_index == 0 {
                let mut target = commands.entity(entity);
                if !has_effect_binding {
                    if let Some(standard) = standard.filter(|_| authored_standard.is_none()) {
                        target.insert(AuthoredModelStandardMaterial {
                            source: standard.0.clone(),
                            rendered_material: standard.id(),
                            fading: false,
                        });
                    }
                    target.remove::<MeshMaterial3d<StandardMaterial>>();
                    target.insert(MeshMaterial3d(pass.clone()));
                }
                if let Ok(mut current) = authored_effects.get_mut(entity) {
                    current.set_if_neq(authored);
                } else {
                    target.insert(authored);
                }
                if let Ok(mut current) = orderings.get_mut(entity) {
                    if current.hierarchy_order() != ordering.hierarchy_order() {
                        *current = current.with_source_hierarchy_order(owner_parent, owner_index);
                    }
                } else {
                    target.insert(ordering);
                }
                continue;
            }
            if let Some(child) = retained_passes.remove(&pass_index) {
                if let Ok(mut current) = authored_effects.get_mut(child) {
                    current.set_if_neq(authored);
                }
                if let Ok(mut current) = orderings.get_mut(child) {
                    if current.hierarchy_order() != ordering.hierarchy_order() {
                        *current = current.with_source_hierarchy_order(owner_parent, owner_index);
                    }
                }
                continue;
            }
            let mut additional_pass = commands.spawn((
                mesh.clone(),
                authored,
                MeshMaterial3d(pass.clone()),
                Transform::IDENTITY,
                Visibility::Inherited,
                ChildOf(entity),
                AdditionalAuthoredModelMaterialPass(pass_index),
                ordering,
            ));
            if let Some(layers) = layers {
                additional_pass.insert(layers.clone());
            }
            if no_shadow_cast {
                additional_pass.insert(NotShadowCaster);
            }
            if no_shadow_receive {
                additional_pass.insert(NotShadowReceiver);
            }
            if let Some(skin) = skin {
                additional_pass.insert(skin.clone());
            }
            if let Some(morph) = morph {
                additional_pass.insert(morph.clone());
            }
            if let Some(shadow_projection) = shadow_projection {
                additional_pass.insert(*shadow_projection);
            }
        }
        for child in retained_passes.into_values() {
            commands.entity(child).try_despawn();
        }
    }
}

fn despawn_authored_material_pass_children(
    commands: &mut Commands,
    primitive: Entity,
    children: &Query<&Children>,
    additional_passes: &Query<&AdditionalAuthoredModelMaterialPass>,
) {
    for child in children
        .get(primitive)
        .into_iter()
        .flat_map(|children| children.iter())
    {
        if additional_passes.contains(child) {
            commands.entity(child).try_despawn();
        }
    }
}

#[derive(bevy::ecs::system::SystemParam)]
pub(super) struct AdditionalPassGeometryChanges<'w, 's> {
    changed: Query<
        'w,
        's,
        Entity,
        (
            With<GltfMaterialName>,
            Without<AdditionalAuthoredModelMaterialPass>,
            Or<(
                Changed<Mesh3d>,
                Changed<SkinnedMesh>,
                Changed<MeshMorphWeights>,
                Changed<RenderLayers>,
                Changed<NotShadowCaster>,
                Changed<NotShadowReceiver>,
                Changed<ProjectedModelDrawShadowPolicy>,
            )>,
        ),
    >,
    skins: RemovedComponents<'w, 's, SkinnedMesh>,
    morphs: RemovedComponents<'w, 's, MeshMorphWeights>,
    layers: RemovedComponents<'w, 's, RenderLayers>,
    shadow_casters: RemovedComponents<'w, 's, NotShadowCaster>,
    shadow_receivers: RemovedComponents<'w, 's, NotShadowReceiver>,
    lighting: RemovedComponents<'w, 's, ProjectedModelDrawShadowPolicy>,
}

#[allow(clippy::type_complexity)]
pub(super) fn synchronize_additional_pass_geometry(
    mut commands: Commands,
    mut changes: AdditionalPassGeometryChanges,
    primitives: Query<
        (
            &Children,
            &Mesh3d,
            Option<&SkinnedMesh>,
            Option<&MeshMorphWeights>,
            Option<&RenderLayers>,
            Has<NotShadowCaster>,
            Has<NotShadowReceiver>,
            Option<&ProjectedModelDrawShadowPolicy>,
        ),
        Without<AdditionalAuthoredModelMaterialPass>,
    >,
    mut passes: Query<
        (
            Entity,
            &mut Mesh3d,
            Option<&mut SkinnedMesh>,
            Option<&mut MeshMorphWeights>,
            Option<&mut RenderLayers>,
            Has<NotShadowCaster>,
            Has<NotShadowReceiver>,
            Option<&mut ProjectedModelDrawShadowPolicy>,
        ),
        With<AdditionalAuthoredModelMaterialPass>,
    >,
) {
    let affected = changes
        .changed
        .iter()
        .chain(changes.skins.read())
        .chain(changes.morphs.read())
        .chain(changes.layers.read())
        .chain(changes.shadow_casters.read())
        .chain(changes.shadow_receivers.read())
        .chain(changes.lighting.read())
        .collect::<bevy::ecs::entity::EntityHashSet>();
    for (children, mesh, skin, morph, layers, no_cast, no_receive, shadow) in
        primitives.iter_many(affected).matched()
    {
        let mut passes = passes.iter_many_mut(children.iter()).matched();
        while let Some((
            pass,
            mut current_mesh,
            current_skin,
            current_morph,
            current_layers,
            had_no_cast,
            had_no_receive,
            current_shadow,
        )) = passes.fetch_next()
        {
            current_mesh.set_if_neq(mesh.clone());
            let mut pass = commands.entity(pass);
            match (current_skin, skin) {
                (Some(mut current), Some(skin)) => {
                    if current.inverse_bindposes != skin.inverse_bindposes
                        || current.joints != skin.joints
                    {
                        *current = skin.clone();
                    }
                }
                (None, Some(skin)) => {
                    pass.try_insert(skin.clone());
                }
                (Some(_), None) => {
                    pass.try_remove::<SkinnedMesh>();
                }
                (None, None) => {}
            }
            match (current_morph, morph) {
                (Some(mut current), Some(morph)) => {
                    let equal = match (&*current, morph) {
                        (
                            MeshMorphWeights::Value { weights: old },
                            MeshMorphWeights::Value { weights: new },
                        ) => old == new,
                        (MeshMorphWeights::Reference(old), MeshMorphWeights::Reference(new)) => {
                            old == new
                        }
                        _ => false,
                    };
                    if !equal {
                        *current = morph.clone();
                    }
                }
                (None, Some(morph)) => {
                    pass.try_insert(morph.clone());
                }
                (Some(_), None) => {
                    pass.try_remove::<MeshMorphWeights>();
                }
                (None, None) => {}
            }
            match (current_layers, layers) {
                (Some(mut current), Some(layers)) => {
                    current.set_if_neq(layers.clone());
                }
                (None, Some(layers)) => {
                    pass.try_insert(layers.clone());
                }
                (Some(_), None) => {
                    pass.try_remove::<RenderLayers>();
                }
                (None, None) => {}
            }
            match (current_shadow, shadow) {
                (Some(mut current), Some(shadow)) => {
                    current.set_if_neq(*shadow);
                }
                (None, Some(shadow)) => {
                    pass.try_insert(*shadow);
                }
                (Some(_), None) => {
                    pass.try_remove::<ProjectedModelDrawShadowPolicy>();
                }
                (None, None) => {}
            }
            match (no_cast, had_no_cast) {
                (true, false) => {
                    pass.try_insert(NotShadowCaster);
                }
                (false, true) => {
                    pass.try_remove::<NotShadowCaster>();
                }
                _ => {}
            }
            match (no_receive, had_no_receive) {
                (true, false) => {
                    pass.try_insert(NotShadowReceiver);
                }
                (false, true) => {
                    pass.try_remove::<NotShadowReceiver>();
                }
                _ => {}
            }
        }
    }
}
