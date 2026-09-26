use super::immersive_mode_policy_source_vocabulary::{
    immersive_mode_action, immersive_mode_kind, immersive_mode_lifecycle_flag,
};
use super::source_element_tree_search::find_descendant;
use super::world_definition_lowering_tables::WorldDefinitionLoweringTables;
use super::world_definition_source_value_reading_and_conversion::{
    array_required, asset, bool_or, flags, id, number_or, required, required_bool, required_number,
};
use crate::assets::source_coordinate_conversion::convert_source_z_up_vector_to_bevy_y_up_coordinates;
use crate::assets::source_document::blue_fang_source_numeric_lexeme::parse_blue_fang_source_numeric_lexeme;
use crate::assets::source_document::ordered_source_document_types::OrderedSourceDocumentNode;
use crate::assets::source_document::resolved_source_record_index::{BindError, RecordView};
use crate::assets::source_document::source_document_semantic_name::canonicalize_source_document_record_key;
use openzt2_game_data::world_definitions::camera_definitions::{
    CameraProjectionKind, CameraTuningDefinition,
};
use openzt2_game_data::world_definitions::immersive_mode_policy::{
    ImmersiveModeActionFlags, ImmersiveModeKind, ImmersiveModePolicy,
};
use openzt2_game_data::AssetId;

pub(super) fn bind_immersive_mode_policy(
    record: &RecordView<'_, '_>,
    output: &mut WorldDefinitionLoweringTables,
) -> Result<(), BindError> {
    if matches!(
        canonicalize_source_document_record_key(&record.semantic_type()).as_str(),
        "ztphotomode" | "ztsuperstaffmode"
    ) {
        return bind_nested_authored_first_person_child_policy(record, output);
    }

    const PAUSE_SIMULATION: u8 = 1 << 0;
    const HIDE_HUD: u8 = 1 << 1;
    const LOCK_SUBJECT: u8 = 1 << 2;
    const RETURN_CAMERA_ON_EXIT: u8 = 1 << 3;

    let lifecycle_flags = flags(
        record.value(&["flags", "modeFlags"]),
        immersive_mode_lifecycle_flag,
    )? as u8;
    output
        .document
        .immersive_mode_policies
        .push(ImmersiveModePolicy {
            id: id(record.key),
            mode: immersive_mode_kind(required(record, &["mode", "modeKind"])?)?,
            allowed_actions: ImmersiveModeActionFlags::from_raw_flag_bits(flags(
                record.value(&["allowedActions", "actions"]),
                immersive_mode_action,
            )? as u32)
            .ok_or_else(|| {
                BindError::record(record, "immersive-mode actions contain unknown flag bits")
            })?,
            interaction_cursor: asset(record, &["cursor"]),
            interaction_prefab: asset(record, &["prefab", "toolPrefab"]),
            camera: asset(record, &["camera"]),
            restore_camera_on_exit: lifecycle_flags & RETURN_CAMERA_ON_EXIT != 0,
            pause_simulation: lifecycle_flags & PAUSE_SIMULATION != 0,
            hide_hud: lifecycle_flags & HIDE_HUD != 0,
            lock_subject: lifecycle_flags & LOCK_SUBJECT != 0,
            last_captured_photo_preview_frame_count: 0,
            photo_zoom_step: 0.0,
        });
    Ok(())
}

fn bind_nested_authored_first_person_child_policy(
    record: &RecordView<'_, '_>,
    output: &mut WorldDefinitionLoweringTables,
) -> Result<(), BindError> {
    let first_person_parent_mode = record
        .find_resolved_source_record_by_reference("mode_first_person")
        .ok_or_else(|| {
            BindError::record(
                record,
                "first-person child mode has no ZTFirstPersonMode source",
            )
        })?;
    let photo = canonicalize_source_document_record_key(&record.semantic_type()) == "ztphotomode";
    let allowed_actions = if photo {
        ImmersiveModeActionFlags::LOOK
            .with_additional_flags(ImmersiveModeActionFlags::ZOOM)
            .with_additional_flags(ImmersiveModeActionFlags::CONFIRM)
            .with_additional_flags(ImmersiveModeActionFlags::CANCEL)
    } else {
        ImmersiveModeActionFlags::LOOK
            .with_additional_flags(ImmersiveModeActionFlags::MOVE_FORWARD)
            .with_additional_flags(ImmersiveModeActionFlags::MOVE_BACK)
            .with_additional_flags(ImmersiveModeActionFlags::STRAFE_LEFT)
            .with_additional_flags(ImmersiveModeActionFlags::STRAFE_RIGHT)
            .with_additional_flags(ImmersiveModeActionFlags::PRIMARY)
            .with_additional_flags(ImmersiveModeActionFlags::CANCEL)
    };
    output
        .document
        .immersive_mode_policies
        .push(ImmersiveModePolicy {
            id: id(record.key),
            mode: if photo {
                ImmersiveModeKind::Photo
            } else {
                ImmersiveModeKind::SuperStaff
            },
            allowed_actions,
            // These children present their tools through their authored layouts;
            // neither names a world cursor or interaction prefab.
            interaction_cursor: AssetId::default(),
            interaction_prefab: AssetId::default(),
            camera: asset(&first_person_parent_mode, &["camera"]),
            // Both children share the first-person camera parent. The overhead
            // camera-set toggle returns to the previously captured overhead pose.
            restore_camera_on_exit: true,
            pause_simulation: false,
            hide_hud: photo,
            lock_subject: false,
            last_captured_photo_preview_frame_count: if photo {
                required_number(record, &["lastPicWaitTime"])?
            } else {
                0
            },
            photo_zoom_step: if photo {
                required_number(record, &["zoomSpeed"])?
            } else {
                0.0
            },
        });
    Ok(())
}

pub(super) fn bind_camera(
    record: &RecordView<'_, '_>,
    output: &mut WorldDefinitionLoweringTables,
) -> Result<(), BindError> {
    if canonicalize_source_document_record_key(&record.semantic_type()) == "bfphysobj" {
        return if record
            .descendant_named("BFOverheadCameraComponent")
            .is_some()
        {
            bind_overhead_camera(record, output)
        } else {
            bind_perspective_camera(record, output)
        };
    }
    output.document.cameras.push(CameraTuningDefinition {
        id: id(record.key),
        projection: match canonicalize_source_document_record_key(required(
            record,
            &["projection"],
        )?)
        .as_str()
        {
            "perspective" => CameraProjectionKind::Perspective,
            "orthographic" => CameraProjectionKind::Orthographic,
            value => {
                return Err(BindError::record(
                    record,
                    format!("unknown camera projection {value}"),
                ));
            }
        },
        fov_y_radians: required_number(record, &["fovYRadians", "fov"])?,
        fixed_aspect_ratio: if canonicalize_source_document_record_key(required(
            record,
            &["projection"],
        )?)
        .as_str()
            == "perspective"
        {
            number_or(record, &["fixedAspectRatio", "aspectRatio"], 4.0 / 3.0)?
        } else {
            0.0
        },
        vertical_view_per_zoom: if canonicalize_source_document_record_key(required(
            record,
            &["projection"],
        )?)
        .as_str()
            == "orthographic"
        {
            number_or(record, &["verticalViewPerZoom"], 1.0)?
        } else {
            0.0
        },
        near_m: required_number(record, &["nearM", "near"])?,
        far_m: required_number(record, &["farM", "far"])?,
        offset_m: convert_source_z_up_vector_to_bevy_y_up_coordinates(array_required(
            record,
            &["offsetX", "offsetY", "offsetZ"],
        )?),
        pitch_radians: [
            required_number(record, &["minimumPitchRadians", "minPitch"])?,
            required_number(record, &["maximumPitchRadians", "maxPitch"])?,
        ],
        yaw_radians: [
            required_number(record, &["minimumYawRadians", "minYaw"])?,
            required_number(record, &["maximumYawRadians", "maxYaw"])?,
        ],
        minimum_zoom_m: required_number(record, &["minimumZoomM", "minZoom"])?,
        maximum_zoom_m: [required_number(record, &["maximumZoomM", "maxZoom"])?; 3],
        collision_radius_cm: required_number(record, &["collisionRadiusCm"])?,
        initial_pitch_radians: required_number(
            record,
            &["initialPitchRadians", "pitchRotate", "pitch"],
        )?,
        initial_zoom_m: required_number(record, &["initialZoomM", "currZoom", "initialZoom"])?,
        look_at_distance_m: number_or(record, &["lookAtDistanceM", "lookAtDist"], 0.0)?,
        parenting_offset_m: number_or(record, &["parentingOffsetM", "parentingOffset"], 0.0)?,
        pan_speed_mps: required_number(record, &["panSpeedMps", "moveSpeed"])?,
        turn_speed_rps: required_number(record, &["turnSpeedRps", "turnSpeed"])?,
        zoom_speed_mps: required_number(record, &["zoomSpeedMps", "zoomSpeed"])?,
        pan_start_rate: required_number(record, &["panStartRate", "startAccel"])?,
        pan_stop_rate: required_number(record, &["panStopRate", "stopAccel"])?,
        ground_buffer_m: required_number(record, &["groundBufferM", "groundBufferZone"])?,
        soft_fit_distance_m: number_or(record, &["softFitDistanceM", "softFitDistance"], 0.0)?,
        soft_fit_speed_mps: number_or(record, &["softFitSpeedMps", "softFitSpeed"], 0.0)?,
        no_tilt_on_fit: bool_or(record, &["noTiltOnFit"], true)?,
        edge_scroll: required_bool(record, &["edgeScroll"])?,
        zoom_speed_samples: [[0.0; 2]; 8],
        zoom_speed_sample_count: 0,
        sub_zero_zoom_multiplier: 1.0,
        ground_acceleration_mps2: 0.0,
    });
    Ok(())
}

fn bind_perspective_camera(
    record: &RecordView<'_, '_>,
    output: &mut WorldDefinitionLoweringTables,
) -> Result<(), BindError> {
    let root = record.source_document_element();
    let first_person_camera = find_descendant(root, "BFFPSCameraComponent");
    let camera = first_person_camera
        .or_else(|| find_descendant(root, "BFCameraComponent"))
        .ok_or_else(|| BindError::record(record, "camera has no perspective component"))?;
    let first_person_controls = find_descendant(root, "ZTFPSUDVComponent");
    let controls =
        first_person_controls.or_else(|| find_descendant(root, "BFUserDrivenVehicleComponent"));
    let frustum = find_descendant(camera, "frustum");
    let ground_fit = find_descendant(root, "ZTFPSCameraGroundFitComponent")
        .or_else(|| find_descendant(root, "BFAboveGroundFitComponent"));
    let number_or = |element: Option<&OrderedSourceDocumentNode>, name: &str, fallback: f32| {
        let Some(value) = element.and_then(|element| element.attribute_named_any(&[name])) else {
            return Ok(fallback);
        };
        parse_blue_fang_source_numeric_lexeme::<f32>(value.trim_end_matches(['f', 'F']))
            .filter(|value| value.is_finite())
            .ok_or_else(|| BindError::record(record, format!("camera has invalid numeric {name}")))
    };
    // Camera components can exist without a vehicle/input component.
    let initial_zoom = number_or(Some(camera), "zoom", 1.0)?;
    let (minimum_zoom, maximum_zoom) = if let Some(controls) = first_person_controls {
        let required_control_number = |name: &str| {
            controls
                .attribute_named_any(&[name])
                .and_then(|value| {
                    parse_blue_fang_source_numeric_lexeme::<f32>(value.trim_end_matches(['f', 'F']))
                })
                .filter(|value| value.is_finite())
                .ok_or_else(|| {
                    BindError::record(
                        record,
                        format!("first-person input is missing numeric {name}"),
                    )
                })
        };
        (
            required_control_number("minZoom")?,
            required_control_number("maxZoom")?,
        )
    } else {
        (initial_zoom, initial_zoom)
    };
    let initial_pitch = number_or(Some(camera), "pitchRotate", 0.0)?;
    let pitch_radians = if first_person_camera.is_some() {
        let minimum = number_or(Some(camera), "minPitch", 0.0)?;
        let maximum = number_or(Some(camera), "maxPitch", -1.0)?;
        // Pitch is capped at the maximum, then raised to the minimum.
        [minimum, maximum.max(minimum)]
    } else {
        [initial_pitch; 2]
    };
    let stand_height = number_or(first_person_camera, "standHeight", 1.8)?;
    let maximum_view_distances = [
        number_or(Some(camera), "minMaxViewDist", 800.0)?,
        number_or(Some(camera), "medMaxViewDist", 800.0)?,
        number_or(Some(camera), "maxMaxViewDist", 800.0)?,
    ];
    let left = number_or(frustum, "left", -0.7315)?;
    let right = number_or(frustum, "right", 0.7315)?;
    let up = number_or(frustum, "up", 0.55)?;
    let down = number_or(frustum, "down", -0.55)?;
    let horizontal_span = right - left;
    let vertical_span = up - down;
    if horizontal_span <= 0.0 || vertical_span <= 0.0 {
        return Err(BindError::record(
            record,
            "perspective camera frustum is invalid",
        ));
    }
    let collision_radius_cm = ground_fit
        .and_then(|component| component.attribute_named_any(&["fitRadius"]))
        .and_then(|value| {
            parse_blue_fang_source_numeric_lexeme::<f32>(value.trim_end_matches(['f', 'F']))
        })
        .map(|metres| (metres * 100.0).round() as u16)
        .unwrap_or(0);
    output.document.cameras.push(CameraTuningDefinition {
        id: id(record.key),
        projection: CameraProjectionKind::Perspective,
        // BFCameraComponent's constructor supplies these 4:3
        // frustum slopes when the source omits them. The widescreen override
        // authors its wider left/right slopes explicitly.
        fov_y_radians: 2.0 * (vertical_span * 0.5).atan(),
        fixed_aspect_ratio: horizontal_span / vertical_span,
        vertical_view_per_zoom: 0.0,
        near_m: number_or(frustum, "near", 0.2)?,
        far_m: maximum_view_distances[2],
        offset_m: [
            0.0,
            if first_person_camera.is_some() {
                stand_height
            } else {
                0.0
            },
            0.0,
        ],
        pitch_radians,
        yaw_radians: [-std::f32::consts::PI, std::f32::consts::PI],
        minimum_zoom_m: minimum_zoom,
        maximum_zoom_m: [maximum_zoom; 3],
        collision_radius_cm,
        initial_pitch_radians: initial_pitch,
        initial_zoom_m: initial_zoom,
        look_at_distance_m: 0.0,
        parenting_offset_m: 0.0,
        pan_speed_mps: if controls.is_some() {
            number_or(controls, "moveSpeed", 10.0)?
        } else {
            0.0
        },
        turn_speed_rps: if controls.is_some() {
            number_or(controls, "turnSpeed", 1.0)?
        } else {
            0.0
        },
        zoom_speed_mps: maximum_zoom - minimum_zoom,
        pan_start_rate: 1.0,
        pan_stop_rate: -1.0,
        ground_buffer_m: 0.0,
        soft_fit_distance_m: 0.0,
        soft_fit_speed_mps: 0.0,
        no_tilt_on_fit: true,
        edge_scroll: false,
        zoom_speed_samples: [[0.0; 2]; 8],
        zoom_speed_sample_count: 0,
        sub_zero_zoom_multiplier: 1.0,
        ground_acceleration_mps2: number_or(ground_fit, "landMaxAccel", 0.0)?,
    });
    Ok(())
}

pub(super) fn bind_overhead_camera(
    record: &RecordView<'_, '_>,
    output: &mut WorldDefinitionLoweringTables,
) -> Result<(), BindError> {
    let root = record.source_document_element();
    let camera = find_descendant(root, "BFOverheadCameraComponent")
        .ok_or_else(|| BindError::record(record, "overhead camera has no camera component"))?;
    let controls = find_descendant(root, "BFOverheadCameraUDVComponent")
        .ok_or_else(|| BindError::record(record, "overhead camera has no input component"))?;
    let frustum = find_descendant(camera, "frustum")
        .ok_or_else(|| BindError::record(record, "overhead camera has no authored frustum"))?;
    let bounds = find_descendant(root, "BFBoundedObjectComponent");
    let number = |element: &'_ OrderedSourceDocumentNode, name: &str| -> Result<f32, BindError> {
        element
            .attribute_named_any(&[name])
            .and_then(|value| {
                parse_blue_fang_source_numeric_lexeme(value.trim_end_matches(['f', 'F']))
            })
            .ok_or_else(|| {
                BindError::record(record, format!("overhead camera is missing numeric {name}"))
            })
    };
    let near = number(frustum, "near")?;
    let left = number(frustum, "left")?;
    let right = number(frustum, "right")?;
    let up = number(frustum, "up")?;
    let down = number(frustum, "down")?;
    let horizontal_span = right - left;
    let vertical_span = up - down;
    if near <= 0.0 || horizontal_span <= 0.0 || vertical_span <= 0.0 {
        return Err(BindError::record(
            record,
            "overhead camera frustum is invalid",
        ));
    }
    let pitch = number(camera, "pitchRotate")?;
    let move_speed = number(controls, "moveSpeed")?;
    let mut zoom_speed_samples = [[0.0; 2]; 8];
    let mut zoom_speed_sample_count = 0_u8;
    if let Some(curve) = find_descendant(controls, "zoomSpeedMod") {
        for modifier in curve
            .element_children()
            .filter(|child| canonicalize_source_document_record_key(child.name.as_str()) == "mod")
        {
            let index = usize::from(zoom_speed_sample_count);
            if index == zoom_speed_samples.len() {
                return Err(BindError::record(
                    record,
                    "overhead camera zoom-speed curve exceeds 8 samples",
                ));
            }
            zoom_speed_samples[index] = [number(modifier, "zoom")?, number(modifier, "speed")?];
            zoom_speed_sample_count += 1;
        }
    }
    output.document.cameras.push(CameraTuningDefinition {
        id: id(record.key),
        projection: CameraProjectionKind::Perspective,
        // Gamebryo stores perspective frustum sides as unit-depth slopes. Its
        // projection matrix uses `2 / (up - down)` directly; `near` controls
        // clipping only. The source keeps this frustum fixed and implements
        // overhead zoom by translating the camera node along its local Y.
        fov_y_radians: 2.0 * (vertical_span * 0.5).atan(),
        fixed_aspect_ratio: horizontal_span / vertical_span,
        vertical_view_per_zoom: 0.0,
        near_m: near,
        // The source camera delegates its far plane to the maximum-view
        // quality setting; the shipped maximum tier is 800 metres.
        far_m: 800.0,
        offset_m: [0.0, number(camera, "lookAtHeight")?, 0.0],
        pitch_radians: [pitch, pitch],
        yaw_radians: [-std::f32::consts::PI, std::f32::consts::PI],
        minimum_zoom_m: number(camera, "minZoom")?,
        maximum_zoom_m: [
            number(camera, "minMaxZoom")?,
            number(camera, "medMaxZoom")?,
            number(camera, "maxMaxZoom")?,
        ],
        collision_radius_cm: bounds
            .and_then(|value| value.attribute_named_any(&["boundOffset"]))
            .and_then(|value| {
                parse_blue_fang_source_numeric_lexeme::<f32>(value.trim_end_matches(['f', 'F']))
            })
            .map(|metres| (metres * 100.0).round() as u16)
            .unwrap_or(0),
        initial_pitch_radians: pitch,
        initial_zoom_m: number(camera, "currZoom")?,
        look_at_distance_m: number(camera, "lookAtDist")?,
        parenting_offset_m: number(camera, "parentingOffset")?,
        pan_speed_mps: move_speed,
        turn_speed_rps: number(controls, "turnSpeed")?,
        // The UDV applies zoom deltas through its move-speed field.
        zoom_speed_mps: move_speed,
        pan_start_rate: number(controls, "startAccel")?,
        pan_stop_rate: number(controls, "stopAccel")?,
        ground_buffer_m: number(camera, "groundBufferZone")?,
        soft_fit_distance_m: number(camera, "softFitDistance")?,
        soft_fit_speed_mps: number(camera, "softFitSpeed")?,
        no_tilt_on_fit: camera
            .attribute_named_any(&["noTiltOnFit"])
            .is_some_and(|value| matches!(value.trim(), "1" | "true" | "TRUE")),
        edge_scroll: true,
        zoom_speed_samples,
        zoom_speed_sample_count,
        sub_zero_zoom_multiplier: number(camera, "subZeroZoomMultiplier")?,
        ground_acceleration_mps2: 0.0,
    });
    Ok(())
}
