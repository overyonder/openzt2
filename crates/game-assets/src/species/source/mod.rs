use self::adoption::lower_authored_animal_adoption_offer_definition;
use self::locomotion::lower_authored_species_ground_movement_speed_metres_per_second;
use self::locomotion::lower_authored_species_swimming_speed_metres_per_second;
use self::needs::lower_effective_authored_species_need_facts;
use self::source_queries::authored_optional_boolean_is_true;
use self::source_queries::find_effective_authored_animal_shared_attribute;
use self::source_queries::find_inherited_animal_shared_attribute;
use self::variants::lower_species_root_actor_to_fallback_canonical_variant;
use self::variants::lower_species_variant_source_record_to_canonical_variant;
use crate::source_document::authored_species_purchase_family_queries::find_authored_species_purchase_family_root;
use crate::source_document::blue_fang_actor_manifest_model_and_scene_resolution_index::BlueFangActorManifestModelAndSceneResolutionIndex;
use crate::source_document::document_semantics::classify_source_document;
use crate::source_document::document_semantics::SourceDocumentKind;
use crate::source_document::ordered_source_document_types::OrderedSourceDocument;
use crate::source_document::ordered_source_document_types::OrderedSourceDocumentNode;
use crate::source_document::resolved_source_record_index::BindError;
use crate::source_document::resolved_source_record_index::RecordView;
use crate::source_document::resolved_source_record_index::SourceIndex;
use crate::source_document::source_document_semantic_name::canonicalize_source_document_record_key;
use crate::source_document::source_document_semantic_name::source_document_names_are_semantically_equal;
use openzt2_game_data::species::ConservationStatus;
use openzt2_game_data::species::Species;
use openzt2_game_data::species::SpeciesDocument;
use openzt2_game_data::species::SpeciesFlags;
use openzt2_game_data::species::SpeciesVariantBinding;
use openzt2_game_data::AssetId;
use std::collections::BTreeSet;

pub mod adoption;
mod donation;
pub mod habitat_space;
pub mod locomotion;
pub mod needs;
pub mod source_queries;
pub mod variants;

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
    let Some(species) = find_authored_species_purchase_family_root(&current, index.records())
    else {
        return Ok(SpeciesDocument::default());
    };

    let authored_species_name =
        find_effective_authored_animal_shared_attribute(&species, "s_Species")
            .ok_or_else(|| BindError::record(&species, "animal family has no s_Species"))?;
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
        authored_species_name_override: find_effective_authored_animal_shared_attribute(
            &current,
            "s_Species",
        )
        .filter(|value| *value != authored_species_name)
        .map(str::to_owned),
        authored_product_name_override: find_effective_authored_animal_shared_attribute(
            &current,
            "s_Product",
        )
        .filter(|value| {
            Some(*value) != find_effective_authored_animal_shared_attribute(&species, "s_Product")
        })
        .map(str::to_owned),
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
                authored_species_name_override: None,
                authored_product_name_override: None,
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
        ("b_ExtinctAnimal", SpeciesFlags::EXTINCT),
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
        donation: donation::lower_animal_donation_policy(species)?,
        id: AssetId::from_key(&canonicalize_source_document_record_key(species.key)),
        authored_species_name: find_effective_authored_animal_shared_attribute(
            species,
            "s_Species",
        )
        .ok_or_else(|| BindError::record(species, "animal family has no s_Species"))?
        .to_owned(),
        authored_product_name: find_effective_authored_animal_shared_attribute(
            species,
            "s_Product",
        )
        .unwrap_or_default()
        .to_owned(),
        name_key: AssetId::from_key(name_key),
        taxonomy_key: AssetId::from_key(&canonicalize_source_document_record_key(&taxonomy)),
        world_definition: AssetId::from_key(&canonicalize_source_document_record_key(species.key)),
        needs: lower_effective_authored_species_need_facts(species)?,
        habitat_space: habitat_space::lower_authored_species_habitat_space(species)?,
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
        keeper_food_definition: keeper_placed_definition(species, "s_KeeperEatType"),
        keeper_drink_definition: keeper_placed_definition(species, "s_KeeperDrinkType"),
        conservation,
        flags,
        adoption_offer: lower_authored_animal_adoption_offer_definition(species)?,
    })
}

/// Keeper placement types name world object definitions by their record key.
fn keeper_placed_definition(species: &RecordView<'_, '_>, attribute: &str) -> Option<AssetId> {
    find_inherited_animal_shared_attribute(species, attribute)
        .map(canonicalize_source_document_record_key)
        .filter(|key| !key.is_empty())
        .map(|key| AssetId::from_key(&key))
}
