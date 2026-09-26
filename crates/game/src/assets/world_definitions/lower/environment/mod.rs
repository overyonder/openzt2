use super::ambient_class_source_vocabulary::ambient_class;
use super::ambient_spawn_element_source_lowering::bind_ambient_element;
use super::environment_source_path_identity::{
    environment_family_from_path, environment_id_from_path,
};
use super::world_definition_lowering_tables::WorldDefinitionLoweringTables;
use super::world_definition_source_value_reading_and_conversion::{
    array, asset, asset_list, child_named, element_array, element_asset, element_bool,
    element_number, id, indexed_attribute, indexed_values, number_or, parse_element_scalar,
    parse_f32_triplet, parse_rgb_u16, required_element, required_element_number, required_number,
    simple_error,
};
use crate::assets::source_coordinate_conversion::convert_source_z_up_vector_to_bevy_y_up_coordinates;
use crate::assets::source_document::blue_fang_source_numeric_lexeme::parse_blue_fang_source_numeric_lexeme;
use crate::assets::source_document::ordered_source_document_types::{
    OrderedSourceDocument as DataDocument, OrderedSourceDocumentNode,
};
use crate::assets::source_document::resolved_source_record_index::{BindError, RecordView};
use crate::assets::source_document::source_document_semantic_name::{
    canonicalize_source_document_record_key, source_document_names_are_semantically_equal,
};
use openzt2_game_data::world_definitions::biomes_locations_and_details::WorldLocationDefinition;
use openzt2_game_data::world_definitions::environment::{
    AmbientSpawnDefinition, EnvironmentFogKeyframe, EnvironmentFogQuality, EnvironmentFogSample,
    EnvironmentLightKeyframe, EnvironmentLightKind, EnvironmentLightSample, EnvironmentLightTarget,
    EnvironmentMapSample, EnvironmentSkyKeyframe, EnvironmentVisualFlags, EnvironmentVisualKind,
    EnvironmentVisualSample, WeatherDefinition, WeatherTransitionRule,
};
use openzt2_game_data::AssetId;

pub(super) fn bind_location_entries_document(
    document: &DataDocument,
    output: &mut WorldDefinitionLoweringTables,
) -> Result<(), BindError> {
    let root = &document.root;
    if !source_document_names_are_semantically_equal(root.name.as_str(), "entries") {
        return Err(BindError::at(
            document,
            root,
            "authored initial-location document root is not entries",
        ));
    }
    for location in root.element_children().filter(|child| {
        source_document_names_are_semantically_equal(child.name.as_str(), "ZTLocationEntry")
    }) {
        let location_name = location.attribute_named_any(&["name"]).ok_or_else(|| {
            BindError::at(document, location, "authored location entry has no name")
        })?;
        output.document.locations.push(WorldLocationDefinition {
            id: id(location_name),
            name_key: location
                .attribute_named_any(&["locid", "nameKey"])
                .map(id)
                .unwrap_or_default(),
            icon: location
                .attribute_named_any(&["icon"])
                .map(id)
                .unwrap_or_default(),
        });
    }
    Ok(())
}

pub(super) fn bind_location(
    record: &RecordView<'_, '_>,
    output: &mut WorldDefinitionLoweringTables,
) -> Result<(), BindError> {
    output.document.locations.push(WorldLocationDefinition {
        id: id(record.key),
        name_key: asset(record, &["locid", "nameKey"]),
        icon: record.value(&["icon"]).map(id).unwrap_or_default(),
    });
    Ok(())
}

pub(super) fn bind_environment_fog(
    record: &RecordView<'_, '_>,
    output: &mut WorldDefinitionLoweringTables,
) -> Result<(), BindError> {
    let environment =
        output.environment_definition(environment_id_from_path(&record.source_path()));
    for (name, quality) in [
        ("low", EnvironmentFogQuality::Low),
        ("medium", EnvironmentFogQuality::Medium),
        ("high", EnvironmentFogQuality::High),
    ] {
        for group in record.children_named(&[name]) {
            for frame in group.element_children().filter(|child| {
                canonicalize_source_document_record_key(child.name.as_str()) == "fogkeyframeargs"
            }) {
                environment.fog_samples.push(EnvironmentFogSample {
                    quality,
                    day_fraction: required_element_number(&frame, &["time"])?,
                    start_factor: required_element_number(&frame, &["start"])?,
                    end_factor: required_element_number(&frame, &["end"])?,
                    color_unorm: parse_rgb_u16(required_element(&frame, &["color"])?)?,
                });
            }
        }
    }
    Ok(())
}

pub(super) fn bind_environment_lights(
    record: &RecordView<'_, '_>,
    output: &mut WorldDefinitionLoweringTables,
) -> Result<(), BindError> {
    let environment =
        output.environment_definition(environment_id_from_path(&record.source_path()));
    let legacy_times = record
        .children_named(&["times"])
        .first()
        .copied()
        .map(|times| {
            times
                .attributes()
                .filter_map(|(name, value)| {
                    indexed_attribute(name, 't').map(|index| (index, value))
                })
                .map(|(index, value)| {
                    parse_blue_fang_source_numeric_lexeme::<f32>(value)
                        .map(|time| (index, time))
                        .ok_or_else(|| simple_error(format!("invalid legacy light time {value}")))
                })
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?
        .unwrap_or_default();
    for (node_name, target) in [
        ("terrLights", EnvironmentLightTarget::Terrain),
        ("objLights", EnvironmentLightTarget::Object),
    ] {
        for lights in record.children_named(&[node_name]) {
            for (name, value) in lights.attributes() {
                let Some(index) = indexed_attribute(name, 'l') else {
                    return Err(BindError::record(
                        record,
                        format!("unsupported legacy light attribute {name}"),
                    ));
                };
                let time = legacy_times
                    .iter()
                    .find_map(|(candidate, time)| (*candidate == index).then_some(*time))
                    .ok_or_else(|| {
                        BindError::record(
                            record,
                            format!("legacy light {name} has no matching time"),
                        )
                    })?;
                environment.light_samples.push(EnvironmentLightSample {
                    target,
                    kind: EnvironmentLightKind::Ambient,
                    quality: 0,
                    key: id(name),
                    model: id(value),
                    day_fraction: time,
                    ambient: [0.0; 3],
                    diffuse: [0.0; 3],
                    specular: [0.0; 3],
                    direction: [0.0; 3],
                    intensity: 0.0,
                    shadow: 0.0,
                });
            }
        }
    }
    if !legacy_times.is_empty() {
        for water in record.children_named(&["waterLights"]) {
            for quality in water.element_children() {
                let quality_code =
                    match canonicalize_source_document_record_key(quality.name.as_str()).as_str() {
                        "low" => 1,
                        "med" | "medium" => 2,
                        "high" => 3,
                        other => {
                            return Err(BindError::record(
                                record,
                                format!("unsupported water-light quality {other}"),
                            ));
                        }
                    };
                for (name, value) in quality.attributes() {
                    let Some(index) = indexed_attribute(name, 'l') else {
                        return Err(BindError::record(
                            record,
                            format!("unsupported legacy water-light attribute {name}"),
                        ));
                    };
                    let time = legacy_times
                        .iter()
                        .find_map(|(candidate, time)| (*candidate == index).then_some(*time))
                        .ok_or_else(|| {
                            BindError::record(
                                record,
                                format!("legacy water light {name} has no matching time"),
                            )
                        })?;
                    environment.light_samples.push(EnvironmentLightSample {
                        target: EnvironmentLightTarget::Water,
                        kind: EnvironmentLightKind::Ambient,
                        quality: quality_code,
                        key: id(name),
                        model: id(value),
                        day_fraction: time,
                        ambient: [0.0; 3],
                        diffuse: [0.0; 3],
                        specular: [0.0; 3],
                        direction: [0.0; 3],
                        intensity: 0.0,
                        shadow: 0.0,
                    });
                }
            }
        }
    }
    for (target_name, target) in [
        ("terrainLights", EnvironmentLightTarget::Terrain),
        ("objectLights", EnvironmentLightTarget::Object),
        ("waterLights", EnvironmentLightTarget::Water),
        ("cloudLights", EnvironmentLightTarget::Cloud),
    ] {
        for target_node in record.children_named(&[target_name]) {
            let mut times = Vec::new();
            if let Some(tods) = target_node.element_children().find(|child| {
                canonicalize_source_document_record_key(child.name.as_str()) == "tods"
            }) {
                for sample in tods.element_children() {
                    let time = required_element_number(&sample, &["t"])?;
                    let ambient = parse_f32_triplet(required_element(&sample, &["ambient"])?)?;
                    let shadow = required_element_number(&sample, &["shadow"])?;
                    times.push((
                        canonicalize_source_document_record_key(sample.name.as_str()),
                        time,
                        ambient,
                        shadow,
                    ));
                    environment.light_samples.push(EnvironmentLightSample {
                        target,
                        kind: EnvironmentLightKind::Ambient,
                        quality: 0,
                        key: id(sample.name.as_str()),
                        model: AssetId::default(),
                        day_fraction: time,
                        ambient,
                        diffuse: [0.0; 3],
                        specular: [0.0; 3],
                        direction: [0.0; 3],
                        intensity: 0.0,
                        shadow,
                    });
                }
            }
            if let Some(lights) = target_node.element_children().find(|child| {
                canonicalize_source_document_record_key(child.name.as_str()) == "lights"
            }) {
                for light in lights.element_children() {
                    let kind = match canonicalize_source_document_record_key(light.name.as_str())
                        .as_str()
                    {
                        "sunlight" => EnvironmentLightKind::Sun,
                        "sidelight" => EnvironmentLightKind::Side,
                        "backlight" => EnvironmentLightKind::Back,
                        value => {
                            return Err(simple_error(format!(
                                "unknown environment light kind {value}"
                            )));
                        }
                    };
                    for sample in light.element_children() {
                        let key = canonicalize_source_document_record_key(sample.name.as_str());
                        let (day_fraction, _, shadow) = times
                            .iter()
                            .find(|(name, _, _, _)| *name == key)
                            .map(|(_, time, ambient, shadow)| (*time, *ambient, *shadow))
                            .ok_or_else(|| {
                                simple_error(format!(
                                    "light sample {} has no matching time-of-day sample",
                                    sample.name.as_str()
                                ))
                            })?;
                        environment.light_samples.push(EnvironmentLightSample {
                            target,
                            kind,
                            quality: 0,
                            key: id(sample.name.as_str()),
                            model: AssetId::default(),
                            day_fraction,
                            ambient: [0.0; 3],
                            diffuse: parse_f32_triplet(required_element(&sample, &["diffuse"])?)?,
                            specular: parse_f32_triplet(required_element(&sample, &["specular"])?)?,
                            direction: convert_source_z_up_vector_to_bevy_y_up_coordinates(
                                parse_f32_triplet(required_element(&sample, &["direction"])?)?,
                            ),
                            intensity: required_element_number(&sample, &["intensity"])?,
                            shadow,
                        });
                    }
                }
            }
        }
    }

    // Some authored defaults make the root `<lights>` element the target
    // container. In particular `world/water/defaultLight.xml` contains the
    // water ambient time-of-day curve directly, rather than wrapping it in a
    // redundant `<waterLights>` node. The directory is the authored target
    // discriminator used by the original content layout.
    if let Some(tods) = record
        .source_document_element()
        .element_children()
        .find(|child| canonicalize_source_document_record_key(child.name.as_str()) == "tods")
    {
        let source_path = record.source_path();
        let target = source_path
            .split('/')
            .rev()
            .skip(1)
            .find_map(|component| {
                match canonicalize_source_document_record_key(component).as_str() {
                    "terrain" => Some(EnvironmentLightTarget::Terrain),
                    "object" | "objects" => Some(EnvironmentLightTarget::Object),
                    "water" => Some(EnvironmentLightTarget::Water),
                    "cloud" | "clouds" => Some(EnvironmentLightTarget::Cloud),
                    _ => None,
                }
            })
            .ok_or_else(|| {
                BindError::record(
                    record,
                    "direct light samples have no terrain/object/water/cloud path owner",
                )
            })?;
        for sample in tods.element_children() {
            environment.light_samples.push(EnvironmentLightSample {
                target,
                kind: EnvironmentLightKind::Ambient,
                quality: 0,
                key: id(sample.name.as_str()),
                model: AssetId::default(),
                day_fraction: required_element_number(&sample, &["t"])?,
                ambient: parse_f32_triplet(required_element(&sample, &["ambient"])?)?,
                diffuse: [0.0; 3],
                specular: [0.0; 3],
                direction: [0.0; 3],
                intensity: 0.0,
                shadow: required_element_number(&sample, &["shadow"])?,
            });
        }
    }

    // Empty and reference-only light documents contribute no samples.
    Ok(())
}

#[inline]
pub(super) fn bind_environment(
    record: &RecordView<'_, '_>,
    output: &mut WorldDefinitionLoweringTables,
) -> Result<(), BindError> {
    let environment = id(record.key);
    let mut visual_samples = Vec::new();
    let mut fog_samples = Vec::new();
    let mut map_samples = Vec::new();
    // Expansion-era environment documents reference their fog programme as a
    // separate source document.  That document is bound by
    // `bind_environment_fog`; the `<fog file="..."/>` node is a link, not an
    // empty inline fog programme. Preserve the referenced programme's family
    // on the environment definition so the catalogue retains the authored
    // relationship without carrying source paths into the runtime.
    let external_fog_family = record
        .children_named(&["fog"])
        .into_iter()
        .find_map(|fog| fog.attribute_named_any(&["file"]))
        .map(environment_id_from_path);
    for sky_layers in record.children_named(&["skylayers"]) {
        let sky_layers = if let Some(reference) = sky_layers.attribute_named_any(&["file"]) {
            record
                .find_resolved_source_record_by_reference(reference)
                .ok_or_else(|| {
                    BindError::record(
                        record,
                        format!("environment sky-layer document is missing: {reference}"),
                    )
                })?
                .source_document_element()
        } else {
            sky_layers
        };
        for (layer_index, layer) in sky_layers.element_children().enumerate() {
            let flags = [
                (
                    element_bool(&layer, &["clearZBufferAfter"], false)?,
                    EnvironmentVisualFlags::CLEAR_DEPTH,
                ),
                (
                    element_bool(&layer, &["useWorldLights"], false)?,
                    EnvironmentVisualFlags::USE_WORLD_LIGHTS,
                ),
                (
                    element_bool(&layer, &["smoothUpdate"], false)?,
                    EnvironmentVisualFlags::SMOOTH_UPDATE,
                ),
            ]
            .into_iter()
            .filter_map(|(enabled, flag)| enabled.then_some(flag))
            .fold(
                EnvironmentVisualFlags::EMPTY,
                EnvironmentVisualFlags::with_additional_flags,
            );
            match canonicalize_source_document_record_key(layer.name.as_str()).as_str() {
                "bfcolorinterpolateskylayer" => {
                    let models = child_named(layer, "interpModels").ok_or_else(|| {
                        BindError::record(record, "interpolated sky layer is missing models")
                    })?;
                    let times = child_named(layer, "interpTimes").ok_or_else(|| {
                        BindError::record(record, "interpolated sky layer is missing times")
                    })?;
                    for (index, model) in indexed_values(models, &['m']) {
                        let time = indexed_values(times, &['t'])
                            .find_map(|(candidate, value)| (candidate == index).then_some(value))
                            .ok_or_else(|| {
                                BindError::record(record, format!("sky model {index} has no time"))
                            })?;
                        let day_fraction = parse_element_scalar(time, "sky time")?;
                        visual_samples.push(EnvironmentVisualSample {
                            kind: EnvironmentVisualKind::Sky,
                            layer: layer_index as u16,
                            // Every interpolated model is a keyframe on
                            // one visual track, not a simultaneous layer.
                            order: 0,
                            day_fraction,
                            asset: id(model),
                            position_model: AssetId::default(),
                            node: AssetId::default(),
                            scale: 1.0,
                            color_unorm: [u16::MAX; 3],
                            flags,
                        });
                    }
                }
                "bfskylayer" => {
                    if let Some(models) = child_named(layer, "models") {
                        for (index, model) in indexed_values(models, &['m']) {
                            visual_samples.push(EnvironmentVisualSample {
                                kind: EnvironmentVisualKind::Sky,
                                layer: layer_index as u16,
                                order: index,
                                day_fraction: 0.0,
                                asset: id(model),
                                position_model: AssetId::default(),
                                node: AssetId::default(),
                                scale: 1.0,
                                color_unorm: [u16::MAX; 3],
                                flags,
                            });
                        }
                    }
                }
                "ztsunlayer" => {
                    let position_model = layer
                        .attribute_named_any(&["sunPositionModel"])
                        .ok_or_else(|| {
                            BindError::record(record, "sun layer is missing its position model")
                        })?;
                    let model = child_named(layer, "sunModels")
                        .and_then(|models| {
                            indexed_values(models, &['m'])
                                .next()
                                .map(|(_, value)| value)
                        })
                        .ok_or_else(|| {
                            BindError::record(record, "sun layer is missing its model")
                        })?;
                    let times = child_named(layer, "interpTimes")
                        .ok_or_else(|| BindError::record(record, "sun layer is missing times"))?;
                    let nodes = child_named(layer, "interpNodes")
                        .ok_or_else(|| BindError::record(record, "sun layer is missing nodes"))?;
                    let scales = child_named(layer, "interpScales")
                        .ok_or_else(|| BindError::record(record, "sun layer is missing scales"))?;
                    let colors = child_named(layer, "interpColors")
                        .ok_or_else(|| BindError::record(record, "sun layer is missing colors"))?;
                    for (index, time) in indexed_values(times, &['t']) {
                        let node = indexed_values(nodes, &['n'])
                            .find_map(|(candidate, value)| (candidate == index).then_some(value))
                            .ok_or_else(|| {
                                BindError::record(record, format!("sun time {index} has no node"))
                            })?;
                        let scale = indexed_values(scales, &['s'])
                            .find_map(|(candidate, value)| (candidate == index).then_some(value))
                            .ok_or_else(|| {
                                BindError::record(record, format!("sun time {index} has no scale"))
                            })?;
                        let color = indexed_values(colors, &['c'])
                            .find_map(|(candidate, value)| (candidate == index).then_some(value))
                            .ok_or_else(|| {
                                BindError::record(record, format!("sun time {index} has no color"))
                            })?;
                        visual_samples.push(EnvironmentVisualSample {
                            kind: EnvironmentVisualKind::Sun,
                            layer: layer_index as u16,
                            // The repeated sun rows are keyframes for one
                            // positioned body/beams track.
                            order: 0,
                            day_fraction: parse_element_scalar(time, "sun time")?,
                            asset: id(model),
                            position_model: id(position_model),
                            node: id(node),
                            scale: parse_element_scalar(scale, "sun scale")?,
                            color_unorm: parse_rgb_u16(color)?,
                            flags,
                        });
                    }
                }
                value => {
                    return Err(BindError::record(
                        record,
                        format!("unknown environment sky layer {value}"),
                    ));
                }
            }
        }
    }
    for skirts in record.children_named(&["skirtModels"]) {
        for (index, model) in indexed_values(skirts, &['s']) {
            visual_samples.push(EnvironmentVisualSample {
                kind: EnvironmentVisualKind::Skirt,
                layer: 0,
                order: index,
                day_fraction: 0.0,
                asset: id(model),
                position_model: AssetId::default(),
                node: AssetId::default(),
                scale: 1.0,
                color_unorm: [u16::MAX; 3],
                flags: EnvironmentVisualFlags::EMPTY,
            });
        }
    }
    for fog in record.children_named(&["fog"]) {
        if fog.attribute_named_any(&["file"]).is_some() {
            if fog.element_children().next().is_some() {
                return Err(BindError::record(
                    record,
                    "environment fog cannot be both an external reference and an inline programme",
                ));
            }
            continue;
        }
        let times = child_named(fog, "interpTimes")
            .ok_or_else(|| BindError::record(record, "inline environment fog is missing times"))?;
        let distances = child_named(fog, "interpDists").ok_or_else(|| {
            BindError::record(record, "inline environment fog is missing distances")
        })?;
        let colors = child_named(fog, "interpColors")
            .ok_or_else(|| BindError::record(record, "inline environment fog is missing colors"))?;
        for (index, time) in indexed_values(times, &['t']) {
            let distance = indexed_values(distances, &['d'])
                .find_map(|(candidate, value)| (candidate == index).then_some(value))
                .ok_or_else(|| {
                    BindError::record(record, format!("fog time {index} has no distance"))
                })?;
            let color = indexed_values(colors, &['c'])
                .find_map(|(candidate, value)| (candidate == index).then_some(value))
                .ok_or_else(|| {
                    BindError::record(record, format!("fog time {index} has no color"))
                })?;
            fog_samples.push(EnvironmentFogSample {
                quality: EnvironmentFogQuality::Medium,
                day_fraction: parse_element_scalar(time, "fog time")?,
                start_factor: 0.0,
                end_factor: parse_element_scalar(distance, "fog distance")?,
                color_unorm: parse_rgb_u16(color)?,
            });
        }
    }
    for maps in record.children_named(&["envmaps"]) {
        for sample in maps
            .element_children()
            .filter(|child| canonicalize_source_document_record_key(child.name.as_str()) == "map")
        {
            map_samples.push(EnvironmentMapSample {
                day_fraction: required_element_number::<f32>(&sample, &["time"])?,
                water_texture: element_asset(&sample, &["waterMap"]),
                object_texture: element_asset(&sample, &["objectMap"]),
            });
        }
    }
    let mut light_keyframes = Vec::new();
    for frame in record.children_named(&["lightKeyframe"]) {
        light_keyframes.push(EnvironmentLightKeyframe {
            day_fraction: required_element_number(&frame, &["dayFraction", "time"])?,
            direction_snorm: element_array(
                &frame,
                &["directionX", "directionY", "directionZ"],
                [0_i16, -32767, 0],
            )?,
            color_unorm: element_array(&frame, &["colorR", "colorG", "colorB"], [u16::MAX; 3])?,
            illuminance_lux: required_element_number(&frame, &["illuminanceLux", "illuminance"])?,
        });
    }
    let mut fog_keyframes = Vec::new();
    for frame in record.children_named(&["fogKeyframe"]) {
        fog_keyframes.push(EnvironmentFogKeyframe {
            day_fraction: required_element_number(&frame, &["dayFraction", "time"])?,
            color_unorm: element_array(&frame, &["colorR", "colorG", "colorB"], [0_u16; 3])?,
            start_cm: required_element_number(&frame, &["startCm", "start"])?,
            end_cm: required_element_number(&frame, &["endCm", "end"])?,
        });
    }
    let mut sky_keyframes = Vec::new();
    for frame in record.children_named(&["skyKeyframe"]) {
        sky_keyframes.push(EnvironmentSkyKeyframe {
            day_fraction: required_element_number(&frame, &["dayFraction", "time"])?,
            texture: element_asset(&frame, &["texture", "skyTexture"]),
            tint_unorm: element_array(
                &frame,
                &["tintR", "tintG", "tintB", "tintA"],
                [u16::MAX; 4],
            )?,
            rotation_snorm: element_number(&frame, &["rotationSnorm", "rotation"], 0)?,
        });
    }
    let weather_start = output.document.weather.len();
    for item in record.children_named(&["weatherDefinition"]) {
        output
            .document
            .weather
            .push(weather_from_element(environment, &item)?);
    }
    let mut transitions = Vec::new();
    for item in record.children_named(&["weatherTransition"]) {
        transitions.push(WeatherTransitionRule {
            from: element_asset(&item, &["from"]),
            to: element_asset(&item, &["to"]),
            probability: required_element_number(&item, &["probability"])?,
            transition_ticks: required_element_number(&item, &["transitionTicks"])?,
            day_fraction: element_array(
                &item,
                &["minimumDayFraction", "maximumDayFraction"],
                [0_u16, u16::MAX],
            )?,
        });
    }
    let ambient_start = output.document.ambient_spawns.len();
    for item in record.children_named(&["ambientSpawn"]) {
        bind_ambient_element(environment, &item, output)?;
    }
    let weather = output.document.weather[weather_start..]
        .iter()
        .map(|weather| weather.id)
        .collect();
    let ambient = output.document.ambient_spawns[ambient_start..]
        .iter()
        .map(|ambient| ambient.id)
        .collect();
    let family =
        external_fog_family.unwrap_or_else(|| environment_family_from_path(&record.source_path()));
    let initial_weather = asset(record, &["initialWeather", "weather"]);
    let wind_mps = array(record, &["minimumWindMps", "maximumWindMps"], [0_f32; 2])?;
    let definition = output.environment_definition(environment);
    definition.family = family;
    definition.light_keyframes = light_keyframes;
    definition.fog_keyframes = fog_keyframes;
    definition.sky_keyframes = sky_keyframes;
    definition.weather = weather;
    definition.transitions = transitions;
    definition.ambient = ambient;
    definition.fog_samples.extend(fog_samples);
    definition.map_samples.extend(map_samples);
    definition.visual_samples.extend(visual_samples);
    definition.initial_weather = initial_weather;
    definition.wind_mps = wind_mps;
    Ok(())
}

pub(super) fn bind_weather(
    record: &RecordView<'_, '_>,
    output: &mut WorldDefinitionLoweringTables,
) -> Result<(), BindError> {
    let effect = asset(record, &["effect", "particle"]);
    if effect != AssetId::default() {
        return Err(BindError::record(
            record,
            "weather effect reference does not identify both the effect asset and its emitter",
        ));
    }
    output.document.weather.push(WeatherDefinition {
        id: id(record.key),
        environment: asset(record, &["environment"]),
        duration_ticks: array(
            record,
            &["minimumDurationTicks", "maximumDurationTicks"],
            [0_u32; 2],
        )?,
        wind_mps: array(record, &["minimumWindMps", "maximumWindMps"], [0_f32; 2])?,
        light_multiplier: number_or(record, &["lightMultiplier"], 1000)?,
        fog_multiplier: number_or(record, &["fogMultiplier"], 1000)?,
        audio: asset(record, &["audio", "sound"]),
        effect_asset: AssetId::default(),
        effect_emitter: AssetId::default(),
        welfare_delta: number_or(record, &["welfareDelta"], 0)?,
    });
    Ok(())
}

pub(super) fn weather_from_element(
    environment: AssetId,
    element: &'_ OrderedSourceDocumentNode,
) -> Result<WeatherDefinition, BindError> {
    let effect = element_asset(element, &["effect", "particle"]);
    if effect != AssetId::default() {
        return Err(simple_error(
            "weather effect reference does not identify both the effect asset and its emitter",
        ));
    }
    Ok(WeatherDefinition {
        id: id(required_element(element, &["id", "name"])?),
        environment,
        duration_ticks: element_array(
            element,
            &["minimumDurationTicks", "maximumDurationTicks"],
            [0_u32; 2],
        )?,
        wind_mps: element_array(element, &["minimumWindMps", "maximumWindMps"], [0_f32; 2])?,
        light_multiplier: element_number(element, &["lightMultiplier"], 1000)?,
        fog_multiplier: element_number(element, &["fogMultiplier"], 1000)?,
        audio: element_asset(element, &["audio", "sound"]),
        effect_asset: AssetId::default(),
        effect_emitter: AssetId::default(),
        welfare_delta: element_number(element, &["welfareDelta"], 0)?,
    })
}

pub(super) fn bind_ambient(
    record: &RecordView<'_, '_>,
    output: &mut WorldDefinitionLoweringTables,
) -> Result<(), BindError> {
    let radius_cm = array(record, &["minimumRadiusCm", "maximumRadiusCm"], [0_u32; 2])?;
    if radius_cm != [0, 0] {
        return Err(BindError::record(
            record,
            "ambient spawn radius requires an origin",
        ));
    }
    let effect = asset(record, &["effect", "particle"]);
    if effect != AssetId::default() {
        return Err(BindError::record(
            record,
            "ambient effect reference does not identify both the effect asset and its emitter",
        ));
    }
    let prefabs = record
        .value(&["prefabs", "entities"])
        .map(asset_list)
        .unwrap_or_default();
    let biomes = record
        .value(&["biomes"])
        .map(asset_list)
        .unwrap_or_default();
    output.document.ambient_spawns.push(AmbientSpawnDefinition {
        id: id(record.key),
        environment: asset(record, &["environment"]),
        prefabs,
        biomes,
        class: ambient_class(record.value(&["class", "ambientClass"]).unwrap_or("ground"))?,
        day_fraction: array(
            record,
            &["minimumDayFraction", "maximumDayFraction"],
            [0_u16, u16::MAX],
        )?,
        population: array(
            record,
            &["minimumPopulation", "maximumPopulation"],
            [0_u16; 2],
        )?,
        spawn_interval_ticks: array(
            record,
            &["minimumSpawnIntervalTicks", "maximumSpawnIntervalTicks"],
            [0_u32; 2],
        )?,
        radius_cm,
        lifetime_ticks: array(
            record,
            &["minimumLifetimeTicks", "maximumLifetimeTicks"],
            [0_u32; 2],
        )?,
        probability: required_number(record, &["probability"])?,
        audio: asset(record, &["audio", "sound"]),
        effect_asset: AssetId::default(),
        effect_emitter: AssetId::default(),
    });
    Ok(())
}
