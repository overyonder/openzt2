//! Parses staff-manager settings from `AI/staffMgr.xml`.

use super::world_definition_lowering_tables::WorldDefinitionLoweringTables;
use super::world_definition_source_value_reading_and_conversion::id;
use crate::assets::source_document::blue_fang_source_numeric_lexeme::parse_blue_fang_source_numeric_lexeme;
use crate::assets::source_document::ordered_source_document_types::OrderedSourceDocumentNode;
use crate::assets::source_document::resolved_source_record_index::{BindError, RecordView};
use crate::assets::source_document::source_document_semantic_name::{
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

const TOKEN_ATTRIBUTES: [&str; 4] = ["Name", "GiveTo", "Payload", "Force"];

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
    staff_tokens: &'_ OrderedSourceDocumentNode,
) -> Result<Vec<StaffTokenDispatchRule>, BindError> {
    let mut rules = Vec::new();
    for token_entry in staff_tokens.element_children() {
        let token = id(token_entry.name.as_str());
        let token_lists = token_entry
            .element_children()
            .filter(|child| {
                source_document_names_are_semantically_equal(child.name.as_str(), "BFAITokenList")
            })
            .collect::<Vec<_>>();
        let token_list = token_lists.first().copied();
        if token_lists.len() > 1 {
            return Err(BindError::record(
                record,
                format!(
                    "authored staff token {token:?} declares more than one BFAITokenList child"
                ),
            ));
        }
        let Some(token_list) = token_list else {
            return Err(BindError::record(
                record,
                format!("authored staff token {token:?} has no BFAITokenList child"),
            ));
        };
        for child in token_entry.element_children() {
            if !source_document_names_are_semantically_equal(child.name.as_str(), "BFAITokenList") {
                return Err(BindError::record(
                    record,
                    format!(
                        "authored staff token {token:?} child {} is not supported",
                        child.name.as_str()
                    ),
                ));
            }
        }
        for token_row in token_list.element_children() {
            if !source_document_names_are_semantically_equal(token_row.name.as_str(), "BFAIToken") {
                return Err(BindError::record(
                    record,
                    format!(
                        "authored BFAITokenList child {} on staff token {token:?} is not supported",
                        token_row.name.as_str()
                    ),
                ));
            }
            let mut name: Option<&str> = None;
            let mut give_to: Option<&str> = None;
            let mut payload: Option<&str> = None;
            let mut force: Option<&str> = None;
            for (attribute_name, value) in token_row.attributes() {
                let attribute = TOKEN_ATTRIBUTES
                    .iter()
                    .find(|candidate| source_document_names_are_semantically_equal(candidate, attribute_name))
                    .ok_or_else(|| {
                        BindError::record(
                            record,
                            format!(
                                "authored BFAIToken attribute {attribute_name:?} on staff token {token:?} is not supported"
                            ),
                        )
                    })?;
                match *attribute {
                    "Name" => name = Some(value),
                    "GiveTo" => give_to = Some(value),
                    "Payload" => payload = Some(value),
                    "Force" => force = Some(value),
                    _ => unreachable!("attribute names are matched from the authored vocabulary"),
                }
            }
            let authored_reference =
                |value: Option<&str>| value.filter(|value| !value.trim().is_empty()).map(id);
            rules.push(StaffTokenDispatchRule {
                token,
                name: authored_reference(name),
                give_to: authored_reference(give_to),
                payload: authored_reference(payload),
                force: force
                    .map(|value| match canonicalize_source_document_record_key(value).as_str() {
                        "true" | "yes" | "1" => Ok(true),
                        "false" | "no" | "0" => Ok(false),
                        value => Err(BindError::record(
                            record,
                            format!(
                                "authored BFAIToken Force {value:?} on staff token {token:?} is not a boolean"
                            ),
                        )),
                    })
                    .transpose()?,
            });
        }
    }
    Ok(rules)
}

#[cfg(test)]
mod tests {
    use super::id;
    use super::StaffRequestData;
    use openzt2_game_data::world_definitions::document::WorldDefinitionDocument;

    use crate::assets::source_document::blue_fang_actor_manifest_model_and_scene_resolution_index::BlueFangActorManifestModelAndSceneResolutionIndex;
    use crate::assets::source_document::blue_fang_source_document_parsing::parse_blue_fang_source_document;
    use crate::assets::source_document::path::AssetPath;
    use crate::assets::world_definitions::world_definition_source_document_lowering::lower_resolved_world_definition_source_document_closure_to_canonical_document;

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
