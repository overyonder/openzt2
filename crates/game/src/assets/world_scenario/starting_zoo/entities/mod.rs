use std::{
    collections::{BTreeMap, BTreeSet},
    io,
};

use crate::assets::world_scenario::world_scenario_source_reference_resolution::{
    SourceReference, SourceReferenceKind,
};

use openzt2_game_data::{
    world_scenario::{
        StartingFenceRecord, StartingPathRecord, StartingTopologyNodeRecord,
        StartingZooEntityTransform, StartingZooRecord, StartingZooSpawnEntityFlags,
        StartingZooSpawnEntityRecord,
    },
    AssetId,
};

use crate::assets::source_coordinate_conversion::{
    conjugate_source_z_up_rotation_into_bevy_y_up_basis,
    convert_row_major_rotation_matrix_to_xyzw_quaternion,
    convert_source_z_up_vector_to_bevy_y_up_coordinates,
};

use crate::assets::source_document::{
    ordered_source_document_types::{OrderedSourceDocument, OrderedSourceDocumentNode},
    path::AssetPath,
};

use super::{at, child, descendants, local, required, required_f32};

pub(super) fn lower_starting_zoo_entities_paths_and_fences_into_record(
    document: &OrderedSourceDocument,
    dependencies: &[SourceReference],
    actor_scene_paths: &BTreeMap<String, String>,
    starting_zoo_record: &mut StartingZooRecord,
) -> io::Result<()> {
    let roots = document.root.element_children().collect::<Vec<_>>();
    let manager = descendants(&roots)
        .find(|node| local(&node.name) == "BFGManager")
        .ok_or_else(|| super::at_root(document, "starting zoo has no BFGManager"))?;
    let entity_list = child(manager, "entityList").ok_or_else(|| {
        at(
            document,
            manager,
            "starting zoo BFGManager has no entityList",
        )
    })?;
    let mut renderables = Vec::new();
    let mut path_rows = Vec::new();
    let fences = entity_list
        .element_children()
        .filter(|node| local(&node.name) == "ZTFenceEntity")
        .collect::<Vec<_>>();
    for entity in entity_list
        .element_children()
        .filter(|node| local(&node.name) == "BFGEntity")
    {
        let binder = required(document, entity, "binderUniqueName")?;
        let definition = normalized(
            binder
                .split_once('/')
                .map_or(binder, |(definition, _)| definition),
        );
        let subcomponents = child(entity, "subComponents");
        let direct_presentation = subcomponents.and_then(starting_entity_presentation_source);
        let direct_scene =
            direct_presentation.and_then(|(_, model)| resolve_scene(model, actor_scene_paths));
        let presentation_scale = direct_presentation
            .map(|(component, _)| starting_entity_presentation_scale(document, component))
            .transpose()?
            .unwrap_or(1.0);
        if let Some(path) = subcomponents.and_then(|components| {
            components.element_children().find(|node| {
                local(&node.name) == "ZTPath"
                    && node.attribute("texture").is_some()
                    && node.attribute("curb").is_some()
            })
        }) {
            path_rows.push((entity, definition, path));
            continue;
        }
        let helper = subcomponents.is_some_and(|components| {
            components.element_children().any(|node| {
                let name = local(&node.name);
                name.starts_with("ZTPath")
                    || (name.starts_with("ZTTransport") && name.contains("Track"))
            })
        });
        if !helper {
            if direct_scene.is_some() || subcomponents.and_then(starting_entity_physical).is_some()
            {
                renderables.push((entity, definition, direct_scene, presentation_scale));
            } else {
                return Err(at(
                    document,
                    entity,
                    format!(
                        "starting-zoo entity {definition} has no physical, path, or helper binding"
                    ),
                ));
            }
        }
    }

    let mut used_ids = BTreeSet::new();
    for entity in renderables
        .iter()
        .map(|(node, _, _, _)| *node)
        .chain(path_rows.iter().map(|(node, _, _)| *node))
        .chain(fences.iter().copied())
    {
        let Some(raw) = entity.attribute("globalID") else {
            continue;
        };
        let id = raw
            .parse::<u64>()
            .ok()
            .filter(|id| *id != 0 && *id != u64::MAX)
            .ok_or_else(|| {
                at(
                    document,
                    entity,
                    "starting-zoo entity globalID is not a usable nonzero u64",
                )
            })?;
        if !used_ids.insert(id) {
            return Err(at(
                document,
                entity,
                format!("duplicate starting-zoo entity globalID {id}"),
            ));
        }
    }
    let mut next_id = 1_u64;
    let mut persistent_id =
        |entity: &OrderedSourceDocumentNode, retain_source: bool| -> io::Result<u64> {
            if retain_source {
                if let Some(raw) = entity.attribute("globalID") {
                    return raw
                        .parse()
                        .map_err(|_| at(document, entity, "invalid validated persistent ID"));
                }
            }
            while used_ids.contains(&next_id) {
                next_id = next_id.checked_add(1).ok_or_else(|| {
                    at(
                        document,
                        entity,
                        "starting-zoo persistent ID space is exhausted",
                    )
                })?;
            }
            let assigned = next_id;
            used_ids.insert(assigned);
            next_id = next_id.checked_add(1).unwrap_or(u64::MAX);
            Ok(assigned)
        };

    let entities = renderables
        .into_iter()
        .map(|(entity, definition, scene, presentation_scale)| {
            let physical = child(entity, "subComponents")
                .and_then(starting_entity_physical)
                .ok_or_else(|| {
                    at(
                        document,
                        entity,
                        "renderable starting-zoo entity has no presentation BFPhysObj",
                    )
                })?;
            let position = child(physical, "position")
                .ok_or_else(|| at(document, physical, "starting-zoo BFPhysObj has no position"))?;
            let rotation = child(physical, "rotation")
                .ok_or_else(|| at(document, physical, "starting-zoo BFPhysObj has no rotation"))?;
            let mut rows = [[0.0; 3]; 3];
            for (index, row) in rows.iter_mut().enumerate() {
                let source = child(rotation, &format!("row{index}")).ok_or_else(|| {
                    at(
                        document,
                        rotation,
                        format!("starting-zoo rotation has no row{index}"),
                    )
                })?;
                *row = [
                    required_f32(document, source, "x")?,
                    required_f32(document, source, "y")?,
                    required_f32(document, source, "z")?,
                ];
            }
            let quaternion = z_up_rotation_from_rows(rows)
                .ok_or_else(|| at(document, rotation, "starting-zoo rotation is not finite"))?;
            let mut flags = StartingZooSpawnEntityFlags::ACTIVE
                .with_additional_flags(StartingZooSpawnEntityFlags::SAVE_RELEVANT)
                .with_additional_flags(StartingZooSpawnEntityFlags::PLAYER_OWNED);
            if entity.attribute("setVisible") != Some("false") {
                flags = flags.with_additional_flags(StartingZooSpawnEntityFlags::VISIBLE);
            }
            let prefab = scene
                .map(|path| {
                    let id = AssetId::from_virtual_path(&path);
                    dependencies
                        .iter()
                        .find(|dependency| {
                            dependency.kind == SourceReferenceKind::Scene
                                && dependency.id == id
                        })
                        .or_else(|| {
                            dependencies.iter().find(|dependency| {
                                dependency.kind == SourceReferenceKind::Scene
                                    && scene_stem(&dependency.path)
                                        .eq_ignore_ascii_case(scene_stem(&path))
                            })
                        })
                        .map(|dependency| dependency.id)
                        .ok_or_else(|| {
                            at(
                                document,
                                entity,
                                format!(
                                    "starting-zoo entity {definition} references unresolved scene {path}"
                                ),
                            )
                        })
                })
                .transpose()?
                .unwrap_or_default();
            Ok(StartingZooSpawnEntityRecord {
                persistent_id: persistent_id(entity, true)?,
                prefab,
                definition: AssetId::from_key(&definition),
                transform: StartingZooEntityTransform {
                    translation_m: convert_source_z_up_vector_to_bevy_y_up_coordinates([
                        required_f32(document, position, "x")?,
                        required_f32(document, position, "y")?,
                        required_f32(document, position, "z")?,
                    ]),
                    rotation_xyzw: quaternion,
                    scale: [presentation_scale; 3],
                },
                parent: u32::MAX,
                flags,
            })
        })
        .collect::<io::Result<Vec<_>>>()?;

    let paths = path_rows
        .into_iter()
        .map(|(entity, definition, _)| {
            let physical = child(entity, "subComponents")
                .and_then(|components| {
                    components
                        .element_children()
                        .find(|node| {
                            local(&node.name) == "BFPhysObj"
                                && child(node, "BFTerrainDecalComponent").is_some()
                        })
                        .or_else(|| {
                            components.element_children().find(|node| {
                                local(&node.name) == "BFPhysObj"
                                    && child(node, "position").is_some()
                            })
                        })
                })
                .ok_or_else(|| {
                    at(
                        document,
                        entity,
                        "starting path has no positioned BFPhysObj",
                    )
                })?;
            let position = child(physical, "position")
                .ok_or_else(|| at(document, physical, "starting path has no position"))?;
            Ok(StartingPathRecord {
                persistent_id: persistent_id(entity, true)?,
                definition: AssetId::from_key(&definition),
                position_m: convert_source_z_up_vector_to_bevy_y_up_coordinates([
                    required_f32(document, position, "x")?,
                    required_f32(document, position, "y")?,
                    required_f32(document, position, "z")?,
                ]),
            })
        })
        .collect::<io::Result<Vec<_>>>()?;

    let mut topology_nodes = Vec::new();
    let mut node_indexes = BTreeMap::<[i64; 2], u32>::new();
    let fences = fences
        .into_iter()
        .map(|fence| {
            let source_definition = normalized(required(document, fence, "ZTFenceType")?);
            let source = fence_position(document, fence)?;
            let direction =
                fence_direction(document, fence, required_f32(document, fence, "rotation")?)?;
            let other = [
                source[0] + direction[0] * 3.0,
                source[1] + direction[1] * 3.0,
                source[2],
            ];
            let mut endpoints = [0_u32; 2];
            for (index, source) in [source, other].into_iter().enumerate() {
                let position = convert_source_z_up_vector_to_bevy_y_up_coordinates(source);
                let key = [position[0], position[2]]
                    .map(|value| (f64::from(value) * 1000.0).round() as i64);
                endpoints[index] = if let Some(existing) = node_indexes.get(&key) {
                    *existing
                } else {
                    let node = u32::try_from(topology_nodes.len()).map_err(|_| {
                        at(document, fence, "starting fence endpoint count exceeds u32")
                    })?;
                    topology_nodes.push(StartingTopologyNodeRecord {
                        persistent_id: persistent_id(fence, false)?,
                        position_m: position,
                    });
                    node_indexes.insert(key, node);
                    node
                };
            }
            if endpoints[0] == endpoints[1] {
                return Err(at(
                    document,
                    fence,
                    "starting fence collapses to one topology endpoint",
                ));
            }
            Ok(StartingFenceRecord {
                persistent_id: persistent_id(fence, true)?,
                definition: AssetId::from_key(&source_definition),
                a: endpoints[0],
                b: endpoints[1],
                gate: false,
                protected: fence.attribute("isProtected") == Some("true"),
            })
        })
        .collect::<io::Result<Vec<_>>>()?;
    starting_zoo_record.entities = entities;
    starting_zoo_record.paths = paths;
    starting_zoo_record.topology_nodes = topology_nodes;
    starting_zoo_record.fences = fences;
    Ok(())
}

fn scene_stem(path: &str) -> &str {
    let path = path.strip_suffix("#Scene").unwrap_or(path);
    path.rsplit_once('.').map_or(path, |(stem, _)| stem)
}

fn normalized(value: &str) -> String {
    value
        .trim()
        .replace('\\', "/")
        .trim_start_matches('/')
        .to_ascii_lowercase()
}

fn resolve_scene(reference: &str, actor_scene_paths: &BTreeMap<String, String>) -> Option<String> {
    let normalized = AssetPath::new(reference).key();
    let path = if normalized.ends_with(".bfm") {
        actor_scene_paths.get(&normalized)?.clone()
    } else {
        normalized
    };
    Some(if path.contains('#') {
        path
    } else {
        crate::assets::model_source::native_model_source_lowering::
            native_model_scene_labelled_asset_path(&path)
    })
}

fn first_nonblank_attribute_and_owner_beneath_starting_entity<'a>(
    node: &'a OrderedSourceDocumentNode,
    attribute: &str,
) -> Option<(&'a OrderedSourceDocumentNode, &'a str)> {
    node.attribute(attribute)
        .filter(|value| !value.trim().is_empty())
        .map(|value| (node, value))
        .or_else(|| {
            node.element_children().find_map(|child| {
                first_nonblank_attribute_and_owner_beneath_starting_entity(child, attribute)
            })
        })
}

fn starting_entity_presentation_source(
    node: &OrderedSourceDocumentNode,
) -> Option<(&OrderedSourceDocumentNode, &str)> {
    first_nonblank_attribute_and_owner_beneath_starting_entity(node, "modelfile")
        .or_else(|| first_nonblank_attribute_and_owner_beneath_starting_entity(node, "actorfile"))
}

fn starting_entity_presentation_scale(
    document: &OrderedSourceDocument,
    presentation_component: &OrderedSourceDocumentNode,
) -> io::Result<f32> {
    presentation_component
        .attribute("scale")
        .map_or(Ok(1.0), |source| {
            source
                .parse::<f32>()
                .ok()
                .filter(|scale| scale.is_finite() && *scale > 0.0)
                .ok_or_else(|| {
                    at(
                        document,
                        presentation_component,
                        format!("starting-zoo presentation scale is not positive finite: {source}"),
                    )
                })
        })
}

fn starting_entity_physical(
    subcomponents: &OrderedSourceDocumentNode,
) -> Option<&OrderedSourceDocumentNode> {
    let physical = subcomponents
        .element_children()
        .filter(|node| local(&node.name) == "BFPhysObj")
        .collect::<Vec<_>>();
    physical
        .iter()
        .copied()
        .find(|node| {
            node.attribute("binderUniqueName")
                .and_then(|name| name.split('/').next())
                .is_some_and(|name| name.eq_ignore_ascii_case("mainObj"))
        })
        .or_else(|| {
            physical
                .iter()
                .copied()
                .find(|node| starting_entity_presentation_source(node).is_some())
        })
        .or_else(|| match physical.as_slice() {
            [only] => Some(*only),
            _ => None,
        })
}

fn fence_position(
    document: &OrderedSourceDocument,
    fence: &OrderedSourceDocumentNode,
) -> io::Result<[f32; 3]> {
    let values = required(document, fence, "position")?
        .split_ascii_whitespace()
        .map(str::parse::<f32>)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| at(document, fence, "starting fence position is not numeric"))?;
    let position: [f32; 3] = values.try_into().map_err(|_| {
        at(
            document,
            fence,
            "starting fence position does not have three coordinates",
        )
    })?;
    position
        .into_iter()
        .all(f32::is_finite)
        .then_some(position)
        .ok_or_else(|| at(document, fence, "starting fence position is not finite"))
}

fn fence_direction(
    document: &OrderedSourceDocument,
    fence: &OrderedSourceDocumentNode,
    rotation: f32,
) -> io::Result<[f32; 2]> {
    // Saved ZTFence headings are clockwise in source XY: the native selector
    // endpoint is (x + span*cos(rotation), y - span*sin(rotation)). Convert
    // that endpoint to Bevy only after interpreting the source heading.
    let direction = [
        if rotation.cos().abs() < 0.5 {
            0.0
        } else {
            rotation.cos().signum()
        },
        if rotation.sin().abs() < 0.5 {
            0.0
        } else {
            -rotation.sin().signum()
        },
    ];
    let authored = rotation.rem_euclid(std::f32::consts::TAU);
    let quantized = (-direction[1])
        .atan2(direction[0])
        .rem_euclid(std::f32::consts::TAU);
    let error = (authored - quantized).abs();
    if !rotation.is_finite()
        || direction == [0.0, 0.0]
        || error.min(std::f32::consts::TAU - error) > 0.001
    {
        Err(at(
            document,
            fence,
            "starting fence rotation is not on the authored eight-way lattice",
        ))
    } else {
        Ok(direction)
    }
}

fn z_up_rotation_from_rows(rows: [[f32; 3]; 3]) -> Option<[f32; 4]> {
    let source = [
        rows[0][0], rows[0][1], rows[0][2], rows[1][0], rows[1][1], rows[1][2], rows[2][0],
        rows[2][1], rows[2][2],
    ];
    source.iter().all(|value| value.is_finite()).then(|| {
        convert_row_major_rotation_matrix_to_xyzw_quaternion(
            conjugate_source_z_up_rotation_into_bevy_y_up_basis(source),
        )
    })
}

#[cfg(test)]
mod starting_fence_heading_tests {
    use super::*;
    use crate::assets::source_document::blue_fang_source_document_parsing::parse_blue_fang_source_document;

    #[test]
    fn saved_fence_headings_follow_native_clockwise_selector_endpoints() {
        let document = parse_blue_fang_source_document(
            AssetPath::new("heading.xml"),
            b"<root><ZTFenceEntity/></root>",
        )
        .expect("valid fence document");
        let fence = document.root.element_children().next().expect("fence");
        let directions = [
            [1.0, 0.0],
            [1.0, -1.0],
            [0.0, -1.0],
            [-1.0, -1.0],
            [-1.0, 0.0],
            [-1.0, 1.0],
            [0.0, 1.0],
            [1.0, 1.0],
        ];
        for (heading, expected) in directions.into_iter().enumerate() {
            for wrap in [-1.0, 0.0, 1.0] {
                let rotation =
                    heading as f32 * std::f32::consts::FRAC_PI_4 + wrap * std::f32::consts::TAU;
                assert_eq!(
                    fence_direction(&document, fence, rotation).unwrap(),
                    expected
                );
            }
        }
        assert!(fence_direction(&document, fence, 0.2).is_err());
        assert!(fence_direction(&document, fence, f32::NAN).is_err());
    }
}

#[cfg(test)]
mod tests;
