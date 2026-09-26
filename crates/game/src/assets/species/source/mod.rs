use self::adoption::lower_authored_animal_adoption_offer_definition;
use self::locomotion::lower_authored_species_ground_movement_speed_metres_per_second;
use self::locomotion::lower_authored_species_swimming_speed_metres_per_second;
use self::needs::lower_effective_authored_species_need_facts;
use self::source_queries::authored_boolean_is_true;
use self::source_queries::authored_optional_boolean_is_true;
use self::variants::lower_species_root_actor_to_fallback_canonical_variant;
use self::variants::lower_species_variant_source_record_to_canonical_variant;
use crate::assets::source_document::blue_fang_actor_manifest_model_and_scene_resolution_index::BlueFangActorManifestModelAndSceneResolutionIndex;
use crate::assets::source_document::document_semantics::classify_source_document;
use crate::assets::source_document::document_semantics::SourceDocumentKind;
use crate::assets::source_document::ordered_source_document_types::OrderedSourceDocument;
use crate::assets::source_document::ordered_source_document_types::OrderedSourceDocumentNode;
use crate::assets::source_document::resolved_source_record_index::BindError;
use crate::assets::source_document::resolved_source_record_index::RecordView;
use crate::assets::source_document::resolved_source_record_index::SourceIndex;
use crate::assets::source_document::source_document_semantic_name::canonicalize_source_document_record_key;
use crate::assets::source_document::source_document_semantic_name::source_document_names_are_semantically_equal;
use openzt2_game_data::species::ConservationStatus;
use openzt2_game_data::species::Species;
use openzt2_game_data::species::SpeciesDocument;
use openzt2_game_data::species::SpeciesFlags;
use openzt2_game_data::species::SpeciesVariantBinding;
use openzt2_game_data::AssetId;
use std::collections::BTreeSet;

mod adoption;
mod locomotion;
mod needs;
mod source_queries;
mod variants;

pub(super) fn collect_authored_species_type_source_references(
    document: &OrderedSourceDocument,
) -> BTreeSet<String> {
    fn collect_authored_inheritance_source_references(
        element: &'_ OrderedSourceDocumentNode,
        references: &mut BTreeSet<String>,
    ) {
        element
            .attributes()
            .filter(|(name, _)| {
                ["extends", "base", "parentType", "inherit"]
                    .iter()
                    .any(|candidate| name.eq_ignore_ascii_case(candidate))
            })
            .map(|(_, value)| value.trim())
            .filter(|value| !value.is_empty())
            .for_each(|value| {
                references.insert(value.to_owned());
            });
        element
            .element_children()
            .for_each(|child| collect_authored_inheritance_source_references(child, references));
    }

    fn collect_authored_type_tree_source_references(
        element: &'_ OrderedSourceDocumentNode,
        references: &mut BTreeSet<String>,
    ) {
        element.element_children().for_each(|child| {
            references.insert(child.name.as_str().to_owned());
            collect_authored_type_tree_source_references(child, references);
        });
    }

    let mut references = BTreeSet::new();
    let root = &document.root;
    collect_authored_inheritance_source_references(root, &mut references);
    root.element_children()
        .filter(|child| source_document_names_are_semantically_equal(child.name.as_str(), "types"))
        .for_each(|types| collect_authored_type_tree_source_references(types, &mut references));
    references
}

pub(super) fn lower_resolved_species_source_document_closure_to_canonical_document(
    source_documents: &[OrderedSourceDocument],
    actor_manifest_resolution_index: &BlueFangActorManifestModelAndSceneResolutionIndex,
    primary_path: &str,
) -> Result<SpeciesDocument, BindError> {
    let species_source_documents = source_documents.iter().filter(|source_document| {
        classify_source_document(source_document) == SourceDocumentKind::SpeciesOrEntity
    });
    let index = SourceIndex::build(species_source_documents)?;
    let Some(current) = index.document_root(primary_path) else {
        return Ok(SpeciesDocument::default());
    };
    let Some(species) = std::iter::once(current)
        .chain(index.records())
        .find(|candidate| {
            source_record_is_species_purchase_family_root(candidate)
                && (candidate.key == current.key
                    || current.has_type_token(candidate.key)
                    || current
                        .descendant_named("BFAIEntityDataShared")
                        .and_then(|shared| shared.attribute_named_any(&["s_Species"]))
                        .is_some_and(|value| {
                            source_document_names_are_semantically_equal(value, candidate.key)
                        }))
        })
    else {
        return Ok(SpeciesDocument::default());
    };

    let species_record = (current.key == species.key)
        .then(|| lower_species_source_record_to_canonical_species(&species))
        .transpose()?
        .into_iter()
        .collect();
    let mut variants = lower_species_variant_source_record_to_canonical_variant(
        &species,
        &current,
        actor_manifest_resolution_index,
    )?
    .into_iter()
    .map(|variant| SpeciesVariantBinding {
        species: AssetId::from_key(&canonicalize_source_document_record_key(species.key)),
        variant,
    })
    .collect::<Vec<_>>();
    if current.key == species.key && variants.is_empty() {
        variants.extend(
            lower_species_root_actor_to_fallback_canonical_variant(
                &species,
                actor_manifest_resolution_index,
            )?
            .map(|variant| SpeciesVariantBinding {
                species: AssetId::from_key(&canonicalize_source_document_record_key(species.key)),
                variant,
            }),
        );
    }
    Ok(SpeciesDocument {
        species: species_record,
        variants,
        compatibilities: Vec::new(),
    })
}

fn lower_species_source_record_to_canonical_species(
    species: &RecordView<'_, '_>,
) -> Result<Species, BindError> {
    let shared = species
        .descendant_named("BFAIEntityDataShared")
        .ok_or_else(|| BindError::record(species, "animal family has no shared entity data"))?;
    let name_key = species
        .descendant_named("UIHelpInfo")
        .and_then(|element| element.attribute_named_any(&["ids"]))
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(species.key);
    let taxonomy = species
        .type_tokens()
        .into_iter()
        .take_while(|token| !token.eq_ignore_ascii_case(species.key))
        .filter(|token| {
            !["entity", "actor", "animal", "mammalia", "aves"]
                .iter()
                .any(|generic| token.eq_ignore_ascii_case(generic))
        })
        .last()
        .unwrap_or_else(|| species.key.to_owned());
    let endangerment = shared
        .attribute_named_any(&["s_Endangerment"])
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    let conservation = match endangerment.as_str() {
        "" | "none" | "notendangered" => ConservationStatus::Unspecified,
        "lowrisk" => ConservationStatus::LowRisk,
        "vulnerable" => ConservationStatus::Vulnerable,
        "endangered" => ConservationStatus::Endangered,
        "critical" | "criticallyendangered" => ConservationStatus::Critical,
        "extinct" => ConservationStatus::Extinct,
        unknown => {
            return Err(BindError::record(
                species,
                format!("unknown authored endangerment classification {unknown}"),
            ));
        }
    };
    let mut flags = SpeciesFlags::ADOPTABLE;
    for (attribute, flag) in [
        ("b_Carnivore", SpeciesFlags::PREDATOR),
        ("b_Predator", SpeciesFlags::PREDATOR),
        ("b_Prey", SpeciesFlags::PREY),
        ("b_MediumPrey", SpeciesFlags::PREY),
        ("b_LargePrey", SpeciesFlags::PREY),
        ("b_Super", SpeciesFlags::SUPER),
        ("b_SmallPredator", SpeciesFlags::SMALL_PREDATOR),
        ("b_MediumPredator", SpeciesFlags::MEDIUM_PREDATOR),
        ("b_LargePredator", SpeciesFlags::LARGE_PREDATOR),
        ("b_XLargePredator", SpeciesFlags::EXTRA_LARGE_PREDATOR),
        ("b_SmallPrey", SpeciesFlags::SMALL_PREY),
        ("b_MediumPrey", SpeciesFlags::MEDIUM_PREY),
        ("b_LargePrey", SpeciesFlags::LARGE_PREY),
        ("b_XLargePrey", SpeciesFlags::EXTRA_LARGE_PREY),
        ("b_XXLargePrey", SpeciesFlags::DOUBLE_EXTRA_LARGE_PREY),
    ] {
        if authored_optional_boolean_is_true(shared.attribute_named_any(&[attribute])) {
            flags = flags | flag;
        }
    }
    if species.descendant_named("BFSwimComponent").is_some() {
        flags = flags | SpeciesFlags::SWIMS;
    }
    if matches!(
        conservation,
        ConservationStatus::Endangered | ConservationStatus::Critical
    ) {
        flags = flags | SpeciesFlags::ENDANGERED;
    }
    if conservation == ConservationStatus::Extinct {
        flags = flags | SpeciesFlags::EXTINCT;
    }

    Ok(Species {
        id: AssetId::from_key(&canonicalize_source_document_record_key(species.key)),
        name_key: AssetId::from_key(name_key),
        taxonomy_key: AssetId::from_key(&canonicalize_source_document_record_key(&taxonomy)),
        world_definition: AssetId::from_key(&canonicalize_source_document_record_key(species.key)),
        needs: lower_effective_authored_species_need_facts(species)?,
        adult_mass_kg: [0.0; 2],
        stage_start_days: [0.0, 0.0, 0.0, 1_000_000_000.0],
        lifespan_days: [1_000_000_000.0; 2],
        gestation_days: 0.0,
        litter: [0; 2],
        move_speed_mps: lower_authored_species_ground_movement_speed_metres_per_second(species),
        swim_speed_mps: lower_authored_species_swimming_speed_metres_per_second(species),
        appetite_per_day: 0.0,
        waste_definition: AssetId::default(),
        waste_interval_days: 0.0,
        waste_units: 0,
        conservation,
        flags,
        adoption_offer: lower_authored_animal_adoption_offer_definition(species)?,
    })
}

fn source_record_is_species_purchase_family_root(record: &RecordView<'_, '_>) -> bool {
    record.has_type_token("animal")
        && record
            .source_document_element()
            .attribute_named_any(&["abstract"])
            .is_some_and(authored_boolean_is_true)
        && record.descendant_named("UIToggleButton").is_some()
        && record
            .descendant_named("BFAIEntityDataShared")
            .and_then(|element| element.attribute_named_any(&["s_Species"]))
            .is_some_and(|value| source_document_names_are_semantically_equal(value, record.key))
}
