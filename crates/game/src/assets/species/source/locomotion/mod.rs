use super::source_queries::find_all_semantically_named_source_descendants;
use super::source_queries::find_first_semantically_named_source_descendant;
use crate::assets::source_document::blue_fang_source_numeric_lexeme::parse_blue_fang_source_numeric_lexeme;
use crate::assets::source_document::resolved_source_record_index::BindError;
use crate::assets::source_document::resolved_source_record_index::RecordView;
use crate::assets::source_document::source_document_semantic_name::source_document_names_are_semantically_equal;

pub(super) fn lower_animal_ground_navigation_clip(record: &RecordView<'_, '_>) -> Option<String> {
    let ancestry = record.type_tokens();
    let mut clip = None;
    let mut has_inherited_modulation = false;
    for member in ancestry
        .iter()
        .filter(|name| !source_document_names_are_semantically_equal(name, record.key))
        .filter_map(|name| record.find_resolved_source_record_by_reference(name))
        .chain(std::iter::once(*record))
    {
        for binder in find_all_semantically_named_source_descendants(
            member.source_document_element(),
            "BFNamedBinder",
        ) {
            if binder.attribute_named_any(&["binderName"]) != Some("ground") {
                continue;
            }
            let Some(loco) =
                find_first_semantically_named_source_descendant(binder, "BFLocoAnimate")
            else {
                continue;
            };
            let Some(slow) = loco
                .element_children()
                .find(|child| child.name.as_str() == "slow")
            else {
                continue;
            };
            // Variable/random playback-rate policies need their own sampled
            // lifetime. This initial integration accepts the unmodulated row.
            has_inherited_modulation |= slow
                .attribute_named_any(&[
                    "minAnimSpeed",
                    "maxAnimSpeed",
                    "animSpeed",
                    "flapAnim",
                    "resetPeriod",
                    "minResetPeriod",
                    "maxResetPeriod",
                ])
                .is_some()
                || slow.element_children().next().is_some();
            if let Some(name) = slow
                .attribute_named_any(&["name"])
                .filter(|name| !name.trim().is_empty())
            {
                clip = Some(name.to_owned());
            }
        }
    }
    (!has_inherited_modulation).then_some(clip).flatten()
}

pub(super) fn lower_animal_navigation_collision_policy(
    record: &RecordView<'_, '_>,
) -> Result<Option<openzt2_game_data::species::AnimalNavigationCollisionPolicy>, BindError> {
    let ancestry = record.type_tokens();
    let family = ancestry
        .iter()
        .filter(|name| !source_document_names_are_semantically_equal(name, record.key))
        .filter_map(|name| record.find_resolved_source_record_by_reference(name))
        .chain(std::iter::once(*record));
    let mut policy = None;
    for member in family {
        for tester in find_all_semantically_named_source_descendants(
            member.source_document_element(),
            "BFGCollisionTester",
        ) {
            let inherited = policy.get_or_insert_with(
                openzt2_game_data::species::AnimalNavigationCollisionPolicy::default,
            );
            for (name, destination) in [
                ("radius", &mut inherited.radius_m),
                ("height", &mut inherited.height_m),
                ("depth", &mut inherited.depth_m),
                ("maxSlope", &mut inherited.maximum_slope_radians),
                ("escapeBuffer", &mut inherited.escape_buffer_m),
                ("wadeDepth", &mut inherited.wade_depth_m),
            ] {
                if let Some(value) = tester.attribute_named_any(&[name]) {
                    *destination = parse_blue_fang_source_numeric_lexeme::<f32>(value)
                        .filter(|value| value.is_finite() && (name != "radius" || *value >= 0.0))
                        .ok_or_else(|| {
                            BindError::record(record, format!("invalid animal collision {name}"))
                        })?;
                }
            }
            for (name, destination) in [
                ("defaultTriScore", &mut inherited.default_triangle_score),
                ("waterScore", &mut inherited.water_score),
                ("landScore", &mut inherited.land_score),
            ] {
                if let Some(value) = tester.attribute_named_any(&[name]) {
                    *destination =
                        parse_blue_fang_source_numeric_lexeme::<i32>(value).ok_or_else(|| {
                            BindError::record(record, format!("invalid animal collision {name}"))
                        })?;
                }
            }
            for (name, destination) in [
                ("useFastPathing", &mut inherited.use_fast_pathing),
                ("canSwimNoEntity", &mut inherited.can_swim_without_entity),
                ("canSwimUnderwater", &mut inherited.can_swim_underwater),
                ("canUseWaterFreely", &mut inherited.can_use_water_freely),
            ] {
                if let Some(value) = tester.attribute_named_any(&[name]) {
                    *destination = super::source_queries::authored_boolean_is_true(value);
                }
            }
            for (name, destination) in [
                ("motionClass", &mut inherited.motion_class),
                ("requiredType", &mut inherited.required_type),
                ("ignoredType", &mut inherited.ignored_type),
            ] {
                if let Some(value) = tester.attribute_named_any(&[name]) {
                    *destination = (!value.trim().is_empty())
                        .then(|| openzt2_game_data::AssetId::from_key(&value.to_ascii_lowercase()));
                }
            }
        }
    }
    Ok(policy)
}

pub(super) fn lower_authored_initial_animal_ground_animation(
    animal: &RecordView<'_, '_>,
) -> Result<Option<(String, bool)>, BindError> {
    let (animation_clip_asset_key, authored_loop_flag) = if let Some(locomotion_switch_set) =
        animal.descendant_named("BFBehLocoSwitchSet")
    {
        let ground_behavior =
            find_first_semantically_named_source_descendant(locomotion_switch_set, "behaviorTable")
                .and_then(|behavior_table| {
                    find_first_semantically_named_source_descendant(behavior_table, "ground")
                })
                .ok_or_else(|| {
                    BindError::record(
                        animal,
                        "authored animal locomotion switch has no ground behavior",
                    )
                })?;
        (
            ground_behavior
                .attribute_named_any(&["behSet"])
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| {
                    BindError::record(animal, "authored animal ground behavior has no behSet")
                })?,
            locomotion_switch_set.attribute_named_any(&["loopFlag"]),
        )
    } else if let Some(initial_animation) = animal
        .descendant_named("BFBehaviorMgr")
        .and_then(|behavior_manager| {
            find_first_semantically_named_source_descendant(behavior_manager, "subBehaviors")
        })
        .and_then(|sub_behaviors| {
            find_first_semantically_named_source_descendant(sub_behaviors, "BFBehAnimate")
        })
    {
        (
            initial_animation
                .attribute_named_any(&["targetAnim"])
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| {
                    BindError::record(
                        animal,
                        "authored initial animal animation has no targetAnim",
                    )
                })?,
            initial_animation.attribute_named_any(&["loopFlag"]),
        )
    } else {
        return Ok(None);
    };
    let initial_animation_loops = authored_loop_flag
        .map(|value| match value.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" => Ok(true),
            "0" | "false" | "no" => Ok(false),
            _ => Err(BindError::record(
                animal,
                format!("authored animal locomotion loopFlag has invalid value {value}"),
            )),
        })
        .transpose()?
        .unwrap_or(false);
    Ok(Some((
        animation_clip_asset_key.to_owned(),
        initial_animation_loops,
    )))
}

pub(super) fn lower_first_available_authored_initial_animal_ground_animation(
    concrete_variant: &RecordView<'_, '_>,
    life_stage: &RecordView<'_, '_>,
    species: &RecordView<'_, '_>,
) -> Result<(Option<String>, bool), BindError> {
    for record in [concrete_variant, life_stage, species] {
        if let Some((animation_clip_asset_key, loops)) =
            lower_authored_initial_animal_ground_animation(record)?
        {
            return Ok((Some(animation_clip_asset_key), loops));
        }
    }
    Ok((None, false))
}

pub(super) fn lower_authored_species_ground_movement_speed_metres_per_second(
    species: &RecordView<'_, '_>,
) -> f32 {
    species
        .descendant_named("BFLocoAnimate")
        .and_then(|loco| {
            loco.element_children().find(|row| {
                source_document_names_are_semantically_equal(row.name.as_str(), "medium")
            })
        })
        .and_then(|row| row.attribute_named_any(&["minAnimSpeed"]))
        .and_then(|value| value.parse().ok())
        .filter(|value: &f32| value.is_finite() && *value >= 0.0)
        .unwrap_or(0.0)
}

pub(super) fn lower_authored_species_swimming_speed_metres_per_second(
    species: &RecordView<'_, '_>,
) -> f32 {
    species
        .descendant_named("BFSwimComponent")
        .map_or(0.0, |_| {
            lower_authored_species_ground_movement_speed_metres_per_second(species)
        })
}
