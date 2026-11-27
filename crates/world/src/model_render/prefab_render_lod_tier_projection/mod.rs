use super::{
    active_model_view_cameras::ActiveModelViewCameras,
    prefab_render_layer_camera_matching::PrefabCameraRenderLayers,
};
use bevy::prelude::*;
use openzt2_game_assets::{
    graphics_settings_types::GraphicsSettings,
    scene_prefab::select_prefab_render_lod_tier,
    world_definitions::{
        world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
        world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
    },
};
use openzt2_game_data::scene_prefab::{PrefabLod, PrefabRenderLodPolicy};
use openzt2_game_data::AssetId;
use openzt2_world_state::model_render::prefab_render_lod_tier::{
    PrefabRenderLodDistance, PrefabRenderLodDistanceOverrides,
};
use openzt2_world_state::{
    animation_graph::animation_presentation_relationship_types::AnimationPresentationBinder,
    world_spawn::world_membership_types::DefinitionId,
};
use openzt2_zoo_state::animal_lifecycle::types::AnimalVariant;

use openzt2_world_state::{
    model_render::prefab_render_lod_tier::{
        PrefabRenderLodSelection, PrefabRenderLodTier, PrefabRenderLodVariant,
        PrefabRenderLodVariants,
    },
    world_spawn::prefab_model_level_of_detail_visibility_range::PrefabModelLevelOfDetailVisibilityRange,
};

pub(crate) fn bind_prefab_lod(
    commands: &mut Commands,
    entities: &[Entity],
    entity: Entity,
    lod: &PrefabLod,
) {
    let group = entities[lod.group_entity as usize];
    if let Some(initial) = lod.initial_render_tier {
        commands
            .entity(group)
            .insert_if_new(PrefabRenderLodTier(initial));
        commands.entity(entity).insert((
            PrefabRenderLodVariant {
                group,
                ordinal: lod.ordinal,
            },
            PrefabRenderLodSelection(initial == lod.ordinal),
        ));
    } else {
        commands
            .entity(entity)
            .insert(PrefabModelLevelOfDetailVisibilityRange {
                group,
                ordinal: lod.ordinal,
                center_m: Vec3::from_array(lod.center_m),
                near_m: lod.near_m,
                far_m: lod.far_m,
                active_without_range: lod.active_without_range,
            });
    }
}

#[derive(Component)]
struct PrefabLodDefinitionScope(Entity);

pub(crate) fn bind_prefab_lod_policy(
    commands: &mut Commands,
    entities: &[Entity],
    entity: Entity,
    scope: Entity,
    policy: &PrefabRenderLodPolicy,
) {
    let Some(&origin) = entities.get(policy.metric_entity as usize) else {
        return;
    };
    commands
        .entity(entity)
        .insert_if_new((
            PrefabRenderLodTier(policy.initial_tier),
            PrefabLodDefinitionScope(scope),
        ))
        .try_insert(PrefabRenderLodDistance {
            origin,
            policy: policy.clone(),
        });
}

pub(super) fn register_prefab_render_lod_distances(app: &mut App) {
    app.add_systems(
        PostUpdate,
        bind_authored_prefab_render_lod_distances.before(select_prefab_render_lod_from_camera),
    );
}

fn bind_authored_prefab_render_lod_distances(
    mut commands: Commands,
    catalogue: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    mut documents: MessageReader<AssetEvent<WorldDefinitionAsset>>,
    parents: Query<Ref<ChildOf>>,
    definitions: Query<(Option<Ref<AnimalVariant>>, Option<Ref<DefinitionId>>)>,
    binders: Query<Ref<AnimationPresentationBinder>>,
    mut groups: Query<(
        Entity,
        Ref<PrefabLodDefinitionScope>,
        Option<&mut PrefabRenderLodDistanceOverrides>,
    )>,
) {
    let documents_changed = documents.read().fold(false, |changed, event| {
        changed
            || matches!(
                event,
                AssetEvent::Added { .. } | AssetEvent::Modified { .. } | AssetEvent::Removed { .. }
            )
    });
    let Some(view) = catalogue.get(&assets) else {
        return;
    };
    let main = AssetId::from_key("mainobj");
    for (entity, scope, existing) in &mut groups {
        let mut dirty = catalogue.is_changed() || documents_changed || scope.is_changed();
        let mut binder = None;
        let mut current = Some(scope.0);
        while let Some(owner) = current {
            if binder.is_none() {
                if let Ok(name) = binders.get(owner) {
                    dirty |= name.is_changed();
                    binder = Some(name.0);
                }
            }
            if let Ok((variant, definition)) = definitions.get(owner) {
                dirty |= variant.as_ref().is_some_and(Ref::is_changed)
                    || definition.as_ref().is_some_and(Ref::is_changed);
                if let Some(definition) = variant
                    .map(|variant| variant.0)
                    .or_else(|| definition.map(|id| id.0))
                {
                    if !dirty {
                        break;
                    }
                    let name = binder.unwrap_or(main);
                    let distances = view.render_lod_distances(definition, name).or_else(|| {
                        (name == main)
                            .then(|| view.render_lod_distances(definition, AssetId::default()))
                            .flatten()
                    });
                    if let Some(distances) = distances {
                        let next = PrefabRenderLodDistanceOverrides {
                            switch_distance_squared: distances.switch_distance_squared.clone(),
                            minimum_switch_distance_squared: distances
                                .minimum_switch_distance_squared
                                .clone(),
                        };
                        if let Some(mut existing) = existing {
                            existing.set_if_neq(next);
                        } else {
                            commands.entity(entity).insert(next);
                        }
                    } else if existing.is_some() {
                        commands
                            .entity(entity)
                            .remove::<PrefabRenderLodDistanceOverrides>();
                    }
                    break;
                }
            }
            current = parents.get(owner).ok().map(|parent| {
                dirty |= parent.is_changed();
                parent.parent()
            });
            if current.is_none() && dirty && existing.is_some() {
                commands
                    .entity(entity)
                    .remove::<PrefabRenderLodDistanceOverrides>();
            }
        }
    }
}

pub(super) fn select_prefab_render_lod_from_camera(
    cameras: Res<ActiveModelViewCameras>,
    graphics: Option<Res<GraphicsSettings>>,
    origins: Query<Ref<GlobalTransform>>,
    overrides: Query<Ref<PrefabRenderLodDistanceOverrides>>,
    parents: Query<&ChildOf>,
    mut groups: Query<(
        Entity,
        Ref<PrefabRenderLodDistance>,
        Option<Ref<PrefabCameraRenderLayers>>,
        &mut PrefabRenderLodTier,
    )>,
    mut removed_overrides: RemovedComponents<PrefabRenderLodDistanceOverrides>,
) {
    let refresh = cameras.is_changed()
        | graphics
            .as_ref()
            .is_some_and(|graphics| graphics.is_changed())
        | (removed_overrides.read().count() != 0);
    let detail_scale = graphics
        .as_ref()
        .map_or(1.0, |settings| settings.object_lod_distance_scale());
    for (entity, distance, layers, mut tier) in &mut groups {
        let Ok(origin) = origins.get(distance.origin) else {
            continue;
        };
        let tuning = std::iter::once(entity)
            .chain(parents.iter_ancestors::<ChildOf>(entity))
            .find_map(|entity| overrides.get(entity).ok());
        if !refresh
            && !distance.is_changed()
            && !origin.is_changed()
            && !layers.as_ref().is_some_and(Ref::is_changed)
            && !tuning.as_ref().is_some_and(Ref::is_changed)
        {
            continue;
        }
        let layers = layers
            .as_deref()
            .map(|layers| &layers.0)
            .unwrap_or_default();
        let next = cameras
            .for_billboard(layers)
            .map_or(distance.policy.initial_tier, |view| {
                let squared = view
                    .pose
                    .translation()
                    .distance_squared(origin.translation());
                select_prefab_render_lod_tier(
                    &distance.policy,
                    squared,
                    (detail_scale / view.lod_view_factor).powi(2),
                    tuning
                        .as_ref()
                        .map_or(&[], |tuning| tuning.switch_distance_squared.as_slice()),
                    tuning.as_ref().map_or(&[], |tuning| {
                        tuning.minimum_switch_distance_squared.as_slice()
                    }),
                )
            });
        tier.set_if_neq(PrefabRenderLodTier(next));
    }
}

pub(super) fn project_prefab_render_lod_tier(
    groups: Query<
        (&PrefabRenderLodTier, &PrefabRenderLodVariants),
        Or<(
            Changed<PrefabRenderLodTier>,
            Changed<PrefabRenderLodVariants>,
        )>,
    >,
    mut variants: Query<(
        &PrefabRenderLodVariant,
        &mut PrefabRenderLodSelection,
        &mut Visibility,
        Has<PrefabModelLevelOfDetailVisibilityRange>,
    )>,
) {
    for (selected, members) in &groups {
        for entity in members.iter() {
            let Ok((variant, mut selection, mut visibility, has_distance_range)) =
                variants.get_mut(entity)
            else {
                continue;
            };
            let shown = selected.0 == variant.ordinal;
            selection.set_if_neq(PrefabRenderLodSelection(shown));
            if !has_distance_range {
                visibility.set_if_neq(if shown {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                });
            }
        }
    }
}
