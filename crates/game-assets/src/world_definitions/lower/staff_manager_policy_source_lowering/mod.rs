//! Parses staff-manager settings from `AI/staffMgr.xml`.

use super::world_definition_lowering_tables::WorldDefinitionLoweringTables;
use super::world_definition_source_value_reading_and_conversion::id;
use crate::source_document::blue_fang_source_numeric_lexeme::parse_blue_fang_source_numeric_lexeme;
use crate::source_document::ordered_source_document_types::OrderedSourceDocumentNode;
use crate::source_document::resolved_source_record_index::{BindError, RecordView};
use crate::source_document::source_document_semantic_name::{
    canonicalize_source_document_record_key, source_document_names_are_semantically_equal,
};
use openzt2_game_data::world_definitions::staff_management::{
    StaffManagerPolicy, StaffRequestData, StaffTokenDispatchRule,
};

const MANAGER_RECORD_TYPE: &str = "ztaistaffmgr";

const MANAGER_ATTRIBUTES: [&str; 7] = [
    "jobSafeDistance",
    "stealJobThreshold",
    "badEntityCleanupInterval",
    "PaleontologistSearchDelay",
    "TimeCategoryDuration",
    "DisplacementX",
    "DisplacementY",
];

pub(super) fn bind_authored_staff_manager_policy(
    record: &RecordView<'_, '_>,
    output: &mut WorldDefinitionLoweringTables,
) -> Result<(), BindError> {
    if canonicalize_source_document_record_key(&record.semantic_type()) != MANAGER_RECORD_TYPE {
        return Ok(());
    }
    if output.document.staff_manager.is_some() {
        return Err(BindError::record(
            record,
            "duplicate authored ZTAIStaffMgr policy in one world-definition document",
        ));
    }
    let mut policy = StaffManagerPolicy {
        id: id(record.key),
        tokens: Vec::new(),
        care_types: Vec::new(),
        job_safe_distance_m: 6.0,
        steal_job_threshold: 10.0,
        bad_entity_cleanup_interval: 30.0,
        paleontologist_search_delay_seconds: 120.0,
        time_category_duration: 1,
        displacement_m: [0.0; 2],
        default_request: StaffRequestData::CONSTRUCTED_DEFAULT,
    };
    for (name, value) in record.source_document_element().attributes() {
        // Shipped x301 staffMgr authors these diagnostics. Neither the retail
        // ZTAIStaffMgr reader nor its BFAIEntityMgr/BFScriptComponent base
        // readers consume them; they have no gameplay policy to retain.
        if source_document_names_are_semantically_equal(name, "LogRequests")
            || source_document_names_are_semantically_equal(name, "LogAssignments")
        {
            continue;
        }
        let attribute = MANAGER_ATTRIBUTES
            .iter()
            .find(|candidate| source_document_names_are_semantically_equal(candidate, name))
            .ok_or_else(|| {
                BindError::record(
                    record,
                    format!("authored staff manager attribute {name:?} is not supported"),
                )
            })?;
        match *attribute {
            "jobSafeDistance" => {
                policy.job_safe_distance_m = lower_authored_policy_number(record, name, value)?;
            }
            "stealJobThreshold" => {
                policy.steal_job_threshold = lower_authored_policy_number(record, name, value)?;
            }
            "badEntityCleanupInterval" => {
                policy.bad_entity_cleanup_interval =
                    lower_authored_policy_number(record, name, value)?;
            }
            "TimeCategoryDuration" => {
                policy.time_category_duration = parse_blue_fang_source_numeric_lexeme::<i32>(value).ok_or_else(
                    || {
                        BindError::record(
                            record,
                            format!(
                                "authored staff manager TimeCategoryDuration {value:?} is not an integer"
                            ),
                        )
                    },
                )?;
            }
            "PaleontologistSearchDelay" => {
                policy.paleontologist_search_delay_seconds =
                    lower_authored_policy_number(record, name, value)?;
            }
            "DisplacementX" => {
                policy.displacement_m[0] = lower_authored_policy_number(record, name, value)?;
            }
            "DisplacementY" => {
                policy.displacement_m[1] = lower_authored_policy_number(record, name, value)?;
            }
            _ => unreachable!("attribute names are matched from the authored vocabulary"),
        }
    }
    for child in record.source_document_element().element_children() {
        if source_document_names_are_semantically_equal(
            child.name.as_str(),
            "typesThatStaffCareAbout",
        ) {
            policy.care_types.extend(
                child
                    .element_children()
                    .map(|care_type| id(care_type.name.as_str())),
            );
        } else if source_document_names_are_semantically_equal(child.name.as_str(), "StaffTokens") {
            policy
                .tokens
                .extend(lower_authored_staff_tokens(record, child)?);
        } else {
            return Err(BindError::record(
                record,
                format!(
                    "authored staff manager child {} is not supported",
                    child.name.as_str()
                ),
            ));
        }
    }
    output.document.staff_manager = Some(policy);
    Ok(())
}

fn lower_authored_policy_number(
    record: &RecordView<'_, '_>,
    field: &str,
    value: &str,
) -> Result<f32, BindError> {
    parse_blue_fang_source_numeric_lexeme::<f32>(value)
        .filter(|value| value.is_finite())
        .ok_or_else(|| {
            BindError::record(
                record,
                format!("authored staff manager {field} {value:?} is not a finite number"),
            )
        })
}

fn lower_authored_staff_tokens(
    record: &RecordView<'_, '_>,
    staff_tokens: &OrderedSourceDocumentNode,
) -> Result<Vec<StaffTokenDispatchRule>, BindError> {
    use super::world_definition_source_value_reading_and_conversion::element_bool;
    use openzt2_game_data::world_definitions::staff_management::StaffTokenRoutingDefinition;
    let mut rules = Vec::new();
    for entry in staff_tokens.element_children() {
        let token = id(entry.name.as_str());
        for list in entry.element_children() {
            if !list.name.eq_ignore_ascii_case("BFAITokenList") {
                return Err(BindError::record(
                    record,
                    "staff token entry must contain BFAITokenList rows",
                ));
            }
            for row in list.element_children() {
                if !row.name.eq_ignore_ascii_case("BFAIToken") {
                    return Err(BindError::record(
                        record,
                        "staff token list must contain BFAIToken rows",
                    ));
                }
                let get = |key| {
                    row.attribute_named_any(&[key])
                        .filter(|value| !value.trim().is_empty())
                };
                let number = |key| {
                    get(key)
                        .map(|value| lower_authored_policy_number(record, key, value))
                        .transpose()
                };
                let force = get("Force")
                    .map(|_| element_bool(&row, &["Force"], false))
                    .transpose()?;
                let point_m = row.element_children().find(|node| node.name.eq_ignore_ascii_case("target"))
                    .map(|target| {
                        let coordinate = |key| target.attribute_named_any(&[key])
                            .map(|value| lower_authored_policy_number(record, key, value)).transpose()
                            .map(Option::unwrap_or_default);
                        Ok::<_, BindError>(crate::source_coordinate_conversion::convert_source_z_up_vector_to_bevy_y_up_coordinates(
                            [coordinate("x")?, coordinate("y")?, coordinate("z")?],
                        ))
                    }).transpose()?;
                rules.push(StaffTokenDispatchRule {
                    token,
                    name: get("Name").map(id),
                    give_to: get("GiveTo").map(id),
                    payload: get("Payload").map(id),
                    force,
                    routing: StaffTokenRoutingDefinition {
                        list: list
                            .attribute_named_any(&["Name"])
                            .filter(|name| !name.trim().is_empty())
                            .map(id),
                        chance_percent: number("Chance")?,
                        radius_m: number("Radius")?,
                        chance_in_radius_percent: number("ChanceInRadius")?,
                        time_in_seconds: number("Timein")?,
                        timeout_seconds: number("Timeout")?,
                        time_left_seconds: number("TimeLeft")?,
                        time_to_activation_seconds: number("TimeToActivation")?,
                        point_m,
                        xml_payload: row
                            .element_children()
                            .find(|node| node.name.eq_ignore_ascii_case("tokenData"))
                            .map(lower_staff_token_xml_element),
                    },
                });
            }
        }
    }
    Ok(rules)
}

fn lower_staff_token_xml_element(
    node: &OrderedSourceDocumentNode,
) -> openzt2_game_data::world_definitions::staff_management::StaffTokenXmlElement {
    use crate::source_document::ordered_source_document_types::OrderedSourceDocumentChild;
    use openzt2_game_data::world_definitions::staff_management::{
        StaffTokenXmlContent, StaffTokenXmlElement,
    };
    StaffTokenXmlElement {
        name: node.name.to_string(),
        attributes: node
            .attributes()
            .map(|(name, value)| (name.to_owned(), value.to_owned()))
            .collect(),
        children: node
            .child_items()
            .iter()
            .filter_map(|child| match child {
                OrderedSourceDocumentChild::Text(text) => {
                    Some(StaffTokenXmlContent::Text(text.value().to_owned()))
                }
                OrderedSourceDocumentChild::Element(element) => Some(
                    StaffTokenXmlContent::Element(lower_staff_token_xml_element(element)),
                ),
                OrderedSourceDocumentChild::Comment
                | OrderedSourceDocumentChild::ProcessingInstruction => None,
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::id;
    use super::StaffRequestData;
    use openzt2_game_data::world_definitions::document::WorldDefinitionDocument;

    use crate::source_document::blue_fang_actor_manifest_model_and_scene_resolution_index::BlueFangActorManifestModelAndSceneResolutionIndex;
    use crate::source_document::blue_fang_source_document_parsing::parse_blue_fang_source_document;
    use crate::source_document::path::AssetPath;
    use crate::world_definitions::world_definition_source_document_lowering::lower_resolved_world_definition_source_document_closure_to_canonical_document;

    const AUTHORED_STAFF_MANAGER: &str = r#"<ZTAIStaffMgr jobSafeDistance="6.0f" stealJobThreshold="10.0f" DisplacementX="0" DisplacementY="5" LogRequests="true" LogAssignments="false">
        <typesThatStaffCareAbout>
            <staffcenter_df/>
            <bench/>
        </typesThatStaffCareAbout>
        <StaffTokens>
            <t_FillFoodContainer>
                <BFAITokenList>
                    <BFAIToken Name="t_FillFoodContainer" GiveTo="subject" Payload="target" Force="1"/>
                </BFAITokenList>
            </t_FillFoodContainer>
            <t_FeedAnimal>
                <BFAITokenList>
                    <BFAIToken Name="t_FeedAnimal" GiveTo="subject" Payload="target" Force="1"/>
                </BFAITokenList>
            </t_FeedAnimal>
        </StaffTokens>
    </ZTAIStaffMgr>"#;

    fn lower_staff_manager(source: &str) -> Result<WorldDefinitionDocument, String> {
        let documents = [parse_blue_fang_source_document(
            AssetPath::new("AI/staffMgr.xml"),
            source.as_bytes(),
        )]
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("{error:?}"))?;
        lower_resolved_world_definition_source_document_closure_to_canonical_document(
            &documents,
            &BlueFangActorManifestModelAndSceneResolutionIndex::default(),
            "AI/staffMgr.xml",
            [0, 0],
            &mut Default::default(),
        )
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "staff manager document was dropped".to_owned())
    }

    #[test]
    fn staff_manager_policy_lowers_with_authored_and_default_values(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let document = lower_staff_manager(AUTHORED_STAFF_MANAGER)?;
        let policy = document
            .staff_manager
            .as_ref()
            .ok_or("staff manager policy absent")?;
        assert_eq!(policy.job_safe_distance_m, 6.0);
        assert_eq!(policy.steal_job_threshold, 10.0);
        assert_eq!(policy.bad_entity_cleanup_interval, 30.0);
        assert_eq!(policy.paleontologist_search_delay_seconds, 120.0);
        assert_eq!(policy.time_category_duration, 1);
        assert_eq!(policy.displacement_m, [0.0, 5.0]);
        assert_eq!(policy.care_types, vec![id("staffcenter_df"), id("bench")]);
        assert_eq!(policy.tokens.len(), 2);
        let refill = &policy.tokens[0];
        assert_eq!(refill.token, id("t_FillFoodContainer"));
        assert_eq!(refill.name, Some(id("t_FillFoodContainer")));
        assert_eq!(refill.give_to, Some(id("subject")));
        assert_eq!(refill.payload, Some(id("target")));
        assert_eq!(refill.force, Some(true));
        assert_eq!(
            policy.default_request,
            StaffRequestData::CONSTRUCTED_DEFAULT
        );
        Ok(())
    }

    #[test]
    fn expansion_staff_manager_retains_authored_fossil_sensor_delay() {
        let source = AUTHORED_STAFF_MANAGER.replace(
            "<ZTAIStaffMgr ",
            "<ZTAIStaffMgr PaleontologistSearchDelay=\"75\" ",
        );
        let policy = lower_staff_manager(&source).unwrap().staff_manager.unwrap();
        assert_eq!(policy.paleontologist_search_delay_seconds, 75.0);
    }
}
