use std::io;

use openzt2_game_data::terrain::{
    TerrainBiome, TerrainBiomeWaterPresentation, TerrainPresentation,
    TerrainWaterGeometricWavePresentation, TerrainWaterReflectionPresentation,
    TerrainWaterRefractionPresentation, TerrainWaterRippleWavePresentation,
    TerrainWaterSurfaceMaterialPresentation, TerrainWaterTextureAnimation, TerrainWaterfall,
};

use crate::assets::source_document::{
    document_semantics::{classify_source_document, SourceDocumentKind},
    ordered_source_document_types::{OrderedSourceDocument, OrderedSourceDocumentNode},
    path::AssetPath,
};

/// Returns the authored vertical origin shared by terrain DAT files.
pub(in crate::assets) fn read_authored_terrain_base_height_metres(
    source_documents: &[OrderedSourceDocument],
) -> io::Result<f32> {
    let terrain_source_document = find_authored_terrain_settings_document(source_documents)?;
    terrain_source_document
        .root
        .attribute("nodeMinHeight")
        .and_then(parse_blue_fang_terrain_number)
        .filter(|value| value.is_finite())
        .ok_or_else(|| invalid_terrain_source("BFTerrain has invalid nodeMinHeight"))
}

pub(in crate::assets) fn lower_authored_terrain_presentation_and_biomes_and_waterfall(
    source_documents: &[OrderedSourceDocument],
    biome_names: &[String],
) -> io::Result<(
    TerrainPresentation,
    Vec<TerrainBiome>,
    Option<TerrainWaterfall>,
)> {
    let terrain_source_document = find_authored_terrain_settings_document(source_documents)?;
    let terrain_presentation = TerrainPresentation {
        detail_texture: required_root_attribute(terrain_source_document, "detailTexture")?
            .to_owned(),
        detail_texture_repetition: required_root_number(
            terrain_source_document,
            "detailTextureRepetition",
        )?,
        cliff_rise_per_metre: [
            required_root_number(terrain_source_document, "cliffZPerMeterHoriz")?,
            required_root_number(terrain_source_document, "cliffZPerMeterDiag")?,
        ],
        surface_resolution: 256,
        surface_margin_cells: 4,
        full_alpha_composition: true,
    };
    let terrain_biomes = biome_names
        .iter()
        .map(|biome_name| lower_authored_terrain_biome(source_documents, biome_name))
        .collect::<io::Result<Vec<_>>>()?;
    let terrain_waterfall = lower_authored_terrain_waterfall(source_documents)?;
    Ok((terrain_presentation, terrain_biomes, terrain_waterfall))
}

pub(in crate::assets) fn find_authored_terrain_waterfall_particle_source_document_path(
    source_documents: &[OrderedSourceDocument],
) -> io::Result<Option<String>> {
    let Some(waterfall_source_document) =
        find_authored_terrain_waterfall_document(source_documents)?
    else {
        return Ok(None);
    };
    find_descendant_source_node(waterfall_source_document, "particle")
        .ok_or_else(|| invalid_terrain_source("BFWaterfall has no particle"))
        .and_then(|particle_source_node| {
            required_source_node_attribute(particle_source_node, "name")
        })
        .map(|particle_name| Some(format!("{particle_name}.xml")))
}

pub(in crate::assets) fn find_authored_terrain_water_material_source_document_paths(
    source_documents: &[OrderedSourceDocument],
    biome_names: &[String],
) -> io::Result<Vec<String>> {
    let mut water_material_source_paths = std::collections::BTreeSet::new();
    for biome_name in biome_names
        .iter()
        .filter(|biome_name| !biome_name.eq_ignore_ascii_case("gridland"))
    {
        let biome_source_document =
            find_authored_terrain_biome_document(source_documents, biome_name)?;
        let Some(water_source_node) = biome_source_document
            .root
            .element_children()
            .find(|source_node| source_node.name.eq_ignore_ascii_case("water"))
        else {
            continue;
        };
        water_material_source_paths.insert(
            water_source_node
                .attribute("waterXML")
                .filter(|source_path| !source_path.is_empty())
                .unwrap_or("Materials/waterflat.xml")
                .to_owned(),
        );
    }
    Ok(water_material_source_paths.into_iter().collect())
}

fn find_authored_terrain_settings_document(
    source_documents: &[OrderedSourceDocument],
) -> io::Result<&OrderedSourceDocument> {
    source_documents
        .iter()
        .filter(|source_document| {
            classify_source_document(source_document) == SourceDocumentKind::Terrain
        })
        .find(|source_document| source_document.root.name.eq_ignore_ascii_case("BFTerrain"))
        .ok_or_else(|| invalid_terrain_source("authored document set has no BFTerrain document"))
}

fn lower_authored_terrain_waterfall(
    source_documents: &[OrderedSourceDocument],
) -> io::Result<Option<TerrainWaterfall>> {
    let Some(waterfall_source_document) =
        find_authored_terrain_waterfall_document(source_documents)?
    else {
        return Ok(None);
    };
    let physical_object_source_node =
        find_descendant_source_node(waterfall_source_document, "BFPhysObj")
            .ok_or_else(|| invalid_terrain_source("BFWaterfall has no BFPhysObj"))?;
    let terrain_decal_source_node =
        find_descendant_source_node(waterfall_source_document, "BFTerrainDecalComponent")
            .ok_or_else(|| invalid_terrain_source("BFWaterfall has no terrain decal"))?;
    let particle_source_node =
        find_descendant_source_node(waterfall_source_document, "particle")
            .ok_or_else(|| invalid_terrain_source("BFWaterfall has no particle"))?;
    let particle_name = required_source_node_attribute(particle_source_node, "name")?;
    let normalized_particle_name = normalize_particle_source_document_name(particle_name);
    let particle_source_document = source_documents
        .iter()
        .find(|candidate_source_document| {
            candidate_source_document
                .path
                .as_str()
                .rsplit('/')
                .next()
                .and_then(|file_name| file_name.rsplit_once('.').map(|(stem, _)| stem))
                .map(normalize_particle_source_document_name)
                .is_some_and(|candidate_name| candidate_name == normalized_particle_name)
        })
        .ok_or_else(|| invalid_terrain_source("BFWaterfall particle names no source document"))?;
    let particle_model_asset_path = particle_source_document
        .root
        .element_children()
        .find_map(|child_source_node| {
            child_source_node
                .name
                .eq_ignore_ascii_case("BFSceneGraphComponent")
                .then_some(child_source_node)
                .or_else(|| find_descendant_node_named(child_source_node, "BFSceneGraphComponent"))
        })
        .and_then(|scene_graph_source_node| scene_graph_source_node.attribute("modelfile"))
        .ok_or_else(|| invalid_terrain_source("BFWaterfall particle document has no model"))?;

    Ok(Some(TerrainWaterfall {
        decal_mask: required_source_node_attribute(terrain_decal_source_node, "imageName")?
            .to_owned(),
        decal_detail: required_source_node_attribute(terrain_decal_source_node, "detailImageName")?
            .to_owned(),
        particle_scene:
            crate::assets::model_source::native_model_source_lowering::
                native_model_scene_labelled_asset_path(
                    &AssetPath::new(particle_model_asset_path).key(),
                ),
        sound: find_descendant_source_node(waterfall_source_document, "BFSndComponent")
            .and_then(|sound_source_node| {
                sound_source_node
                    .attribute("filename")
                    .or_else(|| sound_source_node.attribute("file"))
            })
            .map(|sound_asset_path| AssetPath::new(sound_asset_path).key()),
        decal_width_metres: required_source_node_number(terrain_decal_source_node, "dwidth")?,
        decal_height_metres: required_source_node_number(terrain_decal_source_node, "dheight")?,
        height_offset_metres: required_source_node_number(
            terrain_decal_source_node,
            "heightOffset",
        )?,
        detail_v_scroll: required_source_node_number(terrain_decal_source_node, "detailVScroll")?,
        particle_scrunch: required_source_node_number(particle_source_node, "particleScrunch")?,
        alpha_blend: required_source_node_boolean(terrain_decal_source_node, "alphaBlend")?,
        double_sided: required_source_node_boolean(terrain_decal_source_node, "doubleSided")?,
        rotate_detail: required_source_node_boolean(terrain_decal_source_node, "rotateDetail")?,
        float_on_water: required_source_node_boolean(terrain_decal_source_node, "floatOnWater")?,
        water_effect: required_source_node_boolean(physical_object_source_node, "waterEffect")?,
    }))
}

fn find_authored_terrain_waterfall_document(
    source_documents: &[OrderedSourceDocument],
) -> io::Result<Option<&OrderedSourceDocument>> {
    let mut waterfall_source_documents = source_documents.iter().filter(|source_document| {
        source_document
            .root
            .name
            .eq_ignore_ascii_case("BFWaterfall")
    });
    let waterfall_source_document = waterfall_source_documents.next();
    if waterfall_source_documents.next().is_some() {
        return Err(invalid_terrain_source(
            "authored document set contains ambiguous BFWaterfall documents",
        ));
    }
    Ok(waterfall_source_document)
}

fn lower_authored_terrain_biome(
    source_documents: &[OrderedSourceDocument],
    biome_name: &str,
) -> io::Result<TerrainBiome> {
    if biome_name.eq_ignore_ascii_case("gridland")
        && !source_documents.iter().any(|source_document| {
            source_document.root.name.eq_ignore_ascii_case("BFGBiome")
                && source_document
                    .root
                    .attribute("name")
                    .is_some_and(|candidate_name| candidate_name.eq_ignore_ascii_case(biome_name))
        })
    {
        return Ok(default_gridland_terrain_biome(biome_name));
    }
    let biome_source_document = find_authored_terrain_biome_document(source_documents, biome_name)?;
    let required_biome_child = |wanted_child_name: &str| {
        biome_source_document
            .root
            .element_children()
            .find(|source_node| source_node.name.eq_ignore_ascii_case(wanted_child_name))
            .ok_or_else(|| invalid_terrain_source("BFGBiome is missing a terrain child"))
    };
    let required_brush_set = |wanted_child_name: &str| -> io::Result<[String; 4]> {
        let brush_set_source_node = required_biome_child(wanted_child_name)?;
        ["brush", "brush1", "brush2", "brush3"]
            .map(|attribute_name| {
                brush_set_source_node
                    .attribute(attribute_name)
                    .map(str::to_owned)
                    .ok_or_else(|| invalid_terrain_source("BFGBiome brush set is incomplete"))
            })
            .into_iter()
            .collect::<io::Result<Vec<_>>>()?
            .try_into()
            .map_err(|_| invalid_terrain_source("BFGBiome brush set has the wrong size"))
    };
    let water_source_node = biome_source_document
        .root
        .element_children()
        .find(|source_node| source_node.name.eq_ignore_ascii_case("water"));

    let water_presentation = water_source_node
        .map(|water_source_node| {
            lower_authored_terrain_biome_water_presentation(source_documents, water_source_node)
        })
        .transpose()?;

    Ok(TerrainBiome {
        name: biome_name.to_owned(),
        ground_texture: required_root_attribute(biome_source_document, "groundTexture")?.to_owned(),
        cover_texture: required_root_attribute(biome_source_document, "groundCoverTexture")?
            .to_owned(),
        mix_mask: required_root_attribute(biome_source_document, "mixMask")?.to_owned(),
        cliff_textures: [
            "cliffHoriz",
            "cliffVert",
            "cliffDiag1",
            "cliffDiag2",
            "cliffUnaligned",
        ]
        .map(|attribute_name| {
            required_root_attribute(biome_source_document, attribute_name).map(str::to_owned)
        })
        .into_iter()
        .collect::<io::Result<Vec<_>>>()?
        .try_into()
        .map_err(|_| invalid_terrain_source("BFGBiome cliff set has the wrong size"))?,
        ground_brushes: required_brush_set("groundBrushes")?,
        cover_brushes: required_brush_set("groundCoverBrushes")?,
        ground_no_blend_brushes: required_brush_set("groundBrushesNoBlend")?,
        cover_no_blend_brushes: required_brush_set("groundCoverBrushesNoBlend")?,
        draw_order: required_root_attribute(biome_source_document, "drawOrder")?
            .parse()
            .map_err(|_| invalid_terrain_source("BFGBiome has invalid drawOrder"))?,
        shallow_water_depth_mm: lower_authored_water_depth_to_canonical_millimetres(
            water_source_node,
            "shallowDepth",
        )?,
        deep_water_depth_mm: lower_authored_water_depth_to_canonical_millimetres(
            water_source_node,
            "deepDepth",
        )?,
        water_shore_offset_mm: lower_authored_water_depth_to_canonical_millimetres(
            water_source_node,
            "shoreOffset",
        )?,
        water_presentation,
    })
}

fn find_authored_terrain_biome_document<'a>(
    source_documents: &'a [OrderedSourceDocument],
    biome_name: &str,
) -> io::Result<&'a OrderedSourceDocument> {
    let mut matching_biome_source_documents = source_documents.iter().filter(|source_document| {
        source_document.root.name.eq_ignore_ascii_case("BFGBiome")
            && source_document
                .root
                .attribute("name")
                .is_some_and(|candidate_name| candidate_name.eq_ignore_ascii_case(biome_name))
    });
    let biome_source_document = matching_biome_source_documents.next().ok_or_else(|| {
        invalid_terrain_source(&format!(
            "terrain references undefined BFGBiome {biome_name}"
        ))
    })?;
    if matching_biome_source_documents.next().is_some() {
        return Err(invalid_terrain_source(
            "terrain references an ambiguous BFGBiome",
        ));
    }
    Ok(biome_source_document)
}

fn lower_authored_terrain_biome_water_presentation(
    source_documents: &[OrderedSourceDocument],
    water_source_node: &OrderedSourceDocumentNode,
) -> io::Result<TerrainBiomeWaterPresentation> {
    let texture_animations = water_source_node
        .element_children()
        .filter(|source_node| source_node.name.eq_ignore_ascii_case("texture"))
        .map(lower_authored_terrain_water_texture_animation)
        .collect::<io::Result<Vec<_>>>()?
        .try_into()
        .map_err(|_| {
            invalid_terrain_source("BFGBiome water must author exactly two texture animations")
        })?;
    Ok(TerrainBiomeWaterPresentation {
        vertex_colour_low: parse_authored_rgba255(required_source_node_attribute(
            water_source_node,
            "vertexColorLow",
        )?)?,
        vertex_colour_medium: parse_authored_rgba255(required_source_node_attribute(
            water_source_node,
            "vertexColorMed",
        )?)?,
        vertex_colour_high: parse_authored_rgba255(required_source_node_attribute(
            water_source_node,
            "vertexColorHigh",
        )?)?,
        gloss_map: required_source_node_attribute(water_source_node, "glossMap")?.to_owned(),
        texture_animations,
        surface_material: lower_authored_terrain_water_surface_material_presentation(
            source_documents,
            water_source_node,
        )?,
    })
}

fn lower_authored_terrain_water_surface_material_presentation(
    source_documents: &[OrderedSourceDocument],
    water_source_node: &OrderedSourceDocumentNode,
) -> io::Result<TerrainWaterSurfaceMaterialPresentation> {
    let material_source_path = water_source_node
        .attribute("waterXML")
        .filter(|source_path| !source_path.is_empty())
        .unwrap_or("Materials/waterflat.xml");
    let material_source_path_key = AssetPath::new(material_source_path).key();
    let material_source_document = source_documents
        .iter()
        .find(|source_document| source_document.path.key() == material_source_path_key)
        .ok_or_else(|| {
            invalid_terrain_source("BFGBiome water material source document is missing")
        })?;
    if !material_source_document
        .root
        .name
        .eq_ignore_ascii_case("water")
    {
        return Err(invalid_terrain_source(
            "BFGBiome water material document has the wrong root",
        ));
    }
    let required_material_child = |wanted_child_name: &str| {
        material_source_document
            .root
            .element_children()
            .find(|source_node| source_node.name.eq_ignore_ascii_case(wanted_child_name))
            .ok_or_else(|| invalid_terrain_source("water material is missing a required child"))
    };
    let reflection_source_node = required_material_child("reflection")?;
    let refraction_source_node = required_material_child("refraction")?;
    let geometric_wave_source_node = required_material_child("geometric")?;
    let ripple_wave_source_node = required_material_child("ripple")?;
    Ok(TerrainWaterSurfaceMaterialPresentation {
        reflection: TerrainWaterReflectionPresentation {
            strength_percent: required_source_node_number(reflection_source_node, "strength")?,
            ambient_colour_rgb255: parse_authored_rgb255(required_source_node_attribute(
                reflection_source_node,
                "ambient",
            )?)?,
            tint_colour_rgb255: parse_authored_rgb255(required_source_node_attribute(
                reflection_source_node,
                "tint",
            )?)?,
            map_size_metres: required_source_node_number(reflection_source_node, "mapsize")?,
            bumpiness_percent: required_source_node_number(reflection_source_node, "bumpiness")?,
            falloff_metres: required_source_node_number(reflection_source_node, "falloff")?,
            fresnel_percent: required_source_node_number(reflection_source_node, "fresnel")?,
        },
        refraction: TerrainWaterRefractionPresentation {
            ambient_colour_rgb255: parse_authored_rgb255(required_source_node_attribute(
                refraction_source_node,
                "ambient",
            )?)?,
            tint_colour_rgb255: parse_authored_rgb255(required_source_node_attribute(
                refraction_source_node,
                "tint",
            )?)?,
            map_size_metres: required_source_node_number(refraction_source_node, "mapsize")?,
            bumpiness_percent: required_source_node_number(refraction_source_node, "bumpiness")?,
            falloff_metres: required_source_node_number(refraction_source_node, "falloff")?,
        },
        geometric_waves: TerrainWaterGeometricWavePresentation {
            minimum_amplitude_percent: required_source_node_number(
                geometric_wave_source_node,
                "minAmplitude",
            )?,
            maximum_amplitude_percent: required_source_node_number(
                geometric_wave_source_node,
                "maxAmplitude",
            )?,
            chop_percent: required_source_node_number(geometric_wave_source_node, "chop")?,
        },
        ripple_waves: TerrainWaterRippleWavePresentation {
            lifespan_seconds: required_source_node_number(ripple_wave_source_node, "lifespan")?,
            startup_seconds: required_source_node_number(ripple_wave_source_node, "startup")?,
            minimum_amplitude_percent: required_source_node_number(
                ripple_wave_source_node,
                "minAmplitude",
            )?,
            maximum_amplitude_percent: required_source_node_number(
                ripple_wave_source_node,
                "maxAmplitude",
            )?,
            chop_percent: required_source_node_number(ripple_wave_source_node, "chop")?,
            speed_metres_per_second: required_source_node_number(ripple_wave_source_node, "speed")?,
            ramp_minimum_metres: required_source_node_number(ripple_wave_source_node, "rampmin")?,
            ramp_maximum_metres: required_source_node_number(ripple_wave_source_node, "rampmax")?,
        },
    })
}

fn lower_authored_terrain_water_texture_animation(
    texture_source_node: &OrderedSourceDocumentNode,
) -> io::Result<TerrainWaterTextureAnimation> {
    Ok(TerrainWaterTextureAnimation {
        texture: required_source_node_attribute(texture_source_node, "name")?.to_owned(),
        grid_dimensions: [
            required_source_node_number(texture_source_node, "gridWidth")?,
            required_source_node_number(texture_source_node, "gridHeight")?,
        ],
        start_minimum_uv: parse_authored_vector2(required_source_node_attribute(
            texture_source_node,
            "startMinUV",
        )?)?,
        start_maximum_uv: parse_authored_vector2(required_source_node_attribute(
            texture_source_node,
            "startMaxUV",
        )?)?,
        end_minimum_uv: parse_authored_vector2(required_source_node_attribute(
            texture_source_node,
            "endMinUV",
        )?)?,
        end_maximum_uv: parse_authored_vector2(required_source_node_attribute(
            texture_source_node,
            "endMaxUV",
        )?)?,
        scroll_interval: required_source_node_number(texture_source_node, "scrollInterval")?,
        scroll_texture: required_source_node_boolean(texture_source_node, "scrollTexture")?,
        ping_pong: required_source_node_boolean(texture_source_node, "pingpong")?,
    })
}

fn default_gridland_terrain_biome(biome_name: &str) -> TerrainBiome {
    let ground_texture = "biomes/gridland/gridland_ground_256.dds".to_owned();
    let brushes = ["brush1", "brush2", "brush3", "brush4"]
        .map(|brush_name| format!("biomes/gridland/{brush_name}.dds"));
    TerrainBiome {
        name: biome_name.to_owned(),
        ground_texture: ground_texture.clone(),
        cover_texture: "biomes/gridland/gridland_cover_256.dds".to_owned(),
        mix_mask: "biomes/gridland/mixmask_gridland.dds".to_owned(),
        cliff_textures: std::array::from_fn(|_| ground_texture.clone()),
        ground_brushes: brushes.clone(),
        cover_brushes: brushes.clone(),
        ground_no_blend_brushes: brushes.clone(),
        cover_no_blend_brushes: brushes,
        draw_order: 0,
        shallow_water_depth_mm: None,
        deep_water_depth_mm: None,
        water_shore_offset_mm: None,
        water_presentation: None,
    }
}

fn parse_authored_rgba255(authored_value: &str) -> io::Result<[u8; 4]> {
    authored_value
        .split_ascii_whitespace()
        .map(|component| {
            component
                .parse::<u8>()
                .map_err(|_| invalid_terrain_source("BFGBiome water has an invalid RGBA colour"))
        })
        .collect::<io::Result<Vec<_>>>()?
        .try_into()
        .map_err(|_| invalid_terrain_source("BFGBiome water RGBA colour has the wrong size"))
}

fn parse_authored_rgb255(authored_value: &str) -> io::Result<[u8; 3]> {
    authored_value
        .split_ascii_whitespace()
        .map(|component| {
            component
                .strip_suffix('.')
                .unwrap_or(component)
                .parse::<u8>()
                .map_err(|_| invalid_terrain_source("water material has an invalid RGB colour"))
        })
        .collect::<io::Result<Vec<_>>>()?
        .try_into()
        .map_err(|_| invalid_terrain_source("water material RGB colour has the wrong size"))
}

fn parse_authored_vector2(authored_value: &str) -> io::Result<[f32; 2]> {
    authored_value
        .split_ascii_whitespace()
        .map(|component| {
            parse_blue_fang_terrain_number(component)
                .filter(|value| value.is_finite())
                .ok_or_else(|| invalid_terrain_source("BFGBiome water has an invalid UV vector"))
        })
        .collect::<io::Result<Vec<_>>>()?
        .try_into()
        .map_err(|_| invalid_terrain_source("BFGBiome water UV vector has the wrong size"))
}

fn lower_authored_water_depth_to_canonical_millimetres(
    water_source_node: Option<&OrderedSourceDocumentNode>,
    attribute_name: &str,
) -> io::Result<Option<i32>> {
    let Some(authored_depth) =
        water_source_node.and_then(|source_node| source_node.attribute(attribute_name))
    else {
        return Ok(None);
    };
    let depth_metres = parse_blue_fang_terrain_number(authored_depth)
        .filter(|value| value.is_finite())
        .ok_or_else(|| invalid_terrain_source("BFGBiome has an invalid water depth"))?;
    let depth_millimetres = (f64::from(depth_metres) * 1_000.0).round();
    if depth_millimetres < f64::from(i32::MIN) || depth_millimetres > f64::from(i32::MAX) {
        return Err(invalid_terrain_source(
            "BFGBiome water depth exceeds canonical millimetres",
        ));
    }
    #[allow(
        clippy::cast_possible_truncation,
        reason = "the rounded value is checked against the complete i32 range above"
    )]
    let depth_millimetres = depth_millimetres as i32;
    Ok(Some(depth_millimetres))
}

fn normalize_particle_source_document_name(source_name: &str) -> String {
    source_name
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn find_descendant_source_node<'a>(
    source_document: &'a OrderedSourceDocument,
    wanted_node_name: &str,
) -> Option<&'a OrderedSourceDocumentNode> {
    source_document
        .root
        .element_children()
        .find_map(|source_node| {
            source_node
                .name
                .eq_ignore_ascii_case(wanted_node_name)
                .then_some(source_node)
                .or_else(|| find_descendant_node_named(source_node, wanted_node_name))
        })
}

fn find_descendant_node_named<'a>(
    source_node: &'a OrderedSourceDocumentNode,
    wanted_node_name: &str,
) -> Option<&'a OrderedSourceDocumentNode> {
    source_node
        .element_children()
        .find_map(|child_source_node| {
            child_source_node
                .name
                .eq_ignore_ascii_case(wanted_node_name)
                .then_some(child_source_node)
                .or_else(|| find_descendant_node_named(child_source_node, wanted_node_name))
        })
}

fn required_root_attribute<'a>(
    source_document: &'a OrderedSourceDocument,
    attribute_name: &str,
) -> io::Result<&'a str> {
    source_document
        .root
        .attribute(attribute_name)
        .ok_or_else(|| invalid_terrain_source("authored document is missing a required attribute"))
}

fn required_root_number(
    source_document: &OrderedSourceDocument,
    attribute_name: &str,
) -> io::Result<f32> {
    parse_blue_fang_terrain_number(required_root_attribute(source_document, attribute_name)?)
        .filter(|value| value.is_finite())
        .ok_or_else(|| invalid_terrain_source("authored document has an invalid numeric attribute"))
}

fn required_source_node_attribute<'a>(
    source_node: &'a OrderedSourceDocumentNode,
    attribute_name: &str,
) -> io::Result<&'a str> {
    source_node
        .attribute(attribute_name)
        .ok_or_else(|| invalid_terrain_source("authored node is missing a required attribute"))
}

fn required_source_node_number(
    source_node: &OrderedSourceDocumentNode,
    attribute_name: &str,
) -> io::Result<f32> {
    parse_blue_fang_terrain_number(required_source_node_attribute(source_node, attribute_name)?)
        .filter(|value| value.is_finite())
        .ok_or_else(|| invalid_terrain_source("authored node has an invalid numeric attribute"))
}

fn required_source_node_boolean(
    source_node: &OrderedSourceDocumentNode,
    attribute_name: &str,
) -> io::Result<bool> {
    match source_node
        .attribute(attribute_name)
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("true" | "1") => Ok(true),
        Some("false" | "0") => Ok(false),
        _ => Err(invalid_terrain_source("BFWaterfall has an invalid boolean")),
    }
}

fn parse_blue_fang_terrain_number(authored_value: &str) -> Option<f32> {
    authored_value
        .trim()
        .trim_end_matches(['f', 'F'])
        .parse()
        .ok()
}

fn invalid_terrain_source(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
