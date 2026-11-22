use super::prefab_instance_transform;
use bevy::{ecs::relationship::RelationshipTarget, prelude::*};
use openzt2_game_assets::scene_prefab::ScenePrefabAsset;
use openzt2_world_state::{
    animation_playback::model_joint_attachment_binding::CarriedPrefabPresentation,
    application_schedule::GameSet,
    world_spawn::{
        prefab_presentation_types::PrefabPresentation,
        prefab_source_asset_handle::PrefabSourceAssetHandle,
    },
};

#[derive(Component)]
#[relationship(relationship_target = CarriedPrefabVisual)]
struct CarriedPrefabVisualOf(Entity);

#[derive(Component)]
#[relationship_target(relationship = CarriedPrefabVisualOf, linked_spawn)]
struct CarriedPrefabVisual(Entity);

#[derive(Component)]
struct UncarriedPrefabVisibility(Visibility);

pub(super) fn register_carried_prefab_presentation(app: &mut App) {
    app.add_systems(
        Update,
        update_carried_prefab_presentation
            .before(super::super::prefab_presentation_render_tree::hydrate_prefab_presentations)
            .in_set(GameSet::Intent),
    );
}

fn update_carried_prefab_presentation(
    mut commands: Commands,
    mut removed: RemovedComponents<CarriedPrefabPresentation>,
    mut items: Query<
        (
            &mut Visibility,
            Option<&UncarriedPrefabVisibility>,
            Option<&CarriedPrefabVisual>,
            Option<Ref<CarriedPrefabPresentation>>,
            Option<Ref<PrefabSourceAssetHandle>>,
        ),
        Without<CarriedPrefabVisualOf>,
    >,
    changed: Query<
        Entity,
        Or<(
            Changed<CarriedPrefabPresentation>,
            Changed<PrefabSourceAssetHandle>,
        )>,
    >,
    mut visuals: Query<
        (
            &mut Transform,
            &mut Visibility,
            &ChildOf,
            &PrefabPresentation,
        ),
        With<CarriedPrefabVisualOf>,
    >,
    anchors: Query<()>,
    prefabs: Res<Assets<ScenePrefabAsset>>,
) {
    let affected = changed
        .iter()
        .chain(removed.read())
        .collect::<bevy::ecs::entity::EntityHashSet>();
    for item in affected {
        let Ok((mut visibility, previous, visual, pose, source)) = items.get_mut(item) else {
            continue;
        };
        let existing = visual.and_then(|visual| visual.iter().next());
        let Some((pose, source)) = pose.zip(source) else {
            if let Some(previous) = previous {
                if *visibility == Visibility::Hidden {
                    visibility.set_if_neq(previous.0);
                }
                commands
                    .entity(item)
                    .try_remove::<UncarriedPrefabVisibility>();
            }
            if let Some(visual) = existing {
                commands.entity(visual).try_despawn();
            }
            continue;
        };
        if pose.anchor == item || !anchors.contains(pose.anchor) {
            continue;
        }
        let Some(authored_root) = prefabs
            .get(&source.0)
            .and_then(|prefab| prefab.canonical_scene_prefab_document().entities.first())
        else {
            continue;
        };
        let local_transform = prefab_instance_transform(pose.local_transform, authored_root);
        let carried_visibility = previous.map_or(*visibility, |previous| previous.0);
        if previous.is_none() {
            commands
                .entity(item)
                .try_insert(UncarriedPrefabVisibility(*visibility));
        }
        visibility.set_if_neq(Visibility::Hidden);
        if let Some(visual) = existing {
            if let Ok((mut transform, mut visible, parent, prefab)) = visuals.get_mut(visual) {
                if prefab.0 == source.0 {
                    transform.set_if_neq(local_transform);
                    visible.set_if_neq(carried_visibility);
                    if parent.parent() != pose.anchor {
                        commands.entity(visual).insert(ChildOf(pose.anchor));
                    }
                    continue;
                }
            }
            commands.entity(visual).try_despawn();
        }
        commands.spawn((
            CarriedPrefabVisualOf(item),
            ChildOf(pose.anchor),
            local_transform,
            carried_visibility,
            PrefabPresentation::new(source.0.clone()),
        ));
    }
}
