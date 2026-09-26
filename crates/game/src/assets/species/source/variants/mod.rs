use super::adoption::lower_authored_animal_adoption_count_range;
use super::locomotion::lower_animal_ground_navigation_clip;
use super::locomotion::lower_animal_navigation_collision_policy;
use super::locomotion::lower_authored_initial_animal_ground_animation;
use super::locomotion::lower_first_available_authored_initial_animal_ground_animation;
use super::source_queries::authored_boolean_is_true;
use super::source_queries::find_all_semantically_named_source_descendants;
use super::source_queries::find_first_semantically_named_source_descendant;
use crate::assets::source_document::blue_fang_actor_manifest_model_and_scene_resolution_index::BlueFangActorManifestModelAndSceneResolutionIndex;
use crate::assets::source_document::resolved_source_record_index::BindError;
use crate::assets::source_document::resolved_source_record_index::RecordView;
use crate::assets::source_document::source_document_semantic_name::canonicalize_source_document_record_key;
use crate::assets::source_document::source_document_semantic_name::source_document_names_are_semantically_equal;
use openzt2_game_data::species::LifeStage;
use openzt2_game_data::species::Sex;
use openzt2_game_data::species::SpeciesVariant;
use openzt2_game_data::species::SpeciesVariantFlags;
use openzt2_game_data::AssetId;
use std::collections::BTreeSet;

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
        id: AssetId::from_key(&canonicalize_source_document_record_key(&format!(
            "{}_adult",
            species.key
        ))),
        behavior_subject_type_identifiers: lower_animal_behavior_subject_types(species),
        navigation_collision_policy: lower_animal_navigation_collision_policy(species)?,
        ground_navigation_clip_asset_key: lower_animal_ground_navigation_clip(species),
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

pub(super) fn lower_species_variant_source_record_to_canonical_variant(
    species: &RecordView<'_, '_>,
    candidate: &RecordView<'_, '_>,
    actor_manifest_resolution_index: &BlueFangActorManifestModelAndSceneResolutionIndex,
) -> Result<Option<SpeciesVariant>, BindError> {
    if candidate.key == species.key {
        return Ok(None);
    }
    let Some(shared) = find_first_semantically_named_source_descendant(
        candidate.source_document_element(),
        "BFAIEntityDataShared",
    ) else {
        return Ok(None);
    };
    let candidate_key = candidate.key.to_ascii_lowercase();
    let sex = shared
        .attribute_named_any(&["b_Male"])
        .and_then(|value| match value.trim().to_ascii_lowercase().as_str() {
            "0" | "false" => Some(Sex::Female),
            "1" | "true" => Some(Sex::Male),
            _ => None,
        })
        .or_else(|| candidate_key.contains("_adult_f").then_some(Sex::Female))
        .or_else(|| candidate_key.contains("_young_f").then_some(Sex::Female))
        .or_else(|| candidate_key.contains("_adult_m").then_some(Sex::Male))
        .or_else(|| candidate_key.contains("_young_m").then_some(Sex::Male));
    let Some(sex) = sex else {
        return Ok(None);
    };
    let tokens = candidate.type_tokens();
    let belongs_to_species = shared.attribute_named_any(&["s_Species"]).map_or_else(
        || {
            tokens
                .iter()
                .any(|token| token.eq_ignore_ascii_case(species.key))
        },
        |value| source_document_names_are_semantically_equal(value, species.key),
    );
    if !belongs_to_species || candidate_key.contains("_super") || candidate_key.contains("_old") {
        return Ok(None);
    }
    let stage_record = tokens
        .iter()
        .rev()
        .find(|token| {
            token.to_ascii_lowercase().ends_with("_adult")
                || token.to_ascii_lowercase().ends_with("_young")
        })
        .and_then(|stage| candidate.find_resolved_source_record_by_reference(stage))
        .or_else(|| candidate.find_resolved_source_record_by_reference(candidate.key))
        .unwrap_or(*candidate);
    let life_stage = if candidate_key.contains("_adult")
        || stage_record.key.to_ascii_lowercase().ends_with("_adult")
    {
        LifeStage::Adult
    } else if candidate_key.contains("_young")
        || stage_record.key.to_ascii_lowercase().ends_with("_young")
    {
        LifeStage::Young
    } else {
        return Ok(None);
    };
    let candidate_actor = find_first_semantically_named_source_descendant(
        candidate.source_document_element(),
        "BFActorComponent",
    );
    let stage_actor = find_first_semantically_named_source_descendant(
        stage_record.source_document_element(),
        "BFActorComponent",
    );
    let species_actor = find_first_semantically_named_source_descendant(
        species.source_document_element(),
        "BFActorComponent",
    );
    let actor = candidate_actor.or(stage_actor).or(species_actor);
    let Some(actorfile) = actor.and_then(|actor| actor.attribute_named_any(&["actorfile"])) else {
        return Ok(None);
    };
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
    let probability =
        find_authored_numbered_species_variant_probability(&stage_record, candidate.key)
            .or_else(|| find_authored_base_species_variant_probability(&stage_record))
            .unwrap_or(100);
    Ok(Some(SpeciesVariant {
        id: AssetId::from_key(&canonicalize_source_document_record_key(candidate.key)),
        behavior_subject_type_identifiers: lower_animal_behavior_subject_types(candidate),
        navigation_collision_policy: lower_animal_navigation_collision_policy(candidate)?,
        ground_navigation_clip_asset_key: lower_animal_ground_navigation_clip(candidate),
        sex,
        life_stage,
        model,
        model_animation_set: AssetId::from_virtual_path(&actor_path),
        initial_animation_clip_asset_key,
        initial_animation_loops,
        material_variant: None,
        scale: [parse_positive_authored_scale_or_default(
            actor.and_then(|actor| actor.attribute_named_any(&["scale"])),
        ); 2],
        weight_kg: [0.0; 2],
        probability,
        flags: if candidate.key.ends_with('2') {
            SpeciesVariantFlags::RARE
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

fn lower_animal_behavior_subject_types(record: &RecordView<'_, '_>) -> Vec<AssetId> {
    let ancestry = record.type_tokens();
    let mut types = ancestry
        .iter()
        .map(|name| AssetId::from_key(&name.to_ascii_lowercase()))
        .collect::<BTreeSet<_>>();
    types.insert(AssetId::from_key(&record.key.to_ascii_lowercase()));
    // Use the same resolved family order as need lowering. Later false values
    // must remove inherited true flags (for example a variant's capabilities).
    let family = ancestry
        .iter()
        .filter(|name| !source_document_names_are_semantically_equal(name, record.key))
        .filter_map(|name| record.find_resolved_source_record_by_reference(name))
        .chain(std::iter::once(*record));
    for member in family {
        for group in ["BFAIEntityDataShared", "BFAIEntityDataInstance"] {
            for data in find_all_semantically_named_source_descendants(
                member.source_document_element(),
                group,
            ) {
                for (name, value) in data.attributes() {
                    if !name.to_ascii_lowercase().starts_with("b_") {
                        continue;
                    }
                    let id = AssetId::from_key(&name.to_ascii_lowercase());
                    if authored_boolean_is_true(value) {
                        types.insert(id);
                    } else {
                        types.remove(&id);
                    }
                }
            }
        }
    }
    types.into_iter().collect()
}
