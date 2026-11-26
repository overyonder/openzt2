//! Gameplay-mode cursor and placement-preview source parsing.

use crate::{
    source_document::ordered_source_document_types::{
        OrderedSourceDocument, OrderedSourceDocumentChildren, OrderedSourceDocumentNode,
    },
    source_document::ui::{model::SourceUiHotkey, parser::SourceUiDocument},
    ui_document::source::lower::authored_ui_document_lowering::{
        AuthoredUiInteractionCursors, AuthoredUiPlacementPreview,
    },
};

use super::super::ui_source_document_gap::{
    UiSourceDocumentFamily, UiSourceDocumentGap, UiSourceDocumentGapKind,
};

fn find_nested_ordered_source_document_node_by_name<'a>(
    children: &'a OrderedSourceDocumentChildren,
    tag: &str,
) -> Option<&'a OrderedSourceDocumentNode> {
    children.iter().find_map(|node| {
        node.name
            .eq_ignore_ascii_case(tag)
            .then_some(node)
            .or_else(|| find_nested_ordered_source_document_node_by_name(&node.children, tag))
    })
}

pub(super) fn parse_authored_overhead_camera_hotkeys(
    document: &OrderedSourceDocument,
) -> Vec<SourceUiHotkey> {
    let Some(overhead) =
        find_nested_ordered_source_document_node_by_name(&document.root.children, "ZTOverheadMode")
    else {
        return Vec::new();
    };
    let Some(camera) = find_nested_ordered_source_document_node_by_name(
        &overhead.children,
        "ZTCameraCommandReader",
    ) else {
        return Vec::new();
    };
    parse_mode_hotkeys(document, camera)
}

fn parse_mode_hotkeys(
    document: &OrderedSourceDocument,
    mode: &OrderedSourceDocumentNode,
) -> Vec<SourceUiHotkey> {
    use crate::source_document::ordered_source_document_types::OrderedSourceDocumentChild;

    let Some(hotkeys) = source_element_child(mode, "UIHotKeys") else {
        return Vec::new();
    };
    let root = OrderedSourceDocumentNode::new_synthetic_ordered_source_document_node(
        "UILayout".into(),
        Vec::new(),
        OrderedSourceDocumentChildren::from_ordered_source_document_child_items(vec![
            OrderedSourceDocumentChild::Element(Box::new(hotkeys.clone())),
        ]),
        mode.span,
    );
    SourceUiDocument::parse(&OrderedSourceDocument {
        path: document.path.clone(),
        format: document.format,
        root,
    })
    .root
    .hotkeys
}

pub(in crate::ui_document::source) fn lower_first_person_hotkeys(
    documents: &[OrderedSourceDocument],
) -> io::Result<
    Vec<super::super::super::ui_document_asset_types_and_borrowing_queries::FirstPersonModeHotkeys>,
> {
    use super::super::super::ui_document_asset_types_and_borrowing_queries::FirstPersonModeHotkeys;
    use crate::source_document::ui::model::SourceUiHotkeyTrigger;
    use crate::ui_document::source::lower::{
        authored_ui_event_action_lowering::camera_command_lowering::lower_first_person_command,
        authored_ui_hotkey_lowering::character_code_virtual_key,
    };
    use openzt2_game_data::{
        ui_document::{
            action::UiActionRecord,
            hotkey::{
                UiDocumentHotkeyControlState, UiDocumentHotkeyDefinition, UiDocumentHotkeyTrigger,
            },
        },
        AssetId,
    };
    use std::collections::BTreeMap;

    let Some(modes) = documents
        .iter()
        .find(|doc| doc.path.key() == "ui/modes/modes.xml")
    else {
        return Ok(Vec::new());
    };
    let registries = documents
        .iter()
        .filter(|doc| doc.root.name.eq_ignore_ascii_case("hotkeys"))
        .map(|doc| (doc.path.key(), doc))
        .collect::<BTreeMap<_, _>>();
    let modes = super::super::authored_hotkey_file_references::expand_authored_hotkey_files(
        modes,
        &registries,
    )
    .map_err(|gap| invalid_source_data(gap.message))?;
    let Some(game) =
        find_nested_ordered_source_document_node_by_name(&modes.root.children, "ZTGameMode")
    else {
        return Ok(Vec::new());
    };
    let Some(parent) = source_element_child(game, "children")
        .and_then(|children| source_element_child(children, "ZTFirstPersonMode"))
    else {
        return Ok(Vec::new());
    };
    let binding_key = |key: &SourceUiHotkey| -> io::Result<_> {
        let ctrl = match key.control_state.as_deref().unwrap_or("0") {
            "-1" => UiDocumentHotkeyControlState::AnyControlState,
            "0" => UiDocumentHotkeyControlState::ControlReleased,
            "1" => UiDocumentHotkeyControlState::ControlPressed,
            _ => {
                return Err(invalid_source_data(
                    "invalid authored first-person hotkey control state",
                ))
            }
        };
        let (code, ctrl) = if key.trigger == SourceUiHotkeyTrigger::Character {
            character_code_virtual_key(key.code, ctrl)
        } else {
            (key.code.unwrap_or(0), ctrl)
        };
        let trigger = match key.trigger {
            SourceUiHotkeyTrigger::Down | SourceUiHotkeyTrigger::Character => {
                UiDocumentHotkeyTrigger::KeyPressed
            }
            SourceUiHotkeyTrigger::Up => UiDocumentHotkeyTrigger::KeyReleased,
            SourceUiHotkeyTrigger::Unknown(_) => {
                return Err(invalid_source_data(
                    "invalid authored first-person hotkey edge",
                ))
            }
        };
        let character = key
            .character
            .as_deref()
            .unwrap_or_default()
            .to_ascii_uppercase();
        Ok((trigger, code, character, ctrl))
    };
    let keyed_hotkeys = |mode| {
        parse_mode_hotkeys(&modes, mode)
            .into_iter()
            .map(|key| Ok((binding_key(&key)?, key)))
            .collect::<io::Result<Vec<_>>>()
    };
    let game_hotkeys = keyed_hotkeys(game)?;
    let base = keyed_hotkeys(parent)?;
    let children = source_element_child(parent, "children");
    let mut profiles = Vec::new();
    for mode in std::iter::once(parent).chain(
        children
            .into_iter()
            .flat_map(|container| container.element_children()),
    ) {
        let name = required_source_attribute(mode, "name")?;
        let local = if std::ptr::eq(mode, parent) {
            Vec::new()
        } else {
            keyed_hotkeys(mode)?
        };
        let inherited_base = base
            .iter()
            .filter(|(key, _)| !local.iter().any(|(local, _)| local == key));
        let inherited_game = game_hotkeys
            .iter()
            .filter(|(key, _)| !local.iter().chain(&base).any(|(nearer, _)| nearer == key));
        let bindings = local
            .iter()
            .chain(inherited_base)
            .chain(inherited_game)
            .filter_map(|((trigger, key_code, character, control_state), hotkey)| {
                let actions = hotkey
                    .events
                    .iter()
                    .filter_map(lower_first_person_command)
                    .map(UiActionRecord::Camera)
                    .collect::<Vec<_>>();
                (!actions.is_empty()).then(|| UiDocumentHotkeyDefinition {
                    trigger: *trigger,
                    key_code: *key_code,
                    character: character.clone(),
                    control_state: *control_state,
                    allow_repeat: hotkey.allow_repeat.unwrap_or(false),
                    actions,
                })
            })
            .collect();
        profiles.push(FirstPersonModeHotkeys {
            mode: AssetId::from_key(&name.to_ascii_lowercase()),
            bindings,
        });
    }
    Ok(profiles)
}

pub(super) fn parse_authored_interaction_cursors_from_gameplay_mode_document(
    document: &OrderedSourceDocument,
) -> Result<AuthoredUiInteractionCursors, UiSourceDocumentGap> {
    let required = |tag: &str, attribute: &str| {
        find_nested_ordered_source_document_node_by_name(&document.root.children, tag)
            .and_then(|node| node.attribute(attribute))
            .map(str::to_owned)
            .ok_or_else(|| UiSourceDocumentGap {
                family: UiSourceDocumentFamily::Ui,
                kind: UiSourceDocumentGapKind::UnsupportedVocabulary,
                virtual_path: document.path.key(),
                span: document.root.span,
                message: format!("{tag} has no authored {attribute} cursor"),
            })
    };
    Ok(AuthoredUiInteractionCursors {
        overhead: required("ZTOverheadMode", "cursor")?,
        placement_default: required("ZTPlacementMode", "defaultCursor")?,
        placement_pickup: required("ZTPlacementMode", "pickupCursor")?,
        placement_rotate: required("ZTPlacementMode", "rotateCursor")?,
        fence_default: required("ZTFenceMode", "cursor")?,
        fence_gate: required("ZTFenceMode", "gateCursor")?,
        path: required("ZTPathMode", "cursor")?,
        elevated_path: required("ZTElevatedPathMode", "cursor")?,
        biome_default: required("ZTBiomeMode", "cursor")?,
        biome_paint: required("ZTBiomeMode", "paintCursor")?,
        biome_invalid: required("ZTBiomeMode", "invalidCursor")?,
        terrain: required("ZTTerrainCommandReader", "cursor")?,
        tank: required("ZTTankCommandReader", "cursor")?,
        selection_default: required("ZTSelectionMode", "defaultCursor")?,
        selection_pickup: required("ZTSelectionMode", "pickupCursor")?,
        selection_rotate: required("ZTSelectionMode", "rotateCursor")?,
        delete: required("ZTDeleteMode", "cursor")?,
    })
}

pub(super) fn parse_authored_placement_preview_from_gameplay_mode_document(
    document: &OrderedSourceDocument,
) -> Result<AuthoredUiPlacementPreview, UiSourceDocumentGap> {
    fn direct_child<'a>(
        parent: &'a OrderedSourceDocumentNode,
        name: &str,
    ) -> Option<&'a OrderedSourceDocumentNode> {
        parent
            .children
            .iter()
            .find(|node| node.name.eq_ignore_ascii_case(name))
    }
    let gap = |message: String| UiSourceDocumentGap {
        family: UiSourceDocumentFamily::Ui,
        kind: UiSourceDocumentGapKind::UnsupportedVocabulary,
        virtual_path: document.path.key(),
        span: document.root.span,
        message,
    };
    let placement = find_nested_ordered_source_document_node_by_name(
        &document.root.children,
        "ZTPlacementMode",
    )
    .ok_or_else(|| gap("gameplay mode document has no ZTPlacementMode".into()))?;
    let fence =
        find_nested_ordered_source_document_node_by_name(&document.root.children, "ZTFenceMode")
            .ok_or_else(|| gap("gameplay mode document has no ZTFenceMode".into()))?;
    let required = |node: &OrderedSourceDocumentNode, attribute: &str| {
        node.attribute(attribute)
            .map(str::to_owned)
            .ok_or_else(|| gap(format!("{} has no authored {attribute}", node.name)))
    };
    let color =
        |owner: &OrderedSourceDocumentNode, name: &str| -> Result<[u8; 4], UiSourceDocumentGap> {
            let colors = direct_child(owner, "colors")
                .ok_or_else(|| gap(format!("{} has no authored colors child", owner.name)))?;
            let node = direct_child(colors, name)
                .ok_or_else(|| gap(format!("{} has no authored {name} child", colors.name)))?;
            let channel = |attribute: &str, fallback: u8| {
                node.attribute(attribute).map_or(Ok(fallback), |value| {
                    value.parse::<u8>().map_err(|_| {
                        gap(format!("{name} has invalid {attribute} channel {value:?}"))
                    })
                })
            };
            Ok([
                channel("r", 0)?,
                channel("g", 0)?,
                channel("b", 0)?,
                channel("a", 255)?,
            ])
        };
    let grid = direct_child(placement, "grid")
        .ok_or_else(|| gap(format!("{} has no authored grid child", placement.name)))?;
    let grid_entry = |name: &str| -> Result<(String, String, f32), UiSourceDocumentGap> {
        let node = direct_child(grid, name)
            .ok_or_else(|| gap(format!("{} has no authored {name} child", grid.name)))?;
        let radius = required(node, "radius")?
            .trim_end_matches(['f', 'F'])
            .parse::<f32>()
            .map_err(|_| gap(format!("{name} has an invalid authored radius")))?;
        if !radius.is_finite() || radius <= 0.0 {
            return Err(gap(format!(
                "{name} has an authored radius which is not finite and positive"
            )));
        }
        Ok((
            required(node, "cardinalImage")?,
            required(node, "diagonalImage")?,
            radius,
        ))
    };
    let large = grid_entry("large")?;
    let medium = grid_entry("medium")?;
    let small = grid_entry("small")?;
    Ok(AuthoredUiPlacementPreview {
        footprint_valid: required(placement, "footprintplace_yes")?,
        footprint_valid_small: required(placement, "footprintplace_yes_small")?,
        footprint_invalid: required(placement, "footprintplace_no")?,
        footprint_invalid_small: required(placement, "footprintplace_no_small")?,
        grid_large_cardinal: large.0,
        grid_large_diagonal: large.1,
        grid_large_radius: large.2,
        grid_medium_cardinal: medium.0,
        grid_medium_diagonal: medium.1,
        grid_medium_radius: medium.2,
        grid_small_cardinal: small.0,
        grid_small_diagonal: small.1,
        grid_small_radius: small.2,
        model_valid_srgba: color(placement, "place_yes")?,
        model_invalid_srgba: color(placement, "place_no")?,
        fence_valid_srgba: color(fence, "place_yes")?,
        fence_invalid_srgba: color(fence, "place_no")?,
        footprint_valid_srgba: color(placement, "footprintplace_yes")?,
        footprint_invalid_srgba: color(placement, "footprintplace_no")?,
        grid_srgba: color(placement, "grid")?,
    })
}

use bevy::{asset::LoadContext, prelude::*};
use openzt2_game_data::ui_document::document::{UiDocument, UiDocumentRole};
use std::io;

use crate::{
    source_coordinate_conversion::{
        conjugate_source_z_up_rotation_into_bevy_y_up_basis,
        convert_row_major_rotation_matrix_to_quaternion,
        convert_source_z_up_vector_to_bevy_y_up_coordinates,
    },
    source_document::{
        blue_fang_source_document_parsing::parse_blue_fang_source_document, path::AssetPath,
    },
    ui_document::ui_document_asset_types_and_borrowing_queries::{
        FameStarDisplay, FameStarEffect, FameStarModel,
    },
};

use super::super::rail_camera::source_value_reading::{
    invalid_source_data, optional_source_boolean_attribute, parse_finite_source_number,
    read_source_rotation_matrix, read_source_vector_attributes, required_finite_source_number,
    required_source_attribute, source_element_child,
};

pub(in crate::ui_document::source) async fn load_authored_fame_star_display(
    source_documents: &[OrderedSourceDocument],
    documents: &[UiDocument],
    archives: &crate::asset_source::AssetArchives,
    context: &mut LoadContext<'_>,
) -> io::Result<Option<FameStarDisplay>> {
    let Some(hud) = documents
        .iter()
        .find(|document| document.role == UiDocumentRole::InGameHud)
    else {
        return Ok(None);
    };
    let Some(mode) = source_documents
        .iter()
        .find(|document| document.path.key() == "ui/modes/modes.xml")
    else {
        return Ok(None);
    };
    let Some(display) =
        find_nested_ordered_source_document_node_by_name(&mode.root.children, "ZTStarsDisplay")
    else {
        return Ok(None);
    };
    let (index, region) = hud
        .nodes
        .iter()
        .enumerate()
        .find(|(_, node)| node.name.eq_ignore_ascii_case("zoo fame stars"))
        .ok_or_else(|| invalid_source_data("fame star display has no HUD region"))?;
    let camera = load_fame_star_xml(context, "ui/fx/cameraobject.xml").await?;
    let attachments =
        find_nested_ordered_source_document_node_by_name(&camera.root.children, "extraNodes")
            .ok_or_else(|| invalid_source_data("fame star camera has no attachment nodes"))?;
    let mut wells = Vec::new();
    for node in attachments.element_children() {
        let Some(index) = node
            .attribute("name")
            .and_then(|name| name.strip_prefix("star"))
            .and_then(|index| index.parse::<usize>().ok())
        else {
            continue;
        };
        wells.push((index, fame_star_anchor_transform(attachments, node)?));
    }
    wells.sort_by_key(|(index, _)| *index);
    let Some((_, left)) = wells.first() else {
        return Err(invalid_source_data("fame star camera has no star anchors"));
    };
    let right = &wells.last().expect("star anchors are present").1;
    let step =
        left.translation.distance(right.translation) / wells.len().saturating_sub(1).max(1) as f32;
    let width = step * wells.len() as f32;
    let height = width * region.rect[3] / region.rect[2];
    if !height.is_finite() || width <= 0.0 || height <= 0.0 {
        return Err(invalid_source_data("fame star view has invalid dimensions"));
    }
    let center = (left.translation + right.translation) * 0.5;
    let camera_position = center - Vec3::X * center.x;
    let dictionary = load_fame_star_xml(context, "ui/fx/zoofamedict.xml").await?;
    let model_sources = FameStarModelSources {
        archives,
        mode,
        dictionary: &dictionary,
    };
    let full =
        load_fame_star_model(context, &model_sources, display, "star", Some("starfull")).await?;
    let half = load_fame_star_model(
        context,
        &model_sources,
        display,
        "halfstar",
        Some("starhalffull"),
    )
    .await?;
    let appear = load_fame_star_model(
        context,
        &model_sources,
        display,
        "appear",
        display.attribute("appear"),
    )
    .await?;
    let disappear = load_fame_star_model(
        context,
        &model_sources,
        display,
        "disappear",
        display.attribute("disappear"),
    )
    .await?;
    Ok(Some(FameStarDisplay {
        wells_node_index: index as u32,
        well_transforms: wells.into_iter().map(|(_, transform)| transform).collect(),
        viewport_transform: Transform::from_translation(camera_position)
            .looking_at(center, Vec3::NEG_Z),
        viewport_size_m: Vec2::new(width, height),
        animation_delay_seconds: required_finite_source_number(display, "animationDelay")?,
        full,
        half,
        appear,
        disappear,
    }))
}

struct FameStarModelSources<'a> {
    archives: &'a crate::asset_source::AssetArchives,
    mode: &'a OrderedSourceDocument,
    dictionary: &'a OrderedSourceDocument,
}

fn fame_star_anchor_transform<'a>(
    attachments: &'a OrderedSourceDocumentNode,
    mut node: &'a OrderedSourceDocumentNode,
) -> io::Result<Transform> {
    let mut transform = Transform::IDENTITY;
    for _ in attachments.element_children() {
        let translation = source_element_child(node, "position")
            .map(read_source_vector_attributes)
            .transpose()?
            .map(convert_source_z_up_vector_to_bevy_y_up_coordinates)
            .map(Vec3::from_array)
            .unwrap_or_default();
        let rotation = source_element_child(node, "rotation")
            .map(read_source_rotation_matrix)
            .transpose()?
            .map(|matrix| {
                convert_row_major_rotation_matrix_to_quaternion(
                    conjugate_source_z_up_rotation_into_bevy_y_up_basis(matrix),
                )
            })
            .unwrap_or(Quat::IDENTITY);
        transform = Transform::from_translation(translation)
            .with_rotation(rotation)
            .mul_transform(transform);
        let Some(parent) = node.attribute("parent").filter(|parent| !parent.is_empty()) else {
            return Ok(transform);
        };
        node = attachments
            .element_children()
            .find(|candidate| candidate.attribute("name") == Some(parent))
            .ok_or_else(|| {
                invalid_source_data(format!("unknown fame star attachment parent {parent:?}"))
            })?;
    }
    Err(invalid_source_data(
        "fame star attachment hierarchy contains a cycle",
    ))
}

async fn load_fame_star_model(
    context: &mut LoadContext<'_>,
    sources: &FameStarModelSources<'_>,
    display: &OrderedSourceDocumentNode,
    slot: &str,
    cue: Option<&str>,
) -> io::Result<FameStarModel> {
    let source = source_element_child(display, slot)
        .and_then(|variant| source_element_child(variant, "BFPhysObj"))
        .ok_or_else(|| {
            invalid_source_data(format!("fame star slot {slot} has no physical object"))
        })?;
    let scene = source_element_child(source, "BFSceneGraphComponent").ok_or_else(|| {
        invalid_source_data(format!("fame star slot {slot} has no scene component"))
    })?;
    let controller = source_element_child(source, "BFAnimatedObjectControllerComponent")
        .ok_or_else(|| {
            invalid_source_data(format!("fame star slot {slot} has no animation controller"))
        })?;
    use crate::model_source::native_model_source_lowering::{
        lower_native_physical_scene_document, native_model_scene_labelled_asset_path,
    };
    use openzt2_game_data::scene_prefab::ScenePrefabAssetDependencyKind;
    use std::path::Path;
    let selected_model = |from: &OrderedSourceDocument, scene: &OrderedSourceDocumentNode| {
        let reference = required_source_attribute(scene, "modelfile")?;
        sources
            .archives
            .resolve_model_reference(Path::new(from.path.as_str()), reference)
            .ok_or_else(|| invalid_source_data(format!("unresolved fame-star model {reference:?}")))
    };
    let model_path = selected_model(sources.mode, scene)?;
    let model = context.load(native_model_scene_labelled_asset_path(
        &model_path.to_string_lossy(),
    ));
    let mut effects = Vec::new();
    if let Some(conditions) = cue
        .and_then(|cue| source_element_child(&sources.dictionary.root, cue))
        .and_then(|cue| source_element_child(cue, "Conditions"))
    {
        for entry in conditions
            .element_children()
            .filter(|node| node.name.eq_ignore_ascii_case("BFParticleDefinition"))
        {
            let xml_path = required_source_attribute(entry, "xmlFile")?;
            let xml_path = sources
                .archives
                .resolve_source_reference(Path::new(sources.dictionary.path.as_str()), xml_path)
                .ok_or_else(|| {
                    invalid_source_data(format!("unresolved fame-star cue {xml_path:?}"))
                })?;
            let effect = load_fame_star_xml(context, &xml_path.to_string_lossy()).await?;
            let scene = source_element_child(&effect.root, "BFSceneGraphComponent")
                .ok_or_else(|| invalid_source_data("fame star effect has no scene component"))?;
            let path = selected_model(&effect, scene)?;
            let bytes = context
                .read_asset_bytes(path.clone())
                .await
                .map_err(io::Error::other)?;
            let duration_seconds = entry
                .attribute("removeTime")
                .filter(|value| !value.trim().is_empty())
                .map(parse_finite_source_number)
                .transpose()?
                .unwrap_or(f32::INFINITY);
            let prefab = lower_native_physical_scene_document(&path.to_string_lossy(), &bytes)
                .map_err(io::Error::other)?;
            for dependency in prefab.dependencies.into_iter().filter(|dependency| {
                dependency.asset_kind == ScenePrefabAssetDependencyKind::Effect
            }) {
                effects.push(FameStarEffect {
                    effect: context.load(dependency.asset_path),
                    duration_seconds,
                });
            }
        }
    }
    Ok(FameStarModel {
        model,
        scale: required_finite_source_number(scene, "scale")?,
        duration_seconds: required_finite_source_number(controller, "duration")?,
        auto_start: optional_source_boolean_attribute(controller, "autoStart")?.unwrap_or(false),
        looping: optional_source_boolean_attribute(controller, "loop")?.unwrap_or(false),
        effects,
    })
}

async fn load_fame_star_xml(
    context: &mut LoadContext<'_>,
    path: &str,
) -> io::Result<OrderedSourceDocument> {
    let path = AssetPath::new(path);
    let bytes = context
        .read_asset_bytes(path.key())
        .await
        .map_err(io::Error::other)?;
    parse_blue_fang_source_document(path, &bytes)
        .map_err(|error| invalid_source_data(error.to_string()))
}
