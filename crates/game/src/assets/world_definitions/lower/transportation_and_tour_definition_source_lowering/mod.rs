use super::catalogue_entry_source_lowering::{
    authored_catalogue_purchase_sort_fallback_type_name, authored_catalogue_purchase_sort_key,
};
use super::source_element_tree_search::{
    authored_type_family_component_attribute, authored_type_family_components,
    authored_type_family_elements_named, find_descendant,
    find_descendant_named_with_nonempty_attribute,
};
use super::world_definition_lowering_tables::WorldDefinitionLoweringTables;
use super::world_definition_source_value_reading_and_conversion::{
    asset, bool_or, element_bool, id, money_cents, parse_f32_pair, parse_f32_triplet,
    required_element, required_element_number, required_number, source_distance_cm,
};
use crate::assets::source_document::blue_fang_source_numeric_lexeme::parse_blue_fang_source_numeric_lexeme;
use crate::assets::source_document::ordered_source_document_types::OrderedSourceDocumentNode;
use crate::assets::source_document::resolved_source_record_index::{BindError, RecordView};
use crate::assets::source_document::source_document_semantic_name::{
    canonicalize_source_document_record_key, source_document_names_are_semantically_equal,
};
use openzt2_game_data::world_definitions::catalogue_and_progression::catalogue_definition_types::{
    CatalogueCategory, CatalogueEntry, CatalogueFilterFlags,
};
use openzt2_game_data::world_definitions::transportation_and_tours::{
    GroundTransportTrackPiecePresentationDefinition, SkyTowerTrackPresentationDefinition,
    TourViewDefinition, TransportationStationDefinition, TransportationTrackDefinition,
    TransportationTrackKind, TransportationVehicleDefinition,
    TransportationVehicleSeatAnimationState, TransportationVehicleSeatDefinition,
    TransportationVehicleSeatEmoteBand,
};
use std::collections::BTreeMap;

fn authored_transportation_track_kind(
    record: &RecordView<'_, '_>,
) -> Result<TransportationTrackKind, BindError> {
    match authored_type_family_component_attribute(
        record,
        "BFAIEntityDataShared",
        "s_TransportationMode",
    )
    .map(canonicalize_source_document_record_key)
    .as_deref()
    {
        Some("ground") => Ok(TransportationTrackKind::Ground),
        Some("sky") => Ok(TransportationTrackKind::Sky),
        Some(mode) => Err(BindError::record(
            record,
            format!("unsupported authored transportation mode {mode:?}"),
        )),
        None => Err(BindError::record(
            record,
            "transport binder has no authored transportation mode",
        )),
    }
}

fn authored_transportation_offset_metres(
    record: &RecordView<'_, '_>,
    attribute: &str,
) -> Result<[f32; 3], BindError> {
    let source =
        authored_type_family_component_attribute(record, "BFAIEntityDataShared", attribute)
            .ok_or_else(|| {
                BindError::record(
                    record,
                    format!("missing authored transport offset {attribute}"),
                )
            })?;
    let source_values = parse_f32_triplet(source)?;
    if !source_values.into_iter().all(f32::is_finite) {
        return Err(BindError::record(
            record,
            format!("authored transport offset {attribute} is not finite"),
        ));
    }
    Ok(source_values)
}

fn authored_transportation_planar_offset_centimetres(
    record: &RecordView<'_, '_>,
    attribute: &str,
) -> Result<[i16; 2], BindError> {
    let source =
        authored_type_family_component_attribute(record, "BFAIEntityDataShared", attribute)
            .ok_or_else(|| {
                BindError::record(
                    record,
                    format!("missing authored transport offset {attribute}"),
                )
            })?;
    let [source_x, source_z, _authored_heading_degrees] = parse_f32_triplet(source)?;
    let mut centimetres = [0_i16; 2];
    for (index, metres) in [source_x, source_z].into_iter().enumerate() {
        centimetres[index] = i16::try_from(source_distance_cm(f64::from(metres), record)?)
            .map_err(|_| {
                BindError::record(
                    record,
                    format!("authored transport offset {attribute} exceeds i16 centimetres"),
                )
            })?;
    }
    Ok(centimetres)
}

fn authored_transport_station_queue_capacity(
    record: &RecordView<'_, '_>,
) -> Result<u16, BindError> {
    authored_type_family_elements_named(record, "queue")
        .into_iter()
        .flat_map(|queue| queue.element_children())
        .filter(|slot| {
            source_document_names_are_semantically_equal(
                slot.name.as_str(),
                "BFGEntityContainerSlot",
            )
        })
        .filter_map(|slot| slot.attribute_named_any(&["capacity"]))
        .try_fold(0_u16, |capacity, value| {
            let queue_capacity =
                parse_blue_fang_source_numeric_lexeme::<u16>(value).ok_or_else(|| {
                    BindError::record(record, format!("invalid station queue capacity {value:?}"))
                })?;
            capacity
                .checked_add(queue_capacity)
                .ok_or_else(|| BindError::record(record, "station queue capacity exceeds u16"))
        })
}

fn lower_authored_sky_tower_track_presentation(
    record: &RecordView<'_, '_>,
    resolved_source_records: &[RecordView<'_, '_>],
    entity_scene_paths: &BTreeMap<String, String>,
) -> Result<SkyTowerTrackPresentationDefinition, BindError> {
    let expando = authored_type_family_components(record, "ZTExpandoComponent")
        .into_iter()
        .next()
        .ok_or_else(|| BindError::record(record, "sky tower has no authored ZTExpandoComponent"))?;
    let number = |element: &'_ OrderedSourceDocumentNode, attribute: &str| {
        element
            .attribute_named_any(&[attribute])
            .and_then(parse_blue_fang_source_numeric_lexeme::<f32>)
            .filter(|value| value.is_finite() && *value > 0.0)
            .ok_or_else(|| BindError::record(record, format!("invalid sky-tower {attribute}")))
    };
    let rope = resolved_source_records
        .iter()
        .find(|candidate| {
            candidate
                .source_path()
                .eq_ignore_ascii_case("entities/transportation/track/ai/testropeobj.xml")
        })
        .and_then(|candidate| candidate.descendant_named("BFRopeComponent"))
        .ok_or_else(|| {
            BindError::record(record, "sky track has no authored BFRopeComponent record")
        })?;
    let rope_number = |attribute: &str| {
        rope.attribute_named_any(&[attribute])
            .and_then(parse_blue_fang_source_numeric_lexeme::<f32>)
            .filter(|value| value.is_finite() && *value > 0.0)
            .ok_or_else(|| BindError::record(record, format!("invalid sky-track rope {attribute}")))
    };
    Ok(SkyTowerTrackPresentationDefinition {
        columns: [
            super::expanding_column_source_lowering::lower_expanding_column_presentation(
                record,
                "TopPiece2",
                entity_scene_paths,
            )?,
            super::expanding_column_source_lowering::lower_expanding_column_presentation(
                record,
                "TopPiece1",
                entity_scene_paths,
            )?,
            super::expanding_column_source_lowering::lower_expanding_column_presentation(
                record,
                "TopPiece3",
                entity_scene_paths,
            )?,
        ],
        initial_height_metres: number(expando, "height")?,
        minimum_height_metres: number(expando, "minHeight")?,
        rope_texture: rope
            .attribute_named_any(&["textureName"])
            .map(id)
            .ok_or_else(|| BindError::record(record, "sky-track rope has no textureName"))?,
        rope_radius_metres: rope_number("lineRadius")?,
        rope_droop_metres: rope_number("droopDist")?,
        rope_droop_maximum_length_metres: rope_number("droopDistMaxLen")?,
        rope_points_per_metre: rope_number("pointsPerMeter")?,
    })
}

fn lower_authored_ground_transport_track_piece_presentations(
    record: &RecordView<'_, '_>,
) -> Result<Vec<GroundTransportTrackPiecePresentationDefinition>, BindError> {
    authored_type_family_elements_named(record, "ZTTransportGroundTrackTextureData")
        .into_iter()
        .map(|texture_data| {
            let piece_type = texture_data
                .attribute_named_any(&["pieceType"])
                .and_then(parse_blue_fang_source_numeric_lexeme)
                .ok_or_else(|| {
                    BindError::record(record, "ground-track texture data has no pieceType")
                })?;
            let appearance = texture_data
                .attribute_named_any(&["appearance"])
                .and_then(parse_blue_fang_source_numeric_lexeme)
                .ok_or_else(|| {
                    BindError::record(record, "ground-track texture data has no appearance")
                })?;
            let ground_decal_texture = texture_data
                .attribute_named_any(&["groundDecalTexture"])
                .map(id)
                .ok_or_else(|| {
                    BindError::record(
                        record,
                        "ground-track texture data has no groundDecalTexture",
                    )
                })?;
            let ground_decal_size_metres = texture_data
                .attribute_named_any(&["groundDecalSize"])
                .map(parse_f32_pair)
                .transpose()?
                .unwrap_or([0.0; 2]);
            if !ground_decal_size_metres
                .into_iter()
                .all(|size| size.is_finite() && size > 0.0)
            {
                return Err(BindError::record(
                    record,
                    "ground-track decal size is not positive and finite",
                ));
            }
            let ground_decal_offset_metres = texture_data
                .attribute_named_any(&["groundDecalOffset"])
                .map(parse_f32_pair)
                .transpose()?
                .unwrap_or([0.0; 2]);
            let ground_decal_rotation_degrees = texture_data
                .attribute_named_any(&["groundDecalRotation"])
                .map(|value| {
                    parse_blue_fang_source_numeric_lexeme::<f32>(value).ok_or_else(|| {
                        BindError::record(record, "invalid ground-track decal rotation")
                    })
                })
                .transpose()?
                .unwrap_or(0.0);
            let elevated_path_texture = texture_data
                .attribute_named_any(&["elevatedPathTexture"])
                .map(id)
                .ok_or_else(|| {
                    BindError::record(
                        record,
                        "ground-track texture data has no elevatedPathTexture",
                    )
                })?;
            let elevated_path_rotation_degrees = texture_data
                .attribute_named_any(&["elevatedPathRot"])
                .map(|value| {
                    parse_blue_fang_source_numeric_lexeme::<f32>(value).ok_or_else(|| {
                        BindError::record(record, "invalid ground-track elevated-path rotation")
                    })
                })
                .transpose()?
                .unwrap_or(0.0);
            Ok(GroundTransportTrackPiecePresentationDefinition {
                piece_type,
                appearance,
                ground_decal_texture,
                ground_decal_rotation_radians: ground_decal_rotation_degrees.to_radians(),
                ground_decal_size_metres,
                ground_decal_offset_metres,
                elevated_path_texture,
                elevated_path_rotation_radians: elevated_path_rotation_degrees.to_radians(),
            })
        })
        .collect()
}

pub(super) fn bind_station(
    record: &RecordView<'_, '_>,
    output: &mut WorldDefinitionLoweringTables,
) -> Result<(), BindError> {
    if record
        .source_document_element()
        .attribute_named_any(&["abstract"])
        .is_some_and(|value| {
            matches!(
                canonicalize_source_document_record_key(value).as_str(),
                "true" | "1"
            )
        })
        || record
            .descendant_with_attribute("event", "msg", "ZT_SETPLACEMENTOBJECT")
            .is_none()
    {
        return Ok(());
    }
    let capacity = authored_transport_station_queue_capacity(record)?;
    if capacity == 0 {
        return Err(BindError::record(
            record,
            "transport station has no authored queue capacity",
        ));
    }
    let kind = authored_transportation_track_kind(record)?;
    let cardinal_endpoint_offsets_metres = (kind == TransportationTrackKind::Ground)
        .then(|| {
            Ok([
                authored_transportation_offset_metres(record, "p_EntryCardinal")?,
                authored_transportation_offset_metres(record, "p_ExitCardinal")?,
            ])
        })
        .transpose()?;
    let diagonal_endpoint_offsets_metres = (kind == TransportationTrackKind::Ground)
        .then(|| {
            Ok([
                authored_transportation_offset_metres(record, "p_EntryDiagonal")?,
                authored_transportation_offset_metres(record, "p_ExitDiagonal")?,
            ])
        })
        .transpose()?;
    output
        .document
        .stations
        .push(TransportationStationDefinition {
            id: id(record.key),
            object: id(record.key),
            kind,
            cardinal_endpoint_offsets_metres,
            diagonal_endpoint_offsets_metres,
            passenger_entry_offsets_cm: [
                authored_transportation_planar_offset_centimetres(record, "p_PassengerEnterA")?,
                authored_transportation_planar_offset_centimetres(record, "p_PassengerEnterB")?,
            ],
            capacity,
        });
    Ok(())
}

pub(super) fn bind_track(
    record: &RecordView<'_, '_>,
    resolved_source_records: &[RecordView<'_, '_>],
    entity_scene_paths: &BTreeMap<String, String>,
    output: &mut WorldDefinitionLoweringTables,
) -> Result<(), BindError> {
    if record
        .source_document_element()
        .attribute_named_any(&["abstract"])
        .is_some_and(|value| {
            matches!(
                canonicalize_source_document_record_key(value).as_str(),
                "true" | "1"
            )
        })
        || record.descendant_named("UIToggleButton").is_none()
    {
        return Ok(());
    }
    let kind = authored_transportation_track_kind(record)?;
    let maximum_slope =
        authored_type_family_component_attribute(record, "BFAIEntityDataShared", "f_MaximumSlope")
            .map(|value| {
                parse_blue_fang_source_numeric_lexeme::<f64>(value)
                    .filter(|maximum_slope| maximum_slope.is_finite() && *maximum_slope >= 0.0)
                    .ok_or_else(|| {
                        BindError::record(record, "invalid authored maximum transport-track slope")
                    })
            })
            .transpose()?
            .unwrap_or_default();
    let maximum_grade_permille = (maximum_slope * 1_000.0).round();
    if !(0.0..=f64::from(u16::MAX)).contains(&maximum_grade_permille) {
        return Err(BindError::record(
            record,
            "authored maximum transport-track slope exceeds stored permille",
        ));
    }
    let purchase_cost_cents = find_descendant_named_with_nonempty_attribute(
        record.source_document_element(),
        "ZTEconomyComponent",
        "cost",
    )
    .and_then(|economy| economy.attribute_named_any(&["cost"]))
    .map(|cost| {
        parse_blue_fang_source_numeric_lexeme::<f64>(cost)
            .ok_or_else(|| BindError::record(record, "invalid authored transport-track cost"))
            .and_then(|cost| money_cents(cost, record).map(i64::from))
    })
    .transpose()?
    .unwrap_or_default();
    let sky_tower_presentation = (kind == TransportationTrackKind::Sky)
        .then(|| {
            lower_authored_sky_tower_track_presentation(
                record,
                resolved_source_records,
                entity_scene_paths,
            )
        })
        .transpose()?;
    let sky_maximum_connection_distance_metres = (kind == TransportationTrackKind::Sky)
        .then(|| {
            authored_type_family_component_attribute(
                record,
                "ZTPlacementData",
                "maxDistanceToConnect",
            )
            .and_then(parse_blue_fang_source_numeric_lexeme::<f32>)
            .filter(|distance| distance.is_finite() && *distance > 0.0)
            .ok_or_else(|| BindError::record(record, "sky track has no valid maxDistanceToConnect"))
        })
        .transpose()?;
    let sky_rope_clearance_metres = (kind == TransportationTrackKind::Sky)
        .then(|| {
            authored_type_family_elements_named(record, "ZTTransportSkyTrack")
                .into_iter()
                .find_map(|track| track.attribute_named_any(&["ropeClearance"]))
                .and_then(parse_blue_fang_source_numeric_lexeme::<f32>)
                .filter(|clearance| clearance.is_finite() && *clearance > 0.0)
                .ok_or_else(|| BindError::record(record, "sky track has no valid ropeClearance"))
        })
        .transpose()?;
    let ground_piece_presentations = if kind == TransportationTrackKind::Ground {
        lower_authored_ground_transport_track_piece_presentations(record)?
    } else {
        Vec::new()
    };
    let authored_offsets = |cardinal: (&str, &str), diagonal: (&str, &str)| {
        if kind == TransportationTrackKind::Sky {
            Ok(([[0.0; 3]; 2], [[0.0; 3]; 2]))
        } else {
            Ok((
                [
                    authored_transportation_offset_metres(record, cardinal.0)?,
                    authored_transportation_offset_metres(record, cardinal.1)?,
                ],
                [
                    authored_transportation_offset_metres(record, diagonal.0)?,
                    authored_transportation_offset_metres(record, diagonal.1)?,
                ],
            ))
        }
    };
    let (cardinal_endpoint_offsets_metres, diagonal_endpoint_offsets_metres) = authored_offsets(
        ("p_EntryCardinal", "p_ExitCardinal"),
        ("p_EntryDiagonal", "p_ExitDiagonal"),
    )?;
    output.document.tracks.push(TransportationTrackDefinition {
        id: id(record.key),
        kind,
        object: id(record.key),
        cardinal_endpoint_offsets_metres,
        diagonal_endpoint_offsets_metres,
        max_grade_permille: maximum_grade_permille as u16,
        purchase_cost_cents,
        sky_maximum_connection_distance_metres,
        sky_rope_clearance_metres,
        ground_piece_presentations,
        sky_tower_presentation,
    });
    if output
        .document
        .catalogue
        .iter()
        .all(|entry| entry.definition != id(record.key))
    {
        let purchase_button = record.descendant_named("UIToggleButton").ok_or_else(|| {
            BindError::record(
                record,
                "authored transport track has no catalogue purchase control",
            )
        })?;
        let icon = find_descendant(purchase_button, "default")
            .and_then(|element| element.attribute_named_any(&["image"]))
            .map(id)
            .ok_or_else(|| {
                BindError::record(record, "authored transport track has no catalogue icon")
            })?;
        let name_key = find_descendant(purchase_button, "UIHelpInfo")
            .and_then(|element| element.attribute_named_any(&["ids"]))
            .map(id)
            .unwrap_or_default();
        let mut type_tokens = record.type_tokens();
        if type_tokens.is_empty() {
            type_tokens.push(record.key.to_owned());
        }
        let kind = type_tokens
            .last()
            .map(|value| id(value))
            .unwrap_or_else(|| id(record.key));
        let kinds = type_tokens.iter().map(|value| id(value)).collect();
        output.document.catalogue.push(CatalogueEntry {
            filter_values: super::catalogue_entry_source_lowering::authored_catalogue_filter_values(
                record,
            ),
            id: id(&format!("catalogue/transport/{}", record.key)),
            definition: id(record.key),
            kind,
            kinds,
            category: CatalogueCategory::Transport,
            authored_purchase_sort_key: authored_catalogue_purchase_sort_key(record),
            authored_purchase_sort_fallback_type_name:
                authored_catalogue_purchase_sort_fallback_type_name(record),
            authored_type_registry_source_order: [u64::MAX; 3],
            filters: CatalogueFilterFlags::PURCHASABLE
                .with_additional_flags(CatalogueFilterFlags::BUILDABLE),
            name_key,
            icon,
        });
    }
    Ok(())
}

pub(super) fn bind_vehicle(
    record: &RecordView<'_, '_>,
    output: &mut WorldDefinitionLoweringTables,
) -> Result<(), BindError> {
    if record
        .source_document_element()
        .attribute_named_any(&["abstract"])
        .is_some_and(|value| {
            matches!(
                canonicalize_source_document_record_key(value).as_str(),
                "true" | "1"
            )
        })
        || record.descendant_named("UIToggleButton").is_none()
    {
        return Ok(());
    }
    let seats =
        u16::try_from(authored_type_family_elements_named(record, "seat").len()).map_err(|_| {
            BindError::record(record, "authored transport vehicle seat count exceeds u16")
        })?;
    if seats == 0 {
        return Err(BindError::record(
            record,
            "transport vehicle has no authored passenger seats",
        ));
    }
    let maximum_speed_metres_per_second =
        authored_type_family_component_attribute(record, "BFAIEntityDataShared", "f_BaseSpeed")
            .and_then(parse_blue_fang_source_numeric_lexeme::<f32>)
            .filter(|speed| speed.is_finite() && *speed > 0.0)
            .ok_or_else(|| {
                BindError::record(record, "transport vehicle has no valid authored base speed")
            })?;
    let purchase_cost_cents = find_descendant_named_with_nonempty_attribute(
        record.source_document_element(),
        "ZTEconomyComponent",
        "cost",
    )
    .and_then(|economy| economy.attribute_named_any(&["cost"]))
    .map(|cost| {
        parse_blue_fang_source_numeric_lexeme::<f64>(cost)
            .ok_or_else(|| BindError::record(record, "invalid authored transport vehicle cost"))
            .and_then(|cost| money_cents(cost, record).map(i64::from))
    })
    .transpose()?
    .unwrap_or_default();
    output
        .document
        .vehicles
        .push(TransportationVehicleDefinition {
            id: id(record.key),
            object: id(record.key),
            kind: authored_transportation_track_kind(record)?,
            purchase_cost_cents,
            seats,
            maximum_speed_metres_per_second,
        });
    Ok(())
}

pub(super) fn bind_authored_vehicle_seats(
    record: &RecordView<'_, '_>,
    output: &mut WorldDefinitionLoweringTables,
) -> Result<(), BindError> {
    if !source_document_names_are_semantically_equal(
        record.source_document_element().name.as_str(),
        "BFTypedBinder",
    ) || !record.has_type_token("vehicle")
    {
        return Ok(());
    }
    let Some(vehicle_data) = authored_type_family_components(record, "ZTAIVehicleData")
        .into_iter()
        .next()
    else {
        return Ok(());
    };
    for (seat_index, seat) in vehicle_data
        .element_children()
        .filter(|child| source_document_names_are_semantically_equal(child.name.as_str(), "seat"))
        .enumerate()
    {
        let view_limit = seat
            .element_children()
            .find(|child| {
                source_document_names_are_semantically_equal(child.name.as_str(), "ViewLimit")
            })
            .ok_or_else(|| BindError::record(record, "vehicle seat is missing ViewLimit"))?;
        let mut emote_bands = Vec::new();
        let emote_sets = seat
            .element_children()
            .find(|child| {
                source_document_names_are_semantically_equal(child.name.as_str(), "emoteSets")
            })
            .ok_or_else(|| BindError::record(record, "vehicle seat is missing emoteSets"))?;
        for band in emote_sets.element_children() {
            emote_bands.push(TransportationVehicleSeatEmoteBand {
                behavior_set: id(band.name.as_str()),
                score: [
                    required_element_number(&band, &["min"])?,
                    required_element_number(&band, &["max"])?,
                ],
            });
        }
        if emote_bands.is_empty() {
            return Err(BindError::record(
                record,
                "vehicle seat has no authored emote score bands",
            ));
        }

        let mut resolved_animation_states = Vec::new();
        let animation_states = seat
            .element_children()
            .find(|child| {
                source_document_names_are_semantically_equal(child.name.as_str(), "animStates")
            })
            .ok_or_else(|| BindError::record(record, "vehicle seat is missing animStates"))?;
        let limit_names = ["LeftForward", "LeftBack", "RightForward", "RightBack"];
        for state in animation_states.element_children().filter(|child| {
            source_document_names_are_semantically_equal(child.name.as_str(), "state")
        }) {
            let prefix = required_element(&state, &["StateName"])?;
            let mut limits = [0_i16; 4];
            let mut present_limits = 0_u8;
            for (index, name) in limit_names.iter().enumerate() {
                if let Some(value) = state.attribute_named_any(&[*name]) {
                    limits[index] =
                        parse_blue_fang_source_numeric_lexeme(value).ok_or_else(|| {
                            BindError::record(
                                record,
                                format!("invalid vehicle seat angle {name}: {value}"),
                            )
                        })?;
                    present_limits |= 1 << index;
                }
            }
            resolved_animation_states.push(TransportationVehicleSeatAnimationState {
                clip_prefix: prefix.to_owned(),
                angle_limits_degrees: limits,
                present_limits,
            });
        }
        if resolved_animation_states.is_empty() {
            return Err(BindError::record(
                record,
                "vehicle seat has no authored animation states",
            ));
        }
        let index = u16::try_from(seat_index)
            .map_err(|_| BindError::record(record, "vehicle seat index exceeds u16"))?;
        output
            .document
            .vehicle_seats
            .push(TransportationVehicleSeatDefinition {
                id: id(&format!("{}:seat:{index}", record.key)),
                vehicle: id(record.key),
                index,
                back_facing: element_bool(&seat, &["isBackFacing"], false)?,
                view_limits_degrees: [
                    required_element_number(&view_limit, &["LeftForward"])?,
                    required_element_number(&view_limit, &["LeftBack"])?,
                    required_element_number(&view_limit, &["RightForward"])?,
                    required_element_number(&view_limit, &["RightBack"])?,
                ],
                emote_bands,
                animation_states: resolved_animation_states,
            });
    }
    Ok(())
}

pub(super) fn bind_tour_view(
    record: &RecordView<'_, '_>,
    output: &mut WorldDefinitionLoweringTables,
) -> Result<(), BindError> {
    output.document.tour_views.push(TourViewDefinition {
        id: id(record.key),
        subject: asset(record, &["subject", "object"]),
        radius_cm: required_number(record, &["radiusCm"])?,
        base_score: required_number(record, &["baseScore", "score"])?,
        dwell_ticks: required_number(record, &["dwellTicks"])?,
        occlusion_required: bool_or(record, &["occlusionRequired"], true)?,
    });
    Ok(())
}
