use bevy::{
    camera::{visibility::RenderLayers, ClearColorConfig, RenderTarget, ScalingMode},
    prelude::*,
    render::render_resource::TextureFormat,
};
use openzt2_game_assets::{
    effect::{
        effect_instance_spawning_and_preparation::spawn_complete_particle_effect_document,
        LiveParticleEffectInstance, ParticleEffectDocumentAsset,
        SpawnCompleteParticleEffectDocument,
    },
    scene_prefab::ScenePrefabAsset,
    ui_document::ui_document_asset_types_and_borrowing_queries::{FameStarModel, UiDocumentAsset},
};
use openzt2_interface_state::ui::authored_ui_node_projection_components::{
    UiDocumentOwner, UiDocumentRoot, UiNodeId,
};
use openzt2_world::world_spawn::prefab_presentation_render_tree::spawn_prefab_render_tree;
use openzt2_world_state::world_spawn::{
    prefab_fixed_function_world_lighting_policy::PrefabFixedFunctionWorldLightingPolicy,
    world_load_completion_marker::WorldLoadCompleted, world_membership_types::WorldRoot,
};
use openzt2_zoo_state::progression::fame_types::Fame;

const STAR_RENDER_LAYER: usize = 28;

#[derive(Component)]
#[relationship(relationship_target = FameStarTarget)]
pub(super) struct FameStarSceneFor(Entity);

#[derive(Component)]
#[relationship_target(relationship = FameStarSceneFor, linked_spawn)]
pub(super) struct FameStarTarget(Vec<Entity>);

#[derive(Component)]
pub(super) struct FameStarScene(Handle<UiDocumentAsset>);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
enum Filling {
    #[default]
    Empty,
    Half,
    Full,
}

#[derive(Component)]
pub(super) struct FameStarWell {
    index: usize,
    filling: Filling,
    stable: Option<Entity>,
    reveal: Timer,
}

#[derive(Component)]
pub(super) struct FameStarTransient(Timer);

pub(super) fn create_fame_star_scenes(
    mut commands: Commands,
    documents: Res<Assets<UiDocumentAsset>>,
    mut images: ResMut<Assets<Image>>,
    nodes: Query<(Entity, Ref<UiNodeId>, &UiDocumentOwner), Without<FameStarTarget>>,
    roots: Query<&UiDocumentRoot>,
) {
    for (owner, node, document_owner) in &nodes {
        if !node.is_added() && !documents.is_changed() {
            continue;
        }
        let Ok(binding) = roots.get(document_owner.0) else {
            continue;
        };
        let Some(display) = documents
            .get(&binding.document)
            .and_then(UiDocumentAsset::fame_star_display)
        else {
            continue;
        };
        if node.index != display.wells_node_index {
            continue;
        }

        let target = images.add(Image::new_target_texture(
            320,
            64,
            TextureFormat::Rgba8UnormSrgb,
            None,
        ));
        commands.spawn((
            ImageNode::new(target.clone()),
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                ..default()
            },
            Pickable::IGNORE,
            ZIndex(1),
            ChildOf(owner),
        ));
        let scene = commands
            .spawn((
                Transform::IDENTITY,
                Visibility::Inherited,
                RenderLayers::layer(STAR_RENDER_LAYER),
                PrefabFixedFunctionWorldLightingPolicy {
                    enabled: false,
                    preserve_authored_material_lighting: true,
                },
                FameStarSceneFor(owner),
                FameStarScene(binding.document.clone()),
            ))
            .id();
        commands.spawn((
            Camera3d::default(),
            Camera {
                order: -20,
                clear_color: ClearColorConfig::Custom(Color::NONE),
                ..default()
            },
            Projection::Orthographic(OrthographicProjection {
                scaling_mode: ScalingMode::Fixed {
                    width: display.viewport_size_m.x,
                    height: display.viewport_size_m.y,
                },
                ..OrthographicProjection::default_3d()
            }),
            display.viewport_transform,
            RenderTarget::Image(target.into()),
            RenderLayers::layer(STAR_RENDER_LAYER),
            ChildOf(scene),
        ));
        for (index, &transform) in display.well_transforms.iter().enumerate() {
            commands.spawn((
                FameStarWell {
                    index,
                    filling: Filling::Empty,
                    stable: None,
                    reveal: Timer::from_seconds(0.0, TimerMode::Once),
                },
                ChildOf(scene),
                transform,
                Visibility::Inherited,
            ));
        }
    }
}

pub(super) fn present_fame_stars(
    mut commands: Commands,
    graphics: Res<openzt2_game_assets::graphics_settings_types::GraphicsSettings>,
    documents: Res<Assets<UiDocumentAsset>>,
    prefabs: Res<Assets<ScenePrefabAsset>>,
    effects: Res<Assets<ParticleEffectDocumentAsset>>,
    fame: Single<Ref<Fame>, (With<WorldRoot>, With<WorldLoadCompleted>)>,
    scenes: Query<&FameStarScene>,
    mut wells: Query<(Entity, &ChildOf, &mut FameStarWell)>,
) {
    let animate = graphics.object_detail
        != openzt2_game_assets::graphics_settings_types::ThreeLevelGraphicsDetail::Low;
    for (entity, parent, mut well) in &mut wells {
        let Some(display) = scenes
            .get(parent.parent())
            .ok()
            .and_then(|scene| documents.get(&scene.0))
            .and_then(UiDocumentAsset::fame_star_display)
        else {
            continue;
        };
        let full = usize::from(fame.half_stars / 2);
        let filling = if well.index < full {
            Filling::Full
        } else if well.index == full && fame.half_stars % 2 != 0 {
            Filling::Half
        } else {
            Filling::Empty
        };
        let steady_model = match filling {
            Filling::Empty => None,
            Filling::Half => Some(&display.half),
            Filling::Full => Some(&display.full),
        };
        if fame.is_changed()
            || well.is_added()
            || prefabs.is_changed()
            || effects.is_changed()
            || documents.is_changed()
        {
            if filling != well.filling {
                let steady = steady_model.and_then(|model| {
                    let prefab = prefabs.get(&model.model)?;
                    Some((model, prefab, resolve_fame_star_effects(model, &effects)?))
                });
                if steady_model.is_some() && steady.is_none() {
                    continue;
                }
                let transition = if filling > well.filling {
                    &display.appear
                } else {
                    &display.disappear
                };
                let transition_model = if animate {
                    let Some(model) = prefabs.get(&transition.model) else {
                        continue;
                    };
                    let Some(cues) = resolve_fame_star_effects(transition, &effects) else {
                        continue;
                    };
                    Some((model, cues))
                } else {
                    None
                };
                if let Some(previous) = well.stable.take() {
                    commands.entity(previous).try_despawn();
                }
                let has_transition = transition_model.is_some();
                if let Some((prefab, cues)) = transition_model {
                    let model = spawn_fame_star_model(
                        &mut commands,
                        entity,
                        prefab,
                        transition,
                        Visibility::Inherited,
                    );
                    commands
                        .entity(model)
                        .insert(FameStarTransient(Timer::from_seconds(
                            transition.duration_seconds.max(0.0),
                            TimerMode::Once,
                        )));
                    spawn_fame_star_effects(&mut commands, entity, cues);
                }
                well.filling = filling;
                let delay = if has_transition {
                    display.animation_delay_seconds.max(0.0)
                } else {
                    0.0
                };
                well.reveal = Timer::from_seconds(delay, TimerMode::Once);
                well.stable = steady.map(|(model, prefab, cues)| {
                    let stable = spawn_fame_star_model(
                        &mut commands,
                        entity,
                        prefab,
                        model,
                        if delay > 0.0 {
                            Visibility::Hidden
                        } else {
                            Visibility::Inherited
                        },
                    );
                    if animate {
                        spawn_fame_star_effects(&mut commands, stable, cues);
                    }
                    stable
                });
            }
        }
    }
}

pub(super) fn reveal_delayed_fame_stars(
    time: Res<Time<Real>>,
    mut wells: Query<&mut FameStarWell>,
    mut visibility: Query<&mut Visibility>,
) {
    for mut well in &mut wells {
        if well.reveal.duration().is_zero() || well.reveal.is_finished() {
            continue;
        }
        if well.reveal.tick(time.delta()).just_finished() {
            if let Some(mut visible) = well.stable.and_then(|model| visibility.get_mut(model).ok())
            {
                visible.set_if_neq(Visibility::Inherited);
            }
        }
    }
}

fn spawn_fame_star_model(
    commands: &mut Commands,
    well: Entity,
    prefab: &ScenePrefabAsset,
    model: &FameStarModel,
    visibility: Visibility,
) -> Entity {
    let holder = commands
        .spawn((
            ChildOf(well),
            Transform::from_scale(Vec3::splat(model.scale)),
            visibility,
        ))
        .id();
    let (_, renderables, _) = spawn_prefab_render_tree(commands, prefab, holder, false);
    for entity in renderables {
        commands
            .entity(entity)
            .insert(RenderLayers::layer(STAR_RENDER_LAYER));
    }
    holder
}

fn resolve_fame_star_effects(
    model: &FameStarModel,
    effects: &Assets<ParticleEffectDocumentAsset>,
) -> Option<Vec<(Handle<ParticleEffectDocumentAsset>, Option<f32>)>> {
    model
        .effects
        .iter()
        .map(|cue| {
            effects.get(&cue.effect)?;
            Some((
                cue.effect.clone(),
                cue.duration_seconds
                    .is_finite()
                    .then_some(cue.duration_seconds),
            ))
        })
        .collect()
}

fn spawn_fame_star_effects(
    commands: &mut Commands,
    owner: Entity,
    cues: Vec<(Handle<ParticleEffectDocumentAsset>, Option<f32>)>,
) {
    for (effect, duration) in cues {
        let instance = spawn_complete_particle_effect_document(
            commands,
            &SpawnCompleteParticleEffectDocument {
                particle_effect_asset: effect,
                parent_entity: Some(owner),
                inherit_parent_rotation: true,
                effect_transform: Transform::IDENTITY,
                manual_particle_count: 0,
            },
        );
        let mut instance = commands.entity(instance);
        instance.insert(RenderLayers::layer(STAR_RENDER_LAYER));
        if let Some(duration) = duration {
            instance.insert(FameStarTransient(Timer::from_seconds(
                duration.max(0.0),
                TimerMode::Once,
            )));
        }
    }
}

pub(super) fn expire_fame_star_transients(
    mut commands: Commands,
    time: Res<Time<Real>>,
    mut transients: Query<(Entity, &mut FameStarTransient)>,
) {
    for (entity, mut transient) in &mut transients {
        if transient.0.tick(time.delta()).is_finished() {
            commands.entity(entity).try_despawn();
        }
    }
}

pub(super) fn isolate_fame_star_particles(
    mut commands: Commands,
    emitters: Query<Entity, (Added<LiveParticleEffectInstance>, Without<RenderLayers>)>,
    scenes: Query<(), With<FameStarScene>>,
    parents: Query<&ChildOf>,
) {
    for entity in &emitters {
        if parents
            .iter_ancestors::<ChildOf>(entity)
            .any(|ancestor| scenes.contains(ancestor))
        {
            commands
                .entity(entity)
                .insert(RenderLayers::layer(STAR_RENDER_LAYER));
        }
    }
}

pub(super) fn set_fame_star_camera_visibility(
    mut cameras: Query<(&ChildOf, &mut Camera)>,
    scenes: Query<&FameStarSceneFor>,
    owners: Query<&InheritedVisibility>,
) {
    for (parent, camera) in &mut cameras {
        let Ok(owner) = scenes.get(parent.parent()) else {
            continue;
        };
        let visible = owners.get(owner.0).is_ok_and(|visibility| visibility.get());
        camera
            .map_unchanged(|camera| &mut camera.is_active)
            .set_if_neq(visible);
    }
}
