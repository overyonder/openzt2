//! Authored water configuration and its source-document references.

use std::{collections::BTreeSet, io};

use openzt2_game_data::terrain::{
    TerrainBiomeShorelineConfiguration, TerrainWaterBubbleDetail, TerrainWaterBubbleEmission,
    TerrainWaterCausticsConfiguration, TerrainWaterConfiguration, TerrainWaterCurrentConfiguration,
    TerrainWaterDistanceFadeConfiguration, TerrainWaterEditingConfiguration, TerrainWaterFogDetail,
    TerrainWaterFogDirtiness, TerrainWaterFogKeyframe, TerrainWaterScumConfiguration,
    TerrainWaterShoreConfiguration, TerrainWaterSideConfiguration, TerrainWaterSprayDetail,
    TerrainWaterSprayVariant, TerrainWaterSuspensionConfiguration, TerrainWaterSuspensionEmission,
};
use openzt2_game_data::{
    scene_prefab::PrefabEnvironmentKey, world_definitions::environment::EnvironmentFogQuality,
};

use crate::source_document::{
    ordered_source_document_types::{OrderedSourceDocument, OrderedSourceDocumentNode},
    path::AssetPath,
};

/// File attributes on water configuration blocks are external configuration dependencies.
pub(crate) fn referenced_configuration_paths(documents: &[OrderedSourceDocument]) -> Vec<String> {
    let mut paths = BTreeSet::new();
    for document in documents {
        if matches!(
            document.root.name.as_str(),
            "water" | "suspense" | "fog" | "light" | "lights"
        ) {
            collect_file_references(&document.root, &mut paths);
        }
    }
    paths.into_iter().collect()
}

fn collect_file_references(node: &OrderedSourceDocumentNode, paths: &mut BTreeSet<String>) {
    if let Some(path) = text(node, "file") {
        paths.insert(path);
    }
    for child in node.element_children() {
        collect_file_references(child, paths);
    }
}

pub(crate) fn lower_biome_shoreline(
    node: &OrderedSourceDocumentNode,
) -> io::Result<TerrainBiomeShorelineConfiguration> {
    Ok(TerrainBiomeShorelineConfiguration {
        submerged_texture: text(node, "waterSubmerged"),
        shore_texture: text(node, "shoreTexture"),
        ripple_system: text(node, "shoreRipple"),
        ripple_maximum_percent: Some(number(node, "shoreRippleMaxPercent")?.unwrap_or(100.0)),
        ripple_period: Some(number(node, "shoreRipplePeriod")?.unwrap_or(0.5)),
        ripple_maximum_percent_per_period: Some(
            number(node, "shoreRippleMaxPercentPerPeriod")?.unwrap_or(3.0),
        ),
    })
}

pub(crate) fn lower_water_configuration(
    root: &OrderedSourceDocumentNode,
    documents: &[OrderedSourceDocument],
) -> io::Result<TerrainWaterConfiguration> {
    let shore = child(root, "shore")
        .map(|node| -> io::Result<_> {
            Ok(TerrainWaterShoreConfiguration {
                tint_rgba255: colour(node, "tint")?,
                bottom_rate: Some(number(node, "botRate")?.unwrap_or(0.01)),
                bottom_size: Some(
                    number(node, "botSize")?
                        .filter(|size| *size > 0.0)
                        .unwrap_or(4.0),
                ),
                top_rate: Some(number(node, "topRate")?.unwrap_or(-0.06)),
                top_size: Some(
                    number(node, "topSize")?
                        .filter(|size| *size > 0.0)
                        .unwrap_or(5.0),
                ),
                ripple_top: Some(
                    text(node, "rippleTop").unwrap_or_else(|| "Materials/waterlayout".to_owned()),
                ),
                ripple_bottom: Some(
                    text(node, "rippleBot").unwrap_or_else(|| "Materials/waterlayout".to_owned()),
                ),
                ripple_mask: Some(
                    text(node, "rippleMask").unwrap_or_else(|| "Materials/watermask".to_owned()),
                ),
            })
        })
        .transpose()?;
    let scum = child(root, "scummap")
        .map(|node| -> io::Result<_> {
            Ok(TerrainWaterScumConfiguration {
                image: text(node, "image"),
                bottom_size: Some(vector(node, "botsize")?.unwrap_or([100.0, 100.0])),
                bottom_rate: Some(vector(node, "botrate")?.unwrap_or([0.1, 0.0])),
                top_size: Some(vector(node, "topsize")?.unwrap_or([100.0, 100.0])),
                top_rate: Some(vector(node, "toprate")?.unwrap_or([0.0, 0.1])),
            })
        })
        .transpose()?;
    let side = child(root, "side")
        .map(|node| -> io::Result<_> {
            Ok(TerrainWaterSideConfiguration {
                colour_rgba255: colour(node, "color")?,
                lit_colour_rgba255: colour(node, "lit")?,
            })
        })
        .transpose()?;
    let spray = child(root, "spray")
        .into_iter()
        .flat_map(|node| node.element_children())
        .filter(|node| node.name.eq_ignore_ascii_case("detail"))
        .enumerate()
        .map(|(index, node)| -> io::Result<_> {
            Ok(TerrainWaterSprayDetail {
                level: integer_or(node, "level", index.saturating_sub(1))?,
                variants: node
                    .element_children()
                    .filter(|node| node.name.eq_ignore_ascii_case("level"))
                    .enumerate()
                    .map(|(index, node)| -> io::Result<_> {
                        let fallback = default_spray_variant(index);
                        Ok(TerrainWaterSprayVariant {
                            system: node
                                .attribute("system")
                                .map(str::to_owned)
                                .unwrap_or(fallback.system),
                            maximum_strength: number(node, "maxstrength")?
                                .unwrap_or(fallback.maximum_strength),
                            above_water_sound: text(node, "aboveWaterSound"),
                            below_water_sound: text(node, "belowWaterSound"),
                        })
                    })
                    .collect::<io::Result<_>>()?,
            })
        })
        .collect::<io::Result<_>>()?;
    let bubbles = child(root, "bubbles")
        .into_iter()
        .flat_map(|node| node.element_children())
        .filter(|node| node.name.eq_ignore_ascii_case("detail"))
        .enumerate()
        .map(|(index, node)| -> io::Result<_> {
            Ok(TerrainWaterBubbleDetail {
                level: integer_or(node, "level", index)?,
                system: text(node, "system"),
                sound: text(node, "sound"),
                emitters: node
                    .element_children()
                    .filter(|node| node.name.eq_ignore_ascii_case("emit"))
                    .map(|node| -> io::Result<_> {
                        Ok(TerrainWaterBubbleEmission {
                            small_seconds: Some(number(node, "smallsecs")?.unwrap_or(8.0)),
                            big_seconds: Some(number(node, "bigsecs")?.unwrap_or(1.0)),
                        })
                    })
                    .collect::<io::Result<_>>()?,
            })
        })
        .collect::<io::Result<_>>()?;
    let suspension = configuration_child(root, "suspense", documents)?
        .map(|node| -> io::Result<_> {
            if node.element_children().next().is_none() {
                return Ok(default_suspension());
            }
            Ok(TerrainWaterSuspensionConfiguration {
                system: Some(
                    node.attribute("system")
                        .unwrap_or("default_suspense")
                        .to_owned(),
                ),
                emitters: node
                    .element_children()
                    .filter(|node| node.name.eq_ignore_ascii_case("emit"))
                    .map(|node| -> io::Result<_> {
                        Ok(TerrainWaterSuspensionEmission {
                            minimum_density: numbers(node, "minDensity")?
                                .filter(|values| !values.is_empty())
                                .unwrap_or_else(|| vec![0.0]),
                            maximum_density: numbers(node, "maxDensity")?
                                .filter(|values| !values.is_empty())
                                .unwrap_or_else(|| vec![0.5]),
                        })
                    })
                    .collect::<io::Result<_>>()?,
            })
        })
        .transpose()?
        .or_else(|| Some(default_suspension()));
    let current = child(root, "current")
        .map(|node| -> io::Result<_> {
            Ok(TerrainWaterCurrentConfiguration {
                wind_speed: Some(number(node, "windspeed")?.unwrap_or(10.0)),
                gustiness: Some(number(node, "gustiness")?.unwrap_or(0.5)),
            })
        })
        .transpose()?
        .or_else(|| {
            Some(TerrainWaterCurrentConfiguration {
                wind_speed: Some(15.0),
                gustiness: Some(0.2),
            })
        });
    let fog = configuration_child(root, "fog", documents)?
        .map(lower_fog)
        .transpose()?
        .unwrap_or_default();
    let light = configuration_child(root, "light", documents)?
        .map(lower_light)
        .transpose()?
        .unwrap_or_default();
    let directional_lights = configuration_child(root, "light", documents)?
        .map(|root| lower_directional_lights(root, &light))
        .transpose()?
        .unwrap_or_default();
    Ok(TerrainWaterConfiguration {
        shore,
        scum,
        side,
        spray,
        spray_fallback: default_spray_variants(),
        bubbles,
        bubbles_fallback: TerrainWaterBubbleDetail {
            level: 0,
            system: Some("default_bubbles".to_owned()),
            sound: None,
            emitters: vec![TerrainWaterBubbleEmission {
                small_seconds: Some(8.0),
                big_seconds: Some(1.0),
            }],
        },
        suspension,
        current,
        fog,
        light,
        directional_lights,
        caustics: lower_caustics(child(root, "caustics"))?,
        wake_systems: child(root, "wake")
            .and_then(|node| node.attribute("system"))
            .map(|value| {
                value
                    .split_whitespace()
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            })
            .filter(|systems| !systems.is_empty())
            .unwrap_or_else(|| vec!["pdecal".to_owned()]),
        wake_maximum_counts: child(root, "wake")
            .map(|node| numbers(node, "maxcount"))
            .transpose()?
            .flatten()
            .filter(|counts| !counts.is_empty())
            .unwrap_or_else(|| vec![4.0, 6.0, 8.0, 10.0]),
        high_wake_systems: child(root, "wake")
            .and_then(|node| node.attribute("system"))
            .map(|value| {
                value
                    .split_whitespace()
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            })
            .filter(|systems| !systems.is_empty())
            .unwrap_or_else(|| vec!["phighdecal".to_owned()]),
        high_wake_maximum_counts: child(root, "wake")
            .map(|node| numbers(node, "maxcount"))
            .transpose()?
            .flatten()
            .filter(|counts| !counts.is_empty())
            .unwrap_or_else(|| vec![4.0; 4]),
        high_wake_instance_count: 2,
    })
}

fn default_suspension() -> TerrainWaterSuspensionConfiguration {
    TerrainWaterSuspensionConfiguration {
        system: Some("default_suspense".to_owned()),
        emitters: vec![TerrainWaterSuspensionEmission {
            minimum_density: vec![0.0],
            maximum_density: vec![0.5],
        }],
    }
}

fn default_spray_variants() -> Vec<TerrainWaterSprayVariant> {
    (0..5).map(default_spray_variant).collect()
}

fn default_spray_variant(index: usize) -> TerrainWaterSprayVariant {
    let (size, maximum_strength) = [
        ("tiny", 0.25),
        ("small", 0.45),
        ("med", 1.0),
        ("large", 1.9),
        ("huge", 1.0e10),
    ][index.min(4)];
    TerrainWaterSprayVariant {
        system: format!("test_{size}"),
        maximum_strength,
        above_water_sound: Some(format!("splash_{size}")),
        below_water_sound: Some(format!("splash_{size}_uw")),
    }
}

fn lower_caustics(
    node: Option<&OrderedSourceDocumentNode>,
) -> io::Result<TerrainWaterCausticsConfiguration> {
    let numeric = |name, default| -> io::Result<f32> {
        Ok(node
            .map(|node| number(node, name))
            .transpose()?
            .flatten()
            .unwrap_or(default))
    };
    let map = |name| {
        node.and_then(|node| text(node, name))
            .unwrap_or_else(|| "Materials/CausticTest".to_owned())
    };
    let tint = |name, default| -> io::Result<[f32; 4]> {
        let Some(values) = node.map(|node| numbers(node, name)).transpose()?.flatten() else {
            return Ok(default);
        };
        if values.len() < 3 {
            return Ok(default);
        }
        Ok([
            values[0] / 255.0,
            values[1] / 255.0,
            values[2] / 255.0,
            values.get(3).copied().unwrap_or(default[3]) / 255.0,
        ])
    };
    Ok(TerrainWaterCausticsConfiguration {
        bottom_map: map("botMap"),
        top_map: map("topMap"),
        height_lookup: "Materials/lutangle".to_owned(),
        bottom_rate: [numeric("botRateX", 0.1)?, numeric("botRateY", 0.1)?],
        top_rate: [numeric("topRateX", -0.11)?, numeric("topRateY", 0.0)?],
        bottom_tile: numeric("botTile", 0.05)?,
        top_tile: numeric("topTile", 0.05)?,
        fade_start: numeric("fadeStart", 0.25)?,
        fade_distance: numeric("fadeDist", 2.0)?,
        tint_rgba: tint("tint", [1.0, 1.0, 0.8, 1.0])?,
        terrain_tint_rgba: tint("terraintint", [0.5, 0.5, 0.4, 1.0])?,
        bottom_rock_distance_range: [
            numeric("botRockDistMin", -0.02)?,
            numeric("botRockDistMax", 0.02)?,
        ],
        top_rock_distance_range: [
            numeric("topRockDistMin", -0.02)?,
            numeric("topRockDistMax", 0.02)?,
        ],
        bottom_rock_rate_range: [
            numeric("botRockRateMin", 1.0)?,
            numeric("botRockRateMax", 0.5)?,
        ],
        top_rock_rate_range: [
            numeric("topRockRateMin", 1.0)?,
            numeric("topRockRateMax", 0.5)?,
        ],
    })
}

pub(crate) fn lower_water_distance_fade(
    root: &OrderedSourceDocumentNode,
) -> io::Result<TerrainWaterDistanceFadeConfiguration> {
    let defaults = TerrainWaterDistanceFadeConfiguration::default();
    let Some(node) = child(root, "fade") else {
        return Ok(defaults);
    };
    Ok(TerrainWaterDistanceFadeConfiguration {
        time_seconds: number(node, "time")?.unwrap_or(defaults.time_seconds),
        minimum_distance: number(node, "mindistance")?.unwrap_or(defaults.minimum_distance),
        maximum_distance: number(node, "maxdistance")?.unwrap_or(defaults.maximum_distance),
        search: number(node, "search")?.unwrap_or(defaults.search),
        minimum_fresnel: number(node, "minfresnel")?.unwrap_or(defaults.minimum_fresnel),
        maximum_fresnel: number(node, "maxfresnel")?.unwrap_or(defaults.maximum_fresnel),
    })
}

pub(crate) fn lower_water_editing(
    root: &OrderedSourceDocumentNode,
) -> io::Result<TerrainWaterEditingConfiguration> {
    let defaults = TerrainWaterEditingConfiguration::default();
    Ok(TerrainWaterEditingConfiguration {
        height_snap_interval_metres: number(root, "zSnapInterval")?
            .unwrap_or(defaults.height_snap_interval_metres),
        height_snap_base_metres: number(root, "nodeStartHeight")?
            .unwrap_or(defaults.height_snap_base_metres),
        plane_merge_tolerance_metres: number(root, "waterJoinTolerance")?
            .unwrap_or(defaults.plane_merge_tolerance_metres),
    })
}

fn lower_fog(root: &OrderedSourceDocumentNode) -> io::Result<Vec<TerrainWaterFogDetail>> {
    root.element_children()
        .filter_map(|node| {
            let quality = match node.name.as_str() {
                "low" => EnvironmentFogQuality::Low,
                "medium" => EnvironmentFogQuality::Medium,
                "high" => EnvironmentFogQuality::High,
                _ => return None,
            };
            Some((node, quality))
        })
        .map(|(node, quality)| -> io::Result<_> {
            Ok(TerrainWaterFogDetail {
                quality,
                unlit: child(node, "unlit")
                    .map(lower_fog_dirtiness)
                    .transpose()?
                    .unwrap_or_default(),
                lit: child(node, "lit")
                    .map(lower_fog_dirtiness)
                    .transpose()?
                    .unwrap_or_default(),
                unscoped: lower_fog_dirtiness(node)?,
            })
        })
        .collect()
}

fn lower_fog_dirtiness(
    root: &OrderedSourceDocumentNode,
) -> io::Result<Vec<TerrainWaterFogDirtiness>> {
    root.element_children()
        .filter(|node| node.name.eq_ignore_ascii_case("dirtiness"))
        .map(|node| -> io::Result<_> {
            Ok(TerrainWaterFogDirtiness {
                level: required_number(node, "level")?,
                keyframes: node
                    .element_children()
                    .filter(|node| node.name.eq_ignore_ascii_case("FogKeyframeArgs"))
                    .map(|node| -> io::Result<_> {
                        Ok(TerrainWaterFogKeyframe {
                            time: required_number(node, "time")?,
                            start: required_number(node, "start")?,
                            end: required_number(node, "end")?,
                            colour_rgb255: colour(node, "color")?
                                .ok_or_else(|| invalid("fog keyframe has no color"))?,
                        })
                    })
                    .collect::<io::Result<_>>()?,
            })
        })
        .collect()
}

fn lower_light(root: &OrderedSourceDocumentNode) -> io::Result<Vec<PrefabEnvironmentKey>> {
    child(root, "tods")
        .into_iter()
        .flat_map(|node| node.element_children())
        .map(|node| -> io::Result<_> {
            Ok(PrefabEnvironmentKey {
                time: required_number(node, "t")?,
                ambient_srgb: vector(node, "ambient")?
                    .ok_or_else(|| invalid("water light has no ambient"))?,
                shadow_strength: required_number(node, "shadow")?,
            })
        })
        .collect()
}

fn lower_directional_lights(
    root: &OrderedSourceDocumentNode,
    times: &[PrefabEnvironmentKey],
) -> io::Result<Vec<openzt2_game_data::world_definitions::environment::EnvironmentLightSample>> {
    use openzt2_game_data::world_definitions::environment::{
        EnvironmentLightKind, EnvironmentLightSample, EnvironmentLightTarget,
    };
    let Some(lights) = child(root, "lights") else {
        return Ok(Vec::new());
    };
    let labels: Vec<_> = child(root, "tods")
        .into_iter()
        .flat_map(|node| node.element_children())
        .collect();
    let mut samples = Vec::new();
    for (index, light) in lights.element_children().enumerate() {
        let child_index =
            u16::try_from(index).map_err(|_| invalid("water light child count exceeds u16"))?;
        let kind = match light.name.to_ascii_lowercase().as_str() {
            "sunlight" => EnvironmentLightKind::Sun,
            "sidelight" => EnvironmentLightKind::Side,
            "backlight" => EnvironmentLightKind::Back,
            _ => return Err(invalid("unsupported water light child kind")),
        };
        for node in light.element_children() {
            let index = labels
                .iter()
                .position(|label| label.name.eq_ignore_ascii_case(&node.name))
                .ok_or_else(|| invalid("water light child has no matching time key"))?;
            let key = &times[index];
            let direction = vector(node, "direction")?.unwrap_or([0.0, 0.0, -1.0]);
            samples.push(EnvironmentLightSample {
                target: EnvironmentLightTarget::Water, kind, child_index, quality: 0,
                key: openzt2_game_data::AssetId::from_key(node.name.as_str()), model: Default::default(),
                day_fraction: key.time, ambient: [0.0; 3],
                diffuse: vector(node, "diffuse")?.unwrap_or([1.0; 3]),
                specular: vector(node, "specular")?.unwrap_or([1.0; 3]),
                direction: crate::source_coordinate_conversion::convert_source_z_up_vector_to_bevy_y_up_coordinates(direction),
                intensity: number(node, "intensity")?.unwrap_or(1.0), shadow: key.shadow_strength,
            });
        }
    }
    Ok(samples)
}

fn configuration_child<'a>(
    root: &'a OrderedSourceDocumentNode,
    name: &str,
    documents: &'a [OrderedSourceDocument],
) -> io::Result<Option<&'a OrderedSourceDocumentNode>> {
    let Some(mut node) = child(root, name) else {
        return Ok(None);
    };
    let mut visited = BTreeSet::new();
    while let Some(path) = node.attribute("file").filter(|path| !path.is_empty()) {
        let key = AssetPath::new(path).key();
        if !visited.insert(key.clone()) {
            return Err(invalid("cyclic water configuration file reference"));
        }
        node = &documents
            .iter()
            .find(|document| document.path.key() == key)
            .ok_or_else(|| invalid("water configuration file dependency is missing"))?
            .root;
    }
    Ok(Some(node))
}

fn child<'a>(
    node: &'a OrderedSourceDocumentNode,
    name: &str,
) -> Option<&'a OrderedSourceDocumentNode> {
    node.element_children()
        .find(|node| node.name.eq_ignore_ascii_case(name))
}

fn text(node: &OrderedSourceDocumentNode, name: &str) -> Option<String> {
    node.attribute(name)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn number(node: &OrderedSourceDocumentNode, name: &str) -> io::Result<Option<f32>> {
    node.attribute(name)
        .map(|value| {
            value
                .trim()
                .trim_end_matches(['f', 'F'])
                .parse::<f32>()
                .ok()
                .filter(|value| value.is_finite())
                .ok_or_else(|| invalid(&format!("{} has invalid {name}", node.name)))
        })
        .transpose()
}

fn required_number(node: &OrderedSourceDocumentNode, name: &str) -> io::Result<f32> {
    number(node, name)?.ok_or_else(|| invalid(&format!("{} has no {name}", node.name)))
}

fn integer_or(node: &OrderedSourceDocumentNode, name: &str, default: usize) -> io::Result<u32> {
    node.attribute(name)
        .map(|value| {
            value
                .parse()
                .map_err(|_| invalid("water detail has invalid level"))
        })
        .unwrap_or_else(|| {
            u32::try_from(default).map_err(|_| invalid("too many water detail levels"))
        })
}

fn numbers(node: &OrderedSourceDocumentNode, name: &str) -> io::Result<Option<Vec<f32>>> {
    node.attribute(name)
        .map(|value| {
            value
                .split_whitespace()
                .map(|value| {
                    value
                        .trim_end_matches(['f', 'F'])
                        .parse::<f32>()
                        .ok()
                        .filter(|value| value.is_finite())
                        .ok_or_else(|| invalid("invalid water configuration vector"))
                })
                .collect()
        })
        .transpose()
}

fn vector<const N: usize>(
    node: &OrderedSourceDocumentNode,
    name: &str,
) -> io::Result<Option<[f32; N]>> {
    numbers(node, name)?
        .map(|values| {
            values
                .try_into()
                .map_err(|_| invalid("water configuration vector has wrong length"))
        })
        .transpose()
}

fn colour<const N: usize>(
    node: &OrderedSourceDocumentNode,
    name: &str,
) -> io::Result<Option<[u8; N]>> {
    vector::<N>(node, name)?
        .map(|values| {
            if values
                .iter()
                .any(|value| !(0.0..=255.0).contains(value) || value.fract() != 0.0)
            {
                return Err(invalid("invalid water configuration colour"));
            }
            Ok(values.map(|value| value as u8))
        })
        .transpose()
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
