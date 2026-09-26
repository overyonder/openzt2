use super::source_element_tree_search::authored_type_family_components;
use super::world_definition_source_value_reading_and_conversion::element_bool;
#[cfg(test)]
use crate::assets::source_document::resolved_source_record_index::SourceIndex;
use crate::assets::source_document::resolved_source_record_index::{BindError, RecordView};
use openzt2_game_data::AssetId;

/// Resolves the native selected-entity dispatcher before releasing source data.
pub(super) fn selected_entity_information_panel(
    record: &RecordView<'_, '_>,
) -> Result<AssetId, BindError> {
    let section = {
        [
            ("ObjectCrate", "Crated entity Info"),
            ("AnimalCrate", "Crate Info"),
            ("animal", "Animal Info"),
            ("Guest", "Guest Info"),
            ("Staff", "Staff Info"),
            ("enrichment", "Enrichment Info"),
            ("food", "Food Info"),
            ("trashcontainer", "Trash Info"),
            ("building", "Building Info"),
            ("DinoRecoveryBuilding", "Dino Recovery Building Info"),
            ("donationbox", "Donation Box Info"),
            ("egg", "Egg Info"),
            ("scenery", "Scenery Info"),
            ("tree", "Tree Info"),
            ("path", "Path Info"),
            ("plant", "Plant Info"),
            ("marineplant", "Plant Info"),
            ("rock", "Rocks Info"),
            ("station", "Station Info"),
            ("vehicle", "Vehicle Info"),
            ("TrainingArea", "Training Area Info"),
        ]
        .into_iter()
        .find_map(|(kind, section)| record.has_type_token(kind).then_some(section))
        .unwrap_or("Scenery Info")
    };
    let section = if section == "Staff Info" {
        [
            ("Educator", "Educator Info"),
            ("Paleontologist", "Educator Info"),
            ("MC", "MC Info"),
            ("Keeper", "Keeper Info"),
            ("Worker", "MW Info"),
            ("Trainer", "Trainer Info"),
            ("Entertainer", "Entertainer Info"),
        ]
        .into_iter()
        .find_map(|(kind, section)| record.has_type_token(kind).then_some(section))
        .unwrap_or("Staff Info")
    } else if section == "Building Info" {
        let specialized_section = [
            ("ConservationBreedingCenter_end", "Breeding Center Info"),
            ("CloningCenter", "Cloning Center Info"),
            ("FossilEducationCenter", "Fossil Education Center Info"),
            ("ShowPlatform", "Show Info"),
            ("Water_Filter", "Tank Filter Info"),
        ]
        .into_iter()
        .find_map(|(kind, section)| record.has_type_token(kind).then_some(section));
        if let Some(section) = specialized_section {
            section
        } else if authored_type_family_components(record, "BFAIEntityDataShared")
            .into_iter()
            .find(|shared| {
                shared
                    .attribute_named_any(&["b_CommerceBuilding"])
                    .is_some()
            })
            .map(|shared| element_bool(&shared, &["b_CommerceBuilding"], false))
            .transpose()?
            .unwrap_or(false)
        {
            "Commerce Building Info"
        } else {
            "Building Info"
        }
    } else {
        section
    };
    Ok(AssetId::from_key(section))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::source_document::blue_fang_source_document_parsing::parse_blue_fang_source_document;
    use crate::assets::source_document::path::AssetPath;

    #[test]
    fn entity_information_sections_follow_source_kind_precedence() {
        for (types, attributes, expected) in [
            ("<tree/>", "", "Tree Info"),
            ("<marineplant/>", "", "Plant Info"),
            ("<rock/>", "", "Rocks Info"),
            ("<scenery/>", "", "Scenery Info"),
            ("<Guest/>", "", "Guest Info"),
            ("<Staff><Keeper/></Staff>", "", "Keeper Info"),
            ("<Staff><Worker/></Staff>", "", "MW Info"),
            ("<building><ShowPlatform/></building>", "", "Show Info"),
            (
                "<building><ShowPlatform/></building>",
                "b_CommerceBuilding='invalid'",
                "Show Info",
            ),
            (
                "<building><CloningCenter/></building>",
                "",
                "Cloning Center Info",
            ),
            (
                "<building><FossilEducationCenter/></building>",
                "",
                "Fossil Education Center Info",
            ),
            (
                "<building/>",
                "b_CommerceBuilding='true'",
                "Commerce Building Info",
            ),
            ("<animal/><Staff/>", "", "Animal Info"),
            ("<ObjectCrate/><animal/>", "", "Crated entity Info"),
        ] {
            let source = format!(
                "<BFTypedBinder binderType='subject'><types><entity>{types}</entity></types><shared><BFAIEntityDataShared {attributes}/></shared></BFTypedBinder>"
            );
            let document =
                parse_blue_fang_source_document(AssetPath::new("subject.xml"), source.as_bytes())
                    .expect("source fixture parses");
            let index = SourceIndex::build([&document]).expect("source fixture indexes");
            let record = index.find("subject").expect("subject is indexed");
            assert_eq!(
                selected_entity_information_panel(&record).expect("panel lowers"),
                AssetId::from_key(expected),
                "{types}"
            );
        }
    }
}
