use super::adoption::lower_authored_animal_adoption_count_range;
use super::locomotion::lower_animal_ground_navigation_clip;
use super::locomotion::lower_animal_ground_navigation_clip_for_speed;
use super::locomotion::lower_animal_navigation_collision_policy;
use super::locomotion::lower_authored_initial_animal_ground_animation;
use super::locomotion::lower_first_available_authored_initial_animal_ground_animation;
use super::source_queries::find_first_semantically_named_source_descendant;
use crate::source_document::authored_behavior_type_identifiers::authored_behavior_type_identifiers;
use crate::source_document::blue_fang_actor_manifest_model_and_scene_resolution_index::BlueFangActorManifestModelAndSceneResolutionIndex;
use crate::source_document::resolved_source_record_index::BindError;
use crate::source_document::resolved_source_record_index::RecordView;
use crate::source_document::source_document_semantic_name::canonicalize_source_document_record_key;
use crate::source_document::source_document_semantic_name::source_document_names_are_semantically_equal;
use openzt2_game_data::AssetId;
use openzt2_game_data::species::LifeStage;
use openzt2_game_data::species::Sex;
use openzt2_game_data::species::SpeciesVariant;
use openzt2_game_data::species::SpeciesVariantFlags;

pub(super) fn lower_species_root_actor_to_fallback_canonical_variant(
    species: &RecordView<'_, '_>,
    actor_manifest_resolution_index: &BlueFangActorManifestModelAndSceneResolutionIndex,
) -> Result<Option<SpeciesVariant>, BindError> {
    let Some(actor) = find_first_semantically_named_source_descendant(
        species.source_document_element(),
        "BFActorComponent",
    ) else {
        return Ok(None);
    };
    let actorfile = actor
        .attribute_named_any(&["actorfile"])
        .ok_or_else(|| BindError::record(species, "shared animal actor has no actorfile"))?;
    let actor_path = normalize_authored_actor_manifest_path(actorfile);
    let model = actor_manifest_resolution_index
        .native_model_source_path_for_normalized_actor_manifest_key(&actor_path)
        .map(str::to_owned)
        .ok_or_else(|| {
            BindError::record(species, format!("no resolved model for actor {actor_path}"))
        })?;
    let (initial_animation_clip_asset_key, initial_animation_loops) =
        lower_authored_initial_animal_ground_animation(species)?
            .map_or((None, false), |(animation_clip_asset_key, loops)| {
                (Some(animation_clip_asset_key), loops)
            });
    Ok(Some(SpeciesVariant {
        need_points: lower_need_point_policy(species)?,
        placement_surface_policy: crate::world_definitions::lower::authored_placement_footprint_lowering::authored_surface_policy(species)?,
        id: AssetId::from_key(&canonicalize_source_document_record_key(&format!(
            "{}_adult",
            species.key
        ))),
        behavior_subject_type_identifiers: authored_behavior_type_identifiers(species),
        navigation_collision_policy: lower_animal_navigation_collision_policy(species)?,
        virtual_nodes: crate::world_definitions::lower::object_and_placeable_source_lowering::lower_world_object_virtual_nodes(&species),
        surface_water_locomotion: super::locomotion::lower_animal_surface_water_locomotion(species),
        ground_navigation_clip_asset_key: lower_animal_ground_navigation_clip(species),
        medium_ground_navigation_clip_asset_key: lower_animal_ground_navigation_clip_for_speed(
            species, "medium",
        ),
        fast_ground_navigation_clip_asset_key: lower_animal_ground_navigation_clip_for_speed(
            species, "fast",
        ),
        named_ground_navigation_clip_asset_keys:
            super::locomotion::lower_animal_named_ground_navigation_clips(species),
        sex: Sex::Any,
        life_stage: LifeStage::Adult,
        model,
        model_animation_set: AssetId::from_virtual_path(&actor_path),
        initial_animation_clip_asset_key,
        initial_animation_loops,
        material_variant: None,
        scale: [parse_positive_authored_scale_or_default(actor.attribute_named_any(&["scale"])); 2],
        weight_kg: [0.0; 2],
        probability: 100,
        flags: SpeciesVariantFlags::DEFAULT,
        adoption_count_range: [1, 1],
    }))
}

fn lower_need_point_policy(
    record: &RecordView<'_, '_>,
) -> Result<openzt2_game_data::species::AnimalNeedPointPolicy, BindError> {
    use super::source_queries::find_effective_authored_animal_shared_attribute;
    use crate::source_document::blue_fang_source_numeric_lexeme::parse_blue_fang_source_numeric_lexeme;
    use openzt2_game_data::species::AnimalNeedPointPolicy;

    let default = AnimalNeedPointPolicy::default();
    let number = |name, fallback| {
        let Some(value) = find_effective_authored_animal_shared_attribute(record, name) else {
            return Ok(fallback);
        };
        parse_blue_fang_source_numeric_lexeme::<f32>(value)
            .filter(|value| value.is_finite())
            .ok_or_else(|| BindError::record(record, format!("invalid {name}")))
    };
    Ok(AnimalNeedPointPolicy {
        interval_seconds: number("f_NeedPointUpdateInterval", default.interval_seconds)?,
        maximum: number("f_NeedPointMax", default.maximum)?,
        good_delta: number("f_NeedPointGoodDelta", default.good_delta)?,
        bad_delta: number("f_NeedPointBadDelta", default.bad_delta)?,
        factor: number("f_NeedPointFactor", default.factor)?,
        factor_delta: number("f_NeedPointFactorDelta", default.factor_delta)?,
    })
}

pub(super) fn lower_species_variant_source_record_to_canonical_variant(
    species: &RecordView<'_, '_>,
    candidate: &RecordView<'_, '_>,
    actor_manifest_resolution_index: &BlueFangActorManifestModelAndSceneResolutionIndex,
) -> Result<Option<SpeciesVariant>, BindError> {
    use super::source_queries::{
        authored_optional_boolean_is_true, find_effective_authored_animal_shared_attribute,
    };

    if authored_optional_boolean_is_true(candidate.source_document_element().attribute("abstract"))
    {
        return Ok(None);
    }
    let ancestors = candidate
        .type_tokens()
        .into_iter()
        .rev()
        .filter(|name| !source_document_names_are_semantically_equal(name, candidate.key))
        .filter_map(|name| candidate.find_resolved_source_record_by_reference(&name))
        .collect::<Vec<_>>();
    let effective = |name| find_effective_authored_animal_shared_attribute(candidate, name);
    let sex = effective("b_Male").map_or(Sex::Any, |value| {
        if authored_optional_boolean_is_true(Some(value)) {
            Sex::Male
        } else {
            Sex::Female
        }
    });
    let life_stage = match effective("b_Adult") {
        Some(value) if !authored_optional_boolean_is_true(Some(value)) => LifeStage::Young,
        _ => LifeStage::Adult,
    };
    let stage_record = ancestors
        .iter()
        .find(|record| record.descendant_named("weights").is_some())
        .copied()
        .unwrap_or(*candidate);
    let actors = std::iter::once(candidate)
        .chain(ancestors.iter())
        .chain(std::iter::once(species))
        .filter_map(|record| {
            find_first_semantically_named_source_descendant(
                record.source_document_element(),
                "BFActorComponent",
            )
        });
    let Some(actorfile) = actors
        .clone()
        .find_map(|actor| actor.attribute_named_any(&["actorfile"]))
    else {
        return Ok(None);
    };
    let scale = actors
        .filter_map(|actor| actor.attribute_named_any(&["scale"]))
        .next();
    let actor_path = normalize_authored_actor_manifest_path(actorfile);
    let model = actor_manifest_resolution_index
        .native_model_source_path_for_normalized_actor_manifest_key(&actor_path)
        .map(str::to_owned)
        .ok_or_else(|| {
            BindError::record(
                candidate,
                format!("no resolved model for actor {actor_path}"),
            )
        })?;
    let (initial_animation_clip_asset_key, initial_animation_loops) =
        lower_first_available_authored_initial_animal_ground_animation(
            candidate,
            &stage_record,
            species,
        )?;
    let probability = if ["b_Super", "b_Old", "b_Sickly"]
        .into_iter()
        .any(|flag| authored_optional_boolean_is_true(effective(flag)))
    {
        0
    } else {
        find_authored_numbered_species_variant_probability(&stage_record, candidate.key)
            .or_else(|| find_authored_base_species_variant_probability(&stage_record))
            .unwrap_or(100)
    };
    Ok(Some(SpeciesVariant {
        need_points: lower_need_point_policy(candidate)?,
        placement_surface_policy: crate::world_definitions::lower::authored_placement_footprint_lowering::authored_surface_policy(candidate)?,
        id: AssetId::from_key(&canonicalize_source_document_record_key(candidate.key)),
        behavior_subject_type_identifiers: authored_behavior_type_identifiers(candidate),
        navigation_collision_policy: lower_animal_navigation_collision_policy(candidate)?,
        virtual_nodes: crate::world_definitions::lower::object_and_placeable_source_lowering::lower_world_object_virtual_nodes(&candidate),
        surface_water_locomotion: super::locomotion::lower_animal_surface_water_locomotion(
            candidate,
        ),
        ground_navigation_clip_asset_key: lower_animal_ground_navigation_clip(candidate),
        medium_ground_navigation_clip_asset_key: lower_animal_ground_navigation_clip_for_speed(
            candidate, "medium",
        ),
        fast_ground_navigation_clip_asset_key: lower_animal_ground_navigation_clip_for_speed(
            candidate, "fast",
        ),
        named_ground_navigation_clip_asset_keys:
            super::locomotion::lower_animal_named_ground_navigation_clips(candidate),
        sex,
        life_stage,
        model,
        model_animation_set: AssetId::from_virtual_path(&actor_path),
        initial_animation_clip_asset_key,
        initial_animation_loops,
        material_variant: None,
        scale: [parse_positive_authored_scale_or_default(scale); 2],
        weight_kg: [0.0; 2],
        probability,
        flags: if candidate.key.ends_with('2') {
            SpeciesVariantFlags::RARE
        } else {
            SpeciesVariantFlags::DEFAULT
        } | if super::source_queries::authored_optional_boolean_is_true(
            super::source_queries::find_effective_authored_animal_shared_attribute(
                candidate,
                "b_Rampager",
            ),
        ) {
            SpeciesVariantFlags::RAMPAGER
        } else {
            SpeciesVariantFlags::DEFAULT
        },
        adoption_count_range: lower_authored_animal_adoption_count_range(candidate, sex)?,
    }))
}

fn find_authored_numbered_species_variant_probability(
    stage: &RecordView<'_, '_>,
    key: &str,
) -> Option<u16> {
    let extension = key
        .chars()
        .rev()
        .take_while(char::is_ascii_digit)
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>();
    (!extension.is_empty())
        .then_some(extension)
        .and_then(|extension| {
            stage
                .descendant_named("weights")?
                .element_children()
                .find_map(|row| {
                    (source_document_names_are_semantically_equal(row.name.as_str(), "variant")
                        && row
                            .attribute_named_any(&["extension"])
                            .is_some_and(|value| value.trim() == extension))
                    .then(|| row.attribute_named_any(&["weight"])?.parse().ok())
                    .flatten()
                })
        })
}

fn find_authored_base_species_variant_probability(stage: &RecordView<'_, '_>) -> Option<u16> {
    stage
        .descendant_named("weights")?
        .element_children()
        .find_map(|row| {
            source_document_names_are_semantically_equal(row.name.as_str(), "base")
                .then(|| row.attribute_named_any(&["weight"])?.parse().ok())
                .flatten()
        })
}

fn parse_positive_authored_scale_or_default(value: Option<&str>) -> f32 {
    value
        .and_then(|value| value.parse().ok())
        .filter(|value: &f32| value.is_finite() && *value > 0.0)
        .unwrap_or(1.0)
}

fn normalize_authored_actor_manifest_path(value: &str) -> String {
    value
        .trim()
        .replace('\\', "/")
        .trim_start_matches('/')
        .to_ascii_lowercase()
}
