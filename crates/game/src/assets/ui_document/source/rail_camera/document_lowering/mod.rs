//! Rail-camera source-document lowering into the scene-prefab document.

use std::{collections::BTreeSet, io};

use crate::assets::source_document::ordered_source_document_types::{
    OrderedSourceDocument, OrderedSourceDocumentNode,
};
use openzt2_game_data::{
    scene_prefab::{
        PrefabDirectionalLightKey, PrefabEntity, PrefabEnvironmentKey, PrefabRailCamera,
        PrefabRailCameraCommand, PrefabRailCameraHotkey, PrefabRailCameraKey, PrefabRenderable,
        PrefabTransform, ScenePrefabAssetDependency, ScenePrefabAssetDependencyKind,
        ScenePrefabDocument, ScenePrefabEntityFlags, ScenePrefabRenderableVisibilityFlags,
    },
    AssetId,
};

use crate::assets::source_coordinate_conversion::{
    conjugate_source_z_up_rotation_into_bevy_y_up_basis,
    convert_row_major_rotation_matrix_to_xyzw_quaternion,
    convert_source_z_up_vector_to_bevy_y_up_coordinates,
};

use super::source_value_reading::{
    find_source_descendant, invalid_source_data, optional_source_boolean_attribute,
    parse_finite_source_number, read_source_rotation_matrix, read_source_vector_attributes,
    read_source_vector_string, required_finite_source_number, required_source_attribute,
    source_element_child,
};

pub(in crate::assets::ui_document::source) fn lower_rail_camera_documents_to_scene_prefab_documents(
    documents: &[OrderedSourceDocument],
) -> io::Result<Vec<(String, ScenePrefabDocument)>> {
    documents
        .iter()
        .filter_map(|document| {
            find_source_descendant(document.root.element_children(), "ZTRailCam").map(
                |rail_camera_node| {
                    lower_one_rail_camera_document_to_scene_prefab_document(
                        document,
                        rail_camera_node,
                    )
                },
            )
        })
        .collect()
}

fn lower_one_rail_camera_document_to_scene_prefab_document(
    document: &OrderedSourceDocument,
    rail: &OrderedSourceDocumentNode,
) -> io::Result<(String, ScenePrefabDocument)> {
    let path = format!("{}#RailCamera", document.path.as_str());
    let mut root = create_rail_camera_scene_prefab_entity(
        &path,
        u32::MAX,
        PrefabTransform {
            translation_m: [0.0; 3],
            rotation_xyzw: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0; 3],
        },
        true,
    );
    let mut entities = Vec::new();
    let mut dependencies = Vec::new();

    if let Some(objects) = source_element_child(rail, "objects") {
        for object in objects.element_children() {
            let physical = source_element_child(object, "BFPhysObj").ok_or_else(|| {
                invalid_source_data(format!(
                    "rail-camera object <{}> has no BFPhysObj",
                    object.name
                ))
            })?;
            let component = physical
                .element_children()
                .find(|node| {
                    matches!(
                        node.name.as_str(),
                        "BFRSceneGraphComponent" | "BFSceneGraphComponent" | "BFSimpleLODComponent"
                    )
                })
                .ok_or_else(|| {
                    invalid_source_data(format!(
                        "rail-camera object <{}> has no scene component",
                        object.name
                    ))
                })?;
            let source_model =
                required_source_attribute(component, "modelfile")?.replace('\\', "/");
            let model_path = source_model;
            let model = AssetId::from_virtual_path(&model_path);
            if dependencies
                .iter()
                .all(|dependency: &ScenePrefabAssetDependency| dependency.asset_id != model)
            {
                dependencies.push(ScenePrefabAssetDependency {
                    asset_id: model,
                    asset_path: model_path,
                    asset_kind: ScenePrefabAssetDependencyKind::Model,
                });
            }
            let translation = source_element_child(physical, "position")
                .map(read_source_vector_attributes)
                .transpose()?
                .unwrap_or([0.0; 3]);
            let rotation = source_element_child(physical, "rotation")
                .map(read_source_rotation_matrix)
                .transpose()?
                .map(|matrix| {
                    convert_row_major_rotation_matrix_to_xyzw_quaternion(
                        conjugate_source_z_up_rotation_into_bevy_y_up_basis(matrix),
                    )
                })
                .unwrap_or_else(|| {
                    let yaw = component
                        .attribute("worldyaw")
                        .and_then(|value| parse_finite_source_number(value).ok())
                        .unwrap_or(0.0);
                    [0.0, (yaw * 0.5).sin(), 0.0, (yaw * 0.5).cos()]
                });
            let scale = component
                .attribute("scale")
                .map(parse_finite_source_number)
                .transpose()?
                .unwrap_or(1.0);
            let visible = component.attribute("visible") != Some("0");
            let y_flip = optional_source_boolean_attribute(component, "yflip")?.unwrap_or(false);
            let index = u32::try_from(entities.len() + 1)
                .map_err(|_| invalid_source_data("too many rail-camera objects"))?;
            root.children.push(index);
            let mut object_entity = create_rail_camera_scene_prefab_entity(
                &object.name,
                0,
                PrefabTransform {
                    translation_m: convert_source_z_up_vector_to_bevy_y_up_coordinates(translation),
                    rotation_xyzw: rotation,
                    scale: [scale, scale, if y_flip { -scale } else { scale }],
                },
                visible,
            );
            object_entity.renderables.push(PrefabRenderable {
                model,
                scene_name: String::new(),
                material_overrides: Vec::new(),
                visibility: rail_camera_renderable_visibility_flags(visible),
            });
            entities.push(object_entity);
        }
    }
    entities.insert(0, root);

    let keys = source_element_child(rail, "rail")
        .into_iter()
        .flat_map(OrderedSourceDocumentNode::element_children)
        .map(|key| {
            Ok(PrefabRailCameraKey {
                translation_m: convert_source_z_up_vector_to_bevy_y_up_coordinates(
                    read_source_vector_string(required_source_attribute(key, "translate")?)?,
                ),
                rotation_xyz: read_source_vector_string(required_source_attribute(key, "rotate")?)?,
            })
        })
        .collect::<io::Result<Vec<_>>>()?;
    let environment_nodes = source_element_child(rail, "lights")
        .and_then(|lights| source_element_child(lights, "tods"))
        .into_iter()
        .flat_map(OrderedSourceDocumentNode::element_children)
        .collect::<Vec<_>>();
    let environment_keys = environment_nodes
        .iter()
        .map(|key| {
            Ok(PrefabEnvironmentKey {
                time: required_finite_source_number(key, "t")?,
                ambient_srgb: read_source_vector_string(required_source_attribute(
                    key, "ambient",
                )?)?,
                shadow_strength: required_finite_source_number(key, "shadow")?,
            })
        })
        .collect::<io::Result<Vec<_>>>()?;
    let environment_nodes = environment_nodes.as_slice();
    let directional_light_keys = source_element_child(rail, "lights")
        .and_then(|lights| source_element_child(lights, "lights"))
        .into_iter()
        .flat_map(OrderedSourceDocumentNode::element_children)
        .flat_map(|light| {
            light.element_children().map(move |key| {
                Ok(PrefabDirectionalLightKey {
                    light: AssetId::from_key(&light.name),
                    time: read_rail_camera_directional_light_key_time(key, environment_nodes)?,
                    diffuse_srgb: read_source_vector_string(required_source_attribute(
                        key, "diffuse",
                    )?)?,
                    specular_srgb: read_source_vector_string(required_source_attribute(
                        key, "specular",
                    )?)?,
                    direction: convert_source_z_up_vector_to_bevy_y_up_coordinates(
                        read_source_vector_string(required_source_attribute(key, "direction")?)?,
                    ),
                    intensity: required_finite_source_number(key, "intensity")?,
                })
            })
        })
        .collect::<io::Result<Vec<_>>>()?;
    validate_rail_camera_key_times_are_authored_order(
        environment_keys.iter().map(|key| key.time),
        "rail-camera environment",
    )?;
    for light in directional_light_keys
        .iter()
        .map(|key| key.light)
        .collect::<BTreeSet<_>>()
    {
        validate_rail_camera_key_times_are_authored_order(
            directional_light_keys
                .iter()
                .filter(|key| key.light == light)
                .map(|key| key.time),
            "rail-camera directional light",
        )?;
    }
    let hotkeys = source_element_child(rail, "UIHotKeys")
        .into_iter()
        .flat_map(OrderedSourceDocumentNode::element_children)
        .map(lower_rail_camera_hotkey)
        .collect::<io::Result<Vec<_>>>()?
        .into_iter()
        .flatten()
        .collect();
    let document = ScenePrefabDocument {
        entities,
        automatic_placement_bounds_xz: None,
        rail_camera: Some(PrefabRailCamera {
            seconds_per_segment: required_finite_source_number(rail, "time")?,
            render_size: if let Some(region) = source_element_child(rail, "UIRegion") {
                [
                    read_positive_rail_camera_render_dimension(region, "w")?,
                    read_positive_rail_camera_render_dimension(region, "h")?,
                ]
            } else {
                [1024, 768]
            },
            keys,
            environment_keys,
            directional_light_keys,
            hotkeys,
        }),
        dependencies,
    };
    Ok((path, document))
}

fn create_rail_camera_scene_prefab_entity(
    key: &str,
    parent: u32,
    transform: PrefabTransform,
    visible: bool,
) -> PrefabEntity {
    PrefabEntity {
        stable_id: AssetId::from_key(key),
        attachment_id: AssetId::from_key(key),
        model_joint_binding: None,
        parent,
        transform,
        children: Vec::new(),
        flags: (if visible {
            ScenePrefabEntityFlags::VISIBLE
        } else {
            ScenePrefabEntityFlags(0)
        }) | ScenePrefabEntityFlags::ACTIVE
            | ScenePrefabEntityFlags::STATIC,
        rotation_cycles: Vec::new(),
        transform_animations: Vec::new(),
        renderables: Vec::new(),
        colliders: Vec::new(),
        lods: Vec::new(),
        billboards: Vec::new(),
        lights: Vec::new(),
        effects: Vec::new(),
    }
}

fn rail_camera_renderable_visibility_flags(visible: bool) -> ScenePrefabRenderableVisibilityFlags {
    (if visible {
        ScenePrefabRenderableVisibilityFlags::VISIBLE
    } else {
        ScenePrefabRenderableVisibilityFlags(0)
    }) | ScenePrefabRenderableVisibilityFlags::CAST_SHADOW
        | ScenePrefabRenderableVisibilityFlags::RECEIVE_SHADOW
}

fn lower_rail_camera_hotkey(
    node: &OrderedSourceDocumentNode,
) -> io::Result<Option<PrefabRailCameraHotkey>> {
    let message = required_source_attribute(node, "msg")?;
    if !matches!(message, "ZT_SET_COMMAND_STATE" | "ZT_CLEAR_COMMAND_STATE") {
        return Ok(None);
    }
    let command = match required_source_attribute(node, "string")?
        .to_ascii_lowercase()
        .as_str()
    {
        "left" => PrefabRailCameraCommand::Left,
        "right" => PrefabRailCameraCommand::Right,
        "up" => PrefabRailCameraCommand::Up,
        "down" => PrefabRailCameraCommand::Down,
        "in" => PrefabRailCameraCommand::In,
        "out" => PrefabRailCameraCommand::Out,
        "rollleft" => PrefabRailCameraCommand::RollLeft,
        "rollright" => PrefabRailCameraCommand::RollRight,
        "record" => PrefabRailCameraCommand::Record,
        "play" => PrefabRailCameraCommand::Play,
        "save" => PrefabRailCameraCommand::Save,
        "load" => PrefabRailCameraCommand::Load,
        "clear" => PrefabRailCameraCommand::Clear,
        "fast" => PrefabRailCameraCommand::Fast,
        command => {
            return Err(invalid_source_data(format!(
                "unsupported rail-camera command {command}"
            )));
        }
    };
    Ok(Some(PrefabRailCameraHotkey {
        key_code: required_source_attribute(node, "code")?
            .parse()
            .map_err(|_| invalid_source_data("invalid rail-camera key code"))?,
        triggered_on_press: node.name == "down",
        active: message == "ZT_SET_COMMAND_STATE",
        command,
    }))
}

fn validate_rail_camera_key_times_are_authored_order(
    mut times: impl Iterator<Item = f32>,
    label: &str,
) -> io::Result<()> {
    times
        .try_fold(f32::NEG_INFINITY, |previous, time| {
            if !time.is_finite() || time < previous {
                return Err(invalid_source_data(format!(
                    "{label} keys are not in authored time order"
                )));
            }
            Ok(time)
        })
        .map(|_| ())
}

fn read_rail_camera_directional_light_key_time(
    key: &OrderedSourceDocumentNode,
    environment: &[&OrderedSourceDocumentNode],
) -> io::Result<f32> {
    key.attribute("t")
        .or_else(|| {
            environment
                .iter()
                .find(|candidate| candidate.name.eq_ignore_ascii_case(&key.name))
                .and_then(|candidate| candidate.attribute("t"))
        })
        .ok_or_else(|| invalid_source_data(format!("<{}> has no rail-camera time", key.name)))
        .and_then(parse_finite_source_number)
}

fn read_positive_rail_camera_render_dimension(
    node: &OrderedSourceDocumentNode,
    name: &str,
) -> io::Result<u32> {
    let value = required_finite_source_number(node, name)?;
    if !value.is_finite() || value <= 0.0 || value.fract() != 0.0 || value > u32::MAX as f32 {
        return Err(invalid_source_data(format!(
            "invalid rail-camera {name} dimension {value}"
        )));
    }
    Ok(value as u32)
}
