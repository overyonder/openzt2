use bevy::prelude::*;
use openzt2_game_data::ui_document::{
    action::information::InformationViewCategory, overview_map_presentation::UiOverviewLayer,
    widget::UiWidgetRecord,
};
use openzt2_game_data::AssetId;

use crate::assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use crate::plugins::input::input_types::ActionSource;
use crate::plugins::animal_lifecycle::types::Animal;
use crate::plugins::camera::camera_runtime_state_types::OverheadRig;
use crate::plugins::ui::authored_reusable_list_and_table_runtime_types::UiTablePolicy;
use crate::plugins::ui::authored_ui_node_projection_components::UiDocumentOwner;
use crate::plugins::ui::authored_ui_node_projection_components::UiDocumentRoot;
use crate::plugins::ui::authored_ui_node_projection_components::UiNodeId;
use crate::plugins::world_spawn::world_terrain_hydration::WorldTerrainHorizontalBounds;

use super::super::{
    entity_selection_types::{InformationEntitySource, Inspectable, SelectionRequest},
    information_view_types::InformationViewClass,
};
use super::overview_types::{OverviewCameraMarker, OverviewMarker};

pub(in crate::plugins::information) fn create_missing_authored_overview_map_marker_entities(
    overview_map_surfaces: Query<(Entity, &UiNodeId, &UiDocumentOwner, Ref<UiTablePolicy>)>,
    ui_document_roots: Query<&UiDocumentRoot>,
    projected_ui_nodes: Query<(Entity, &UiNodeId, &UiDocumentOwner)>,
    inspectable_world_subjects: Query<(
        Entity,
        &Inspectable,
        Option<&InformationViewClass>,
        Has<Animal>,
    )>,
    existing_overview_markers: Query<(&OverviewMarker, &ChildOf)>,
    existing_camera_markers: Query<&ChildOf, With<OverviewCameraMarker>>,
    ui_documents: Res<Assets<UiDocumentAsset>>,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    mut commands: Commands,
) {
    let Some(world_definitions) = active_world_definitions.get(&world_definition_assets) else {
        return;
    };
    for (_, overview_map_surface_node, document_owner, table_policy) in &overview_map_surfaces {
        if *table_policy != UiTablePolicy::WorldMap {
            continue;
        }
        let Ok(document_root) = ui_document_roots.get(document_owner.0) else {
            continue;
        };
        let Some(ui_document) = ui_documents.get(&document_root.document) else {
            continue;
        };
        let Some(UiWidgetRecord::WorldMap { layers, .. }) = ui_document
            .canonical_ui_document()
            .nodes
            .get(overview_map_surface_node.index as usize)
            .map(|node_record| &node_record.widget)
        else {
            continue;
        };
        for layer_record in layers {
            let authored_parent_node_id = AssetId(layer_record.node.0);
            let Some(projected_layer_parent) = projected_ui_nodes.iter().find_map(
                |(projected_node, candidate_node_id, candidate_owner)| {
                    (candidate_owner.0 == document_owner.0
                        && candidate_node_id.id == authored_parent_node_id)
                        .then_some(projected_node)
                },
            ) else {
                continue;
            };
            if matches!(layer_record.kind, UiOverviewLayer::CameraPosition) {
                if !existing_camera_markers.iter().any(|camera_marker_parent| {
                    camera_marker_parent.parent() == projected_layer_parent
                }) {
                    let Some(camera_marker_icon) = ui_document
                        .cloned_texture_image_handle(AssetId(layer_record.marker_icon.0))
                    else {
                        continue;
                    };
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
                        ImageNode::new(camera_marker_icon),
                        Pickable::IGNORE,
                        ChildOf(projected_layer_parent),
                    ));
                }
                continue;
            }
            let accepted_view_category = match layer_record.kind {
                UiOverviewLayer::Animals => Some(InformationViewCategory::Animals),
                UiOverviewLayer::Buildings => Some(InformationViewCategory::Buildings),
                _ => None,
            };
            let Some(accepted_view_category) = accepted_view_category else {
                continue;
            };
            for (world_subject, world_subject_inspectable, view_class, is_animal) in
                &inspectable_world_subjects
            {
                let subject_view_category = if is_animal {
                    InformationViewCategory::Animals
                } else {
                    view_class
                        .map(|view_class| view_class.0)
                        .unwrap_or(InformationViewCategory::Foliage)
                };
                let subject_is_accepted = subject_view_category == accepted_view_category
                    || (accepted_view_category == InformationViewCategory::Buildings
                        && subject_view_category == InformationViewCategory::Entrances);
                if subject_is_accepted
                    && !overview_map_marker_exists_for_subject(
                        &existing_overview_markers,
                        projected_layer_parent,
                        world_subject,
                    )
                {
                    spawn_world_subject_overview_map_marker(
                        &mut commands,
                        projected_layer_parent,
                        world_subject,
                        world_subject_inspectable,
                        ui_document
                            .cloned_texture_image_handle(AssetId(layer_record.marker_icon.0)),
                        Some(world_definitions),
                    );
                }
            }
        }
    }
}

pub(in crate::plugins::information) fn project_overhead_camera_position_to_overview_map_marker(
    world_bounds: Query<&WorldTerrainHorizontalBounds>,
    overhead_camera_rigs: Query<&OverheadRig>,
    mut camera_markers: Query<(&mut Node, &mut UiTransform), With<OverviewCameraMarker>>,
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
    for (mut marker_node, mut marker_transform) in &mut camera_markers {
        marker_node.left = Val::Percent((normalized_camera_position.x * 100.0).clamp(0.0, 100.0));
        marker_node.top = Val::Percent((normalized_camera_position.y * 100.0).clamp(0.0, 100.0));
        marker_transform.rotation = Rot2::radians(-overhead_camera_rig.yaw);
    }
}

pub(in crate::plugins::information) fn project_world_subject_positions_to_overview_map_markers(
    world_bounds: Query<&WorldTerrainHorizontalBounds>,
    world_subjects: Query<(&GlobalTransform, Option<&Visibility>), Without<OverviewMarker>>,
    mut overview_map_markers: Query<(&OverviewMarker, &mut Node, &mut Visibility)>,
) {
    let Ok(world_bounds) = world_bounds.single() else {
        return;
    };
    let world_bounds_size = world_bounds.max - world_bounds.min;
    if !world_bounds_size.is_finite() || world_bounds_size.x <= 0.0 || world_bounds_size.y <= 0.0 {
        return;
    }
    for (overview_map_marker, mut marker_node, mut marker_visibility) in &mut overview_map_markers {
        let Ok((subject_transform, subject_visibility)) =
            world_subjects.get(overview_map_marker.subject)
        else {
            *marker_visibility = Visibility::Hidden;
            continue;
        };
        let subject_is_visible =
            subject_visibility.is_none_or(|visibility| *visibility != Visibility::Hidden);
        let subject_world_position = subject_transform.translation();
        marker_node.left = Val::Percent(
            ((subject_world_position.x - world_bounds.min.x) / world_bounds_size.x * 100.0)
                .clamp(0.0, 100.0),
        );
        marker_node.top = Val::Percent(
            ((subject_world_position.z - world_bounds.min.y) / world_bounds_size.y * 100.0)
                .clamp(0.0, 100.0),
        );
        *marker_visibility = if subject_is_visible {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
}

pub(in crate::plugins::information) fn request_entity_selection_from_pressed_overview_map_markers(
    pressed_overview_map_markers: Query<(&OverviewMarker, &Interaction), Changed<Interaction>>,
    mut selection_requests: MessageWriter<SelectionRequest>,
) {
    for (overview_map_marker, interaction) in &pressed_overview_map_markers {
        if *interaction == Interaction::Pressed {
            selection_requests.write(SelectionRequest {
                entity: Some(overview_map_marker.subject),
                source: ActionSource::KeyboardMouse,
            });
        }
    }
}

pub(in crate::plugins::information) fn remove_overview_map_markers_for_retired_subjects(
    overview_map_markers: Query<(Entity, &OverviewMarker)>,
    inspectable_world_subjects: Query<(), With<Inspectable>>,
    mut commands: Commands,
) {
    for (marker_entity, overview_map_marker) in &overview_map_markers {
        if inspectable_world_subjects
            .get(overview_map_marker.subject)
            .is_err()
        {
            commands.entity(marker_entity).despawn();
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
        InformationEntitySource(world_subject),
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
        Interaction::None,
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

fn overview_map_marker_exists_for_subject(
    existing_overview_markers: &Query<(&OverviewMarker, &ChildOf)>,
    projected_layer_parent: Entity,
    world_subject: Entity,
) -> bool {
    existing_overview_markers
        .iter()
        .any(|(overview_map_marker, parent_relationship)| {
            overview_map_marker.subject == world_subject
                && parent_relationship.parent() == projected_layer_parent
        })
}
