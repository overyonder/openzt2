use bevy::picking::{events::PointerPress, pointer::PointerButton};
use bevy::{
    ecs::{lifecycle::Remove, relationship::RelationshipTarget},
    prelude::*,
};
use openzt2_game_data::ui_document::{
    action::information::InformationViewCategory, overview_map_presentation::UiOverviewLayer,
    widget::UiWidgetRecord,
};
use openzt2_game_data::AssetId;

use openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use openzt2_interface_state::input::input_types::ActionSource;
use openzt2_zoo_state::animal_lifecycle::types::Animal;
use openzt2_interface_state::camera::camera_runtime_state_types::OverheadRig;
use openzt2_interface_state::ui::authored_reusable_list_and_table_runtime_types::UiTablePolicy;
use openzt2_interface_state::ui::authored_ui_node_projection_components::{UiDocumentMembers, UiDocumentOwner};
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentRoot;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiNodeId;
use openzt2_world_state::world_spawn::world_terrain_hydration::WorldTerrainHorizontalBounds;

use openzt2_interface_state::information::entity_selection_types::Inspectable;
use openzt2_interface_state::information::entity_selection_types::SelectionRequest;

use super::super::information_view_types::InformationViewClass;
use openzt2_interface_state::information::overview::overview_types::OverviewCameraMarker;
use openzt2_interface_state::information::overview::overview_types::{
    OverviewMarker, OverviewSubjectMarkers,
};

#[derive(Component, PartialEq)]
pub(crate) struct OverviewSubjectLayer {
    category: InformationViewCategory,
    frame: Option<Handle<Image>>,
}

pub(crate) fn hydrate_authored_overview_marker_layers(
    surfaces: Query<(&UiNodeId, &UiDocumentOwner, &UiTablePolicy)>,
    roots: Query<(&UiDocumentRoot, &UiDocumentMembers)>,
    node_ids: Query<&UiNodeId>,
    documents: Res<Assets<UiDocumentAsset>>,
    children: Query<&Children>,
    cameras: Query<(), With<OverviewCameraMarker>>,
    mut layers: Query<&mut OverviewSubjectLayer>,
    mut commands: Commands,
) {
    for (node, owner, policy) in &surfaces {
        if *policy != UiTablePolicy::WorldMap {
            continue;
        }
        let Ok((root, members)) = roots.get(owner.0) else {
            continue;
        };
        let Some(document) = documents.get(&root.document) else {
            continue;
        };
        let Some(UiWidgetRecord::WorldMap {
            layers: records, ..
        }) = document
            .canonical_ui_document()
            .nodes
            .get(node.index as usize)
            .map(|record| &record.widget)
        else {
            continue;
        };
        for record in records {
            let id = AssetId(record.node.0);
            let Some(layer) = members
                .iter()
                .find(|&entity| node_ids.get(entity).is_ok_and(|node| node.id == id))
            else {
                continue;
            };
            let frame = document.cloned_texture_image_handle(AssetId(record.marker_icon.0));
            let category = match record.kind {
                UiOverviewLayer::Animals => InformationViewCategory::Animals,
                UiOverviewLayer::Buildings => InformationViewCategory::Buildings,
                UiOverviewLayer::CameraPosition => {
                    if let Some(frame) = frame {
                        if !children.get(layer).is_ok_and(|children| {
                            children.iter().any(|child| cameras.contains(child))
                        }) {
                            commands.spawn((
                                Name::new("overview camera position"),
                                OverviewCameraMarker,
                                Node {
                                    position_type: PositionType::Absolute,
                                    width: Val::Px(43.0),
                                    height: Val::Px(43.0),
                                    ..default()
                                },
                                UiTransform {
                                    translation: Val2::px(-21.5, -21.5),
                                    ..default()
                                },
                                ImageNode::new(frame),
                                Pickable::IGNORE,
                                ChildOf(layer),
                            ));
                        }
                    }
                    continue;
                }
                _ => continue,
            };
            let binding = OverviewSubjectLayer { category, frame };
            if let Ok(mut existing) = layers.get_mut(layer) {
                existing.set_if_neq(binding);
            } else {
                commands.entity(layer).insert(binding);
            }
        }
    }
}

pub(crate) fn reconcile_changed_overview_subject_markers(
    layers: Query<(Entity, Ref<OverviewSubjectLayer>)>,
    subjects: Query<(
        Entity,
        &Inspectable,
        Option<&InformationViewClass>,
        Has<Animal>,
        Option<&OverviewSubjectMarkers>,
    )>,
    changed: Query<
        Entity,
        Or<(
            Changed<Inspectable>,
            Changed<InformationViewClass>,
            Added<Animal>,
        )>,
    >,
    mut removed_class: RemovedComponents<InformationViewClass>,
    mut removed_animal: RemovedComponents<Animal>,
    parents: Query<&ChildOf, With<OverviewMarker>>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    definitions: Res<WorldDefinitions>,
    mut commands: Commands,
) {
    let changed_subjects = changed
        .iter()
        .chain(removed_class.read())
        .chain(removed_animal.read())
        .collect::<std::collections::BTreeSet<_>>();
    let Some(definitions_view) = definitions.get(&assets) else {
        return;
    };
    for (layer_entity, layer) in &layers {
        let reconcile_all = layer.is_changed() || definitions.is_changed() || assets.is_changed();
        let candidates = reconcile_all
            .then(|| subjects.iter())
            .into_iter()
            .flatten()
            .chain(
                (!reconcile_all)
                    .then(|| {
                        subjects
                            .iter_many(changed_subjects.iter().copied())
                            .filter_map(Result::ok)
                    })
                    .into_iter()
                    .flatten(),
            );
        for (subject, inspectable, class, animal, markers) in candidates {
            let category = if animal {
                InformationViewCategory::Animals
            } else {
                class.map_or(InformationViewCategory::Foliage, |class| class.0)
            };
            let accepted = category == layer.category
                || (layer.category == InformationViewCategory::Buildings
                    && category == InformationViewCategory::Entrances);
            let marker = markers.and_then(|markers| {
                markers.iter().find(|&entity| {
                    parents
                        .get(entity)
                        .is_ok_and(|parent| parent.parent() == layer_entity)
                })
            });
            match (marker, accepted) {
                (None, true) => spawn_world_subject_overview_map_marker(
                    &mut commands,
                    layer_entity,
                    subject,
                    inspectable,
                    layer.frame.clone(),
                    Some(definitions_view),
                ),
                (Some(entity), false) => {
                    commands.entity(entity).try_despawn();
                }
                _ => {}
            }
        }
    }
}

pub(crate) fn project_overhead_camera_position_to_overview_map_marker(
    world_bounds: Query<Ref<WorldTerrainHorizontalBounds>>,
    overhead_camera_rigs: Query<Ref<OverheadRig>>,
    mut camera_markers: Query<(&mut Node, &mut UiTransform, Ref<OverviewCameraMarker>)>,
) {
    let (Ok(world_bounds), Ok(overhead_camera_rig)) =
        (world_bounds.single(), overhead_camera_rigs.single())
    else {
        return;
    };
    let world_bounds_size = world_bounds.max - world_bounds.min;
    if !world_bounds_size.is_finite() || world_bounds_size.min_element() <= 0.0 {
        return;
    }
    let normalized_camera_position =
        (overhead_camera_rig.focus - world_bounds.min) / world_bounds_size;
    for (mut marker_node, mut marker_transform, marker) in &mut camera_markers {
        if !world_bounds.is_changed() && !overhead_camera_rig.is_changed() && !marker.is_added() {
            continue;
        }
        let left = Val::Percent((normalized_camera_position.x * 100.0).clamp(0.0, 100.0));
        let top = Val::Percent((normalized_camera_position.y * 100.0).clamp(0.0, 100.0));
        let rotation = Rot2::radians(-overhead_camera_rig.yaw);
        if marker_node.left != left {
            marker_node.left = left;
        }
        if marker_node.top != top {
            marker_node.top = top;
        }
        if marker_transform.rotation != rotation {
            marker_transform.rotation = rotation;
        }
    }
}

pub(crate) fn project_world_subject_positions_to_overview_map_markers(
    world_bounds: Query<Ref<WorldTerrainHorizontalBounds>>,
    all_subjects: Query<
        (
            &GlobalTransform,
            Option<&Visibility>,
            &OverviewSubjectMarkers,
        ),
        Without<OverviewMarker>,
    >,
    changed_subjects: Query<
        (
            &GlobalTransform,
            Option<&Visibility>,
            &OverviewSubjectMarkers,
        ),
        (
            Without<OverviewMarker>,
            Or<(
                Changed<GlobalTransform>,
                Changed<Visibility>,
                Changed<OverviewSubjectMarkers>,
            )>,
        ),
    >,
    mut markers: Query<(&mut Node, &mut Visibility), With<OverviewMarker>>,
) {
    let Ok(bounds) = world_bounds.single() else {
        return;
    };
    let size = bounds.max - bounds.min;
    if !size.is_finite() || size.min_element() <= 0.0 {
        return;
    }
    let changed = bounds.is_changed();
    let affected = changed
        .then(|| all_subjects.iter())
        .into_iter()
        .flatten()
        .chain(
            (!changed)
                .then(|| changed_subjects.iter())
                .into_iter()
                .flatten(),
        );
    for (transform, visibility, members) in affected {
        let position = transform.translation();
        let left = Val::Percent(((position.x - bounds.min.x) / size.x * 100.0).clamp(0.0, 100.0));
        let top = Val::Percent(((position.z - bounds.min.y) / size.y * 100.0).clamp(0.0, 100.0));
        let visible = if visibility == Some(&Visibility::Hidden) {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
        for entity in members.iter() {
            if let Ok((mut node, mut visibility)) = markers.get_mut(entity) {
                if node.left != left {
                    node.left = left;
                }
                if node.top != top {
                    node.top = top;
                }
                visibility.set_if_neq(visible);
            }
        }
    }
}

pub(crate) fn request_entity_selection_from_pressed_overview_map_markers(
    mut presses: MessageReader<PointerPress>,
    pressed_overview_map_markers: Query<&OverviewMarker>,
    mut selection_requests: MessageWriter<SelectionRequest>,
) {
    for press in presses
        .read()
        .filter(|press| press.pointer.id.is_mouse() && press.button == PointerButton::Primary)
    {
        if let Ok(overview_map_marker) = pressed_overview_map_markers.get(press.entity) {
            selection_requests.write(SelectionRequest {
                entity: Some(overview_map_marker.subject),
                source: ActionSource::KeyboardMouse,
            });
        }
    }
}

pub(crate) fn retire_overview_markers_when_subject_loses_inspectability(
    removed: On<Remove<Inspectable>>,
    subjects: Query<&OverviewSubjectMarkers>,
    mut commands: Commands,
) {
    if let Ok(markers) = subjects.get(removed.entity) {
        for entity in markers.iter() {
            commands.entity(entity).try_despawn();
        }
    }
}

fn spawn_world_subject_overview_map_marker(
    commands: &mut Commands,
    projected_layer_parent: Entity,
    world_subject: Entity,
    world_subject_inspectable: &Inspectable,
    marker_frame: Option<Handle<Image>>,
    world_definitions: Option<WorldDefinitionsView<'_>>,
) {
    let mut marker_entity = commands.spawn((
        OverviewMarker {
            subject: world_subject,
        },
        Node {
            position_type: PositionType::Absolute,
            width: Val::Px(38.0),
            height: Val::Px(38.0),
            ..default()
        },
        UiTransform {
            translation: Val2::px(-19.0, -19.0),
            ..default()
        },
        Pickable::default(),
        ChildOf(projected_layer_parent),
    ));
    if let Some(marker_frame) = marker_frame {
        marker_entity.insert(ImageNode::new(marker_frame));
    }
    if let Some(subject_icon) = world_definitions.and_then(|world_definitions| {
        world_definitions
            .find_object(world_subject_inspectable.definition)
            .and_then(|definition| {
                world_definitions.texture_image(openzt2_game_data::AssetId(definition.icon.0))
            })
    }) {
        marker_entity.with_child((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(3.0),
                top: Val::Px(3.0),
                width: Val::Px(32.0),
                height: Val::Px(32.0),
                ..default()
            },
            ImageNode::new(subject_icon),
            Pickable::IGNORE,
        ));
    }
}
