//! Parses staff-request controllers, including inherited binders.

use super::world_definition_lowering_tables::WorldDefinitionLoweringTables;
use super::world_definition_source_value_reading_and_conversion::id;
use crate::assets::source_document::blue_fang_source_numeric_lexeme::parse_blue_fang_source_numeric_lexeme;
use crate::assets::source_document::ordered_source_document_types::OrderedSourceDocumentNode;
use crate::assets::source_document::resolved_source_record_index::{BindError, RecordView};
use crate::assets::source_document::source_document_semantic_name::{
    canonicalize_source_document_record_key, source_document_names_are_semantically_equal,
};
use openzt2_game_data::world_definitions::staff_management::{
    StaffRequestControllerDefinition, StaffRequestData, StaffRequestThresholdComparison,
    StaffRequestThresholdValue,
};
use openzt2_game_data::AssetId;
use std::collections::BTreeSet;

const CONTROLLER_COMPONENT: &str = "ZTAIStaffRequestController";
const REQUEST_DATA_COMPONENT: &str = "ZTStaffRequestData";

const CONTROLLER_ATTRIBUTES: [&str; 7] = [
    "attribName",
    "thresholdValue",
    "testType",
    "cancelThresholdValue",
    "cancelTestType",
    "triggerOnCreation",
    "triggerOnDestruction",
];

const REQUEST_DATA_ATTRIBUTES: [&str; 5] =
    ["priority", "tokenKey", "subjectType", "target", "staff"];

const CONSTRUCTED_NUMBER_THRESHOLD_DEFAULT: f32 = 101.0;
const CONSTRUCTED_NUMBER_CANCEL_DEFAULT: f32 = 0.0;

pub(super) fn bind_authored_staff_request_controllers_to_entity_definitions(
    record: &RecordView<'_, '_>,
    output: &mut WorldDefinitionLoweringTables,
) -> Result<(), BindError> {
    // The staff manager itself is lowered by the manager-policy binder.
    if canonicalize_source_document_record_key(&record.semantic_type()) == "ztaistaffmgr" {
        return Ok(());
    }
    for controller in inherited_authored_staff_request_controller_instances(record) {
        let definition = lower_authored_staff_request_controller_instance(record, controller)?;
        output.document.staff_requests.push(definition);
    }
    Ok(())
}

/// Collects `ZTAIStaffRequestController` binder instances most-specific first.
///
/// The record's own binders come first, then resolved ancestors in
/// nearest-first order. Each ancestor contributes its own binders before a
/// `<suppressed>` section stops traversal at that record (authored
/// `entities/objects/paths/ai/path.xml` suppresses the inherited controller).
fn inherited_authored_staff_request_controller_instances<'index, 'document>(
    record: &RecordView<'index, 'document>,
) -> Vec<(&'document OrderedSourceDocumentNode, Option<AssetId>)> {
    fn append_binder_controller_instances<'document>(
        element: &'document OrderedSourceDocumentNode,
        output: &mut Vec<(&'document OrderedSourceDocumentNode, Option<AssetId>)>,
        claimed_named_binders: &mut BTreeSet<AssetId>,
    ) {
        for binder in element.element_children().filter(|binder| {
            source_document_names_are_semantically_equal(binder.name.as_str(), "BFBinder")
                || source_document_names_are_semantically_equal(
                    binder.name.as_str(),
                    "BFNamedBinder",
                )
        }) {
            let binder_name = binder
                .attribute_named_any(&["binderName"])
                .filter(|name| !name.trim().is_empty())
                .map(id);
            if let Some(binder_name) = binder_name {
                if !claimed_named_binders.insert(binder_name) {
                    continue;
                }
            }
            for instance in binder.element_children().filter(|child| {
                source_document_names_are_semantically_equal(child.name.as_str(), "instance")
            }) {
                for controller in descendant_elements_named(instance, CONTROLLER_COMPONENT) {
                    output.push((controller, binder_name));
                }
            }
        }
    }

    let mut instances = Vec::new();
    let mut claimed_named_binders = BTreeSet::new();
    for binder_section in record
        .source_document_element()
        .element_children()
        .filter(|child| source_document_names_are_semantically_equal(child.name.as_str(), "binder"))
    {
        append_binder_controller_instances(
            binder_section,
            &mut instances,
            &mut claimed_named_binders,
        );
    }
    if !authored_record_suppresses_controller(record) {
        for family_key in record
            .type_tokens()
            .into_iter()
            .rev()
            .filter(|family_key| {
                !source_document_names_are_semantically_equal(family_key, record.key)
            })
            .filter_map(|family_key| record.find_resolved_source_record_by_reference(&family_key))
        {
            for binder_section in family_key
                .source_document_element()
                .element_children()
                .filter(|child| {
                    source_document_names_are_semantically_equal(child.name.as_str(), "binder")
                })
            {
                append_binder_controller_instances(
                    binder_section,
                    &mut instances,
                    &mut claimed_named_binders,
                );
            }
            if authored_record_suppresses_controller(&family_key) {
                break;
            }
        }
    }
    instances
}

/// `<suppressed>` sections remove inherited binder components by name
/// (`entities/objects/paths/ai/path.xml` suppresses the request controller).
fn authored_record_suppresses_controller(record: &RecordView<'_, '_>) -> bool {
    record
        .source_document_element()
        .element_children()
        .find(|child| {
            source_document_names_are_semantically_equal(child.name.as_str(), "suppressed")
        })
        .is_some_and(|suppressed| {
            suppressed.element_children().any(|child| {
                source_document_names_are_semantically_equal(
                    child.name.as_str(),
                    CONTROLLER_COMPONENT,
                )
            })
        })
}

fn descendant_elements_named<'a>(
    element: &'a OrderedSourceDocumentNode,
    wanted: &str,
) -> Vec<&'a OrderedSourceDocumentNode> {
    let mut output = Vec::new();
    append_descendant_elements_named(element, wanted, &mut output);
    output
}

fn append_descendant_elements_named<'a>(
    element: &'a OrderedSourceDocumentNode,
    wanted: &str,
    output: &mut Vec<&'a OrderedSourceDocumentNode>,
) {
    for child in element.element_children() {
        if source_document_names_are_semantically_equal(child.name.as_str(), wanted) {
            output.push(child);
        }
        append_descendant_elements_named(child, wanted, output);
    }
}

fn lower_authored_staff_request_controller_instance(
    record: &RecordView<'_, '_>,
    instance: (&'_ OrderedSourceDocumentNode, Option<AssetId>),
) -> Result<StaffRequestControllerDefinition, BindError> {
    let (controller, binder) = instance;
    let mut attribute_key: Option<&str> = None;
    let mut threshold_value: Option<&str> = None;
    let mut test_type: Option<&str> = None;
    let mut cancel_threshold_value: Option<&str> = None;
    let mut cancel_test_type: Option<&str> = None;
    let mut trigger_on_creation: Option<&str> = None;
    let mut trigger_on_destruction: Option<&str> = None;
    for (name, value) in controller.attributes() {
        let attribute = CONTROLLER_ATTRIBUTES
            .iter()
            .find(|candidate| source_document_names_are_semantically_equal(candidate, name))
            .ok_or_else(|| {
                BindError::record(
                    record,
                    format!(
                        "authored staff request controller attribute {name:?} is not supported"
                    ),
                )
            })?;
        match *attribute {
            "attribName" => attribute_key = Some(value),
            "thresholdValue" => threshold_value = Some(value),
            "testType" => test_type = Some(value),
            "cancelThresholdValue" => cancel_threshold_value = Some(value),
            "cancelTestType" => cancel_test_type = Some(value),
            "triggerOnCreation" => trigger_on_creation = Some(value),
            "triggerOnDestruction" => trigger_on_destruction = Some(value),
            _ => unreachable!("attribute names are matched from the authored vocabulary"),
        }
    }
    let attribute_key_id = attribute_key.filter(|key| !key.trim().is_empty()).map(id);
    // An empty tracked attribute keeps the default comparisons and values.
    let threshold_comparison = parse_test_type(record, test_type, 4)?;
    let cancel_comparison = parse_test_type(record, cancel_test_type, 1)?;
    Ok(StaffRequestControllerDefinition {
        id: id(record.key),
        binder,
        attribute_key: attribute_key_id,
        threshold: lower_authored_staff_request_threshold_value(
            record,
            "thresholdValue",
            attribute_key,
            threshold_value,
            CONSTRUCTED_NUMBER_THRESHOLD_DEFAULT,
        )?,
        threshold_comparison,
        cancel_threshold: lower_authored_staff_request_threshold_value(
            record,
            "cancelThresholdValue",
            attribute_key,
            cancel_threshold_value,
            CONSTRUCTED_NUMBER_CANCEL_DEFAULT,
        )?,
        cancel_comparison,
        trigger_on_creation: trigger_on_creation
            .map(|value| parse_authored_bool(record, "triggerOnCreation", value))
            .transpose()?
            .unwrap_or(false),
        trigger_on_destruction: trigger_on_destruction
            .map(|value| parse_authored_bool(record, "triggerOnDestruction", value))
            .transpose()?
            .unwrap_or(false),
        request: lower_authored_staff_request_data(record, controller)?,
    })
}

fn lower_authored_staff_request_threshold_value(
    record: &RecordView<'_, '_>,
    field: &str,
    attribute_key: Option<&str>,
    authored_value: Option<&str>,
    constructed_default: f32,
) -> Result<StaffRequestThresholdValue, BindError> {
    let Some(attribute_key) = attribute_key else {
        return Ok(StaffRequestThresholdValue::Number(constructed_default));
    };
    let prefix = attribute_key.split_once('_').map(|(prefix, _)| prefix);
    match prefix {
        None | Some("f") => {
            let value = authored_value.map_or(Ok(constructed_default), |value| {
                parse_blue_fang_source_numeric_lexeme::<f32>(value)
                    .filter(|value| value.is_finite())
                    .ok_or_else(|| {
                        BindError::record(
                            record,
                            format!(
                                "authored staff request {field} {value:?} is not a finite number"
                            ),
                        )
                    })
            })?;
            Ok(StaffRequestThresholdValue::Number(value))
        }
        Some("b") => {
            let value = authored_value
                .map(|value| parse_authored_bool(record, field, value))
                .transpose()?
                .unwrap_or(false);
            Ok(StaffRequestThresholdValue::Boolean(value))
        }
        Some(prefix) => Err(BindError::record(
            record,
            format!(
                "authored staff request attribute key {attribute_key:?} has an unsupported {prefix:?} prefix; threshold {field} does not support this attribute type"
            ),
        )),
    }
}

fn parse_test_type(
    record: &RecordView<'_, '_>,
    authored_value: Option<&str>,
    constructed_default: i32,
) -> Result<StaffRequestThresholdComparison, BindError> {
    let test_type = authored_value.map_or(Ok(constructed_default), |value| {
        parse_blue_fang_source_numeric_lexeme::<i32>(value).ok_or_else(|| {
            BindError::record(
                record,
                format!("authored staff request testType {value:?} is not an integer"),
            )
        })
    })?;
    StaffRequestThresholdComparison::from_authored_test_type(test_type).ok_or_else(|| {
        BindError::record(
            record,
            format!(
                "authored staff request testType {test_type} is outside the supported comparison set 0..=4"
            ),
        )
    })
}

fn parse_authored_bool(
    record: &RecordView<'_, '_>,
    field: &str,
    value: &str,
) -> Result<bool, BindError> {
    match canonicalize_source_document_record_key(value).as_str() {
        "true" | "yes" | "1" => Ok(true),
        "false" | "no" | "0" => Ok(false),
        value => Err(BindError::record(
            record,
            format!("authored staff request {field} {value:?} is not a boolean"),
        )),
    }
}

fn lower_authored_staff_request_data(
    record: &RecordView<'_, '_>,
    controller: &'_ OrderedSourceDocumentNode,
) -> Result<StaffRequestData, BindError> {
    let mut request_nodes = controller.element_children().filter(|child| {
        source_document_names_are_semantically_equal(child.name.as_str(), REQUEST_DATA_COMPONENT)
    });
    let request_node = request_nodes.next();
    if request_nodes.next().is_some() {
        return Err(BindError::record(
            record,
            "authored staff request controller declares more than one ZTStaffRequestData child",
        ));
    }
    let Some(request_node) = request_node else {
        return Ok(StaffRequestData::CONSTRUCTED_DEFAULT);
    };
    let mut priority: Option<&str> = None;
    let mut token_key: Option<&str> = None;
    let mut subject_type: Option<&str> = None;
    let mut target: Option<&str> = None;
    let mut staff: Option<&str> = None;
    for (name, value) in request_node.attributes() {
        let attribute = REQUEST_DATA_ATTRIBUTES
            .iter()
            .find(|candidate| source_document_names_are_semantically_equal(candidate, name))
            .ok_or_else(|| {
                BindError::record(
                    record,
                    format!("authored ZTStaffRequestData attribute {name:?} is not supported"),
                )
            })?;
        match *attribute {
            "priority" => priority = Some(value),
            "tokenKey" => token_key = Some(value),
            "subjectType" => subject_type = Some(value),
            "target" => target = Some(value),
            "staff" => staff = Some(value),
            _ => unreachable!("attribute names are matched from the authored vocabulary"),
        }
    }
    let authored_reference =
        |value: Option<&str>| value.filter(|value| !value.trim().is_empty()).map(id);
    Ok(StaffRequestData {
        token: authored_reference(token_key),
        subject_type: authored_reference(subject_type),
        priority: priority.map_or(Ok(2.0), |value| {
            parse_blue_fang_source_numeric_lexeme::<f32>(value)
                .filter(|value| value.is_finite())
                .ok_or_else(|| {
                    BindError::record(
                        record,
                        format!("authored staff request priority {value:?} is not a finite number"),
                    )
                })
        })?,
        target: authored_reference(target),
        staff: authored_reference(staff),
    })
}

#[cfg(test)]
mod tests {
    use super::{
        canonicalize_source_document_record_key, id, StaffRequestThresholdComparison,
        StaffRequestThresholdValue,
    };
    use openzt2_game_data::world_definitions::document::WorldDefinitionDocument;

    use crate::assets::source_document::blue_fang_actor_manifest_model_and_scene_resolution_index::BlueFangActorManifestModelAndSceneResolutionIndex;
    use crate::assets::source_document::blue_fang_source_document_parsing::parse_blue_fang_source_document;
    use crate::assets::source_document::path::AssetPath;
    use crate::assets::world_definitions::world_definition_source_document_lowering::lower_resolved_world_definition_source_document_closure_to_canonical_document;

    fn lower_fixture(
        documents: &[(&str, String)],
        resolved_scenes: &[(&str, &str)],
        primary_path: &str,
    ) -> Result<Option<WorldDefinitionDocument>, String> {
        let documents = documents
            .iter()
            .map(|(path, source)| {
                parse_blue_fang_source_document(AssetPath::new(path), source.as_bytes())
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("{error:?}"))?;
        let mut scenes = BlueFangActorManifestModelAndSceneResolutionIndex::default();
        for (model, scene) in resolved_scenes {
            scenes.register_archive_resolved_actor_scene_asset_path(
                canonicalize_source_document_record_key(model).as_str(),
                (*scene).to_owned(),
            );
        }
        lower_resolved_world_definition_source_document_closure_to_canonical_document(
            &documents,
            &scenes,
            primary_path,
            [0, 0],
        )
        .map_err(|error| error.to_string())
    }

    const AUTHORED_FOOD_DISH: &str = r#"<BFTypedBinder binderType="FoodDish">
        <types><entity><food><FoodDish/></food></entity></types>
        <binder>
            <BFNamedBinder binderName="mainObj"><instance><BFPhysObj>
                <BFSimpleLODComponent modelfile="dish.nif"/>
            </BFPhysObj></instance></BFNamedBinder>
            <BFBinder required="1"><instance>
                <ZTAIStaffRequestController attribName="f_FoodLevel" thresholdValue="25" testType="1" cancelThresholdValue="50" cancelTestType="2">
                    <ZTStaffRequestData tokenKey="t_FillFoodContainer" subjectType="Keeper" priority="4"/>
                </ZTAIStaffRequestController>
            </instance></BFBinder>
        </binder>
    </BFTypedBinder>"#;

    #[test]
    fn food_dish_threshold_request_lowers_with_authored_values(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let document = lower_fixture(
            &[(
                "entities/objects/food/ai/FoodDish.xml",
                AUTHORED_FOOD_DISH.to_owned(),
            )],
            &[("dish.nif", "dish.nif#scene")],
            "entities/objects/food/ai/FoodDish.xml",
        )?
        .ok_or("fixture document was dropped")?;
        let requests = &document.staff_requests;
        assert_eq!(requests.len(), 1);
        let request = &requests[0];
        assert_eq!(request.id, id("FoodDish"));
        assert_eq!(request.binder, None);
        assert_eq!(request.attribute_key, Some(id("f_FoodLevel")));
        assert_eq!(request.threshold, StaffRequestThresholdValue::Number(25.0));
        assert_eq!(
            request.threshold_comparison,
            StaffRequestThresholdComparison::FallsBelow
        );
        assert_eq!(
            request.cancel_threshold,
            StaffRequestThresholdValue::Number(50.0)
        );
        assert_eq!(
            request.cancel_comparison,
            StaffRequestThresholdComparison::RisesAbove
        );
        assert!(!request.trigger_on_creation);
        assert!(!request.trigger_on_destruction);
        assert_eq!(request.request.token, Some(id("t_FillFoodContainer")));
        assert_eq!(request.request.subject_type, Some(id("Keeper")));
        assert_eq!(request.request.priority, 4.0);
        Ok(())
    }

    #[test]
    fn animal_family_requests_lower_through_inherited_binders(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let animal = r#"<BFTypedBinder binderType="animal" abstract="true">
            <types><entity><actor><animal/></actor></entity></types>
            <binder>
                <BFNamedBinder binderName="hunger" required="1"><instance>
                    <ZTAIStaffRequestController attribName="hunger" thresholdValue="70" testType="4" cancelThresholdValue="60" cancelTestType="1">
                        <ZTStaffRequestData tokenKey="t_FeedAnimal" subjectType="Keeper" priority="3"/>
                    </ZTAIStaffRequestController>
                </instance></BFNamedBinder>
                <BFNamedBinder binderName="escaped" required="1"><instance>
                    <ZTAIStaffRequestController attribName="b_Escaped" thresholdValue="true" testType="0" cancelThresholdValue="false" cancelTestType="0">
                        <ZTStaffRequestData tokenKey="t_CrateAnimal" subjectType="Keeper" priority="1"/>
                    </ZTAIStaffRequestController>
                </instance></BFNamedBinder>
            </binder>
        </BFTypedBinder>"#;
        let species = r#"<BFTypedBinder binderType="ZebraCommon" abstract="true">
            <types><entity><actor><animal><ZebraCommon/></animal></actor></entity></types>
        </BFTypedBinder>"#;
        let document = lower_fixture(
            &[
                ("entities/units/animals/ai/animal.xml", animal.to_owned()),
                (
                    "entities/units/animals/ai/ZebraCommon.xml",
                    species.to_owned(),
                ),
            ],
            &[],
            "entities/units/animals/ai/ZebraCommon.xml",
        )?
        .ok_or("fixture document was dropped")?;
        assert_eq!(document.staff_requests.len(), 2);
        let hunger = document
            .staff_requests
            .iter()
            .find(|request| request.request.token == Some(id("t_FeedAnimal")))
            .ok_or("hunger request absent")?;
        assert_eq!(hunger.id, id("ZebraCommon"));
        assert_eq!(hunger.binder, Some(id("hunger")));
        assert_eq!(hunger.attribute_key, Some(id("hunger")));
        assert_eq!(
            hunger.threshold_comparison,
            StaffRequestThresholdComparison::ReachesOrAbove
        );
        let escaped = document
            .staff_requests
            .iter()
            .find(|request| request.request.token == Some(id("t_CrateAnimal")))
            .ok_or("escaped request absent")?;
        assert_eq!(escaped.threshold, StaffRequestThresholdValue::Boolean(true));
        assert_eq!(
            escaped.threshold_comparison,
            StaffRequestThresholdComparison::EntersEquality
        );
        assert_eq!(
            escaped.cancel_threshold,
            StaffRequestThresholdValue::Boolean(false)
        );
        Ok(())
    }

    #[test]
    fn named_health_binders_merge_when_authored_names_differ(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let animal = r#"<BFTypedBinder binderType="animal" abstract="true">
            <types><entity><actor><animal/></actor></entity></types>
            <binder><BFNamedBinder binderName="sick"><instance>
                <ZTAIStaffRequestController attribName="health" thresholdValue="50" testType="4" cancelThresholdValue="40" cancelTestType="1">
                    <ZTStaffRequestData tokenKey="t_HealAnimal" subjectType="Keeper" priority="2"/>
                </ZTAIStaffRequestController>
            </instance></BFNamedBinder></binder>
        </BFTypedBinder>"#;
        let young = r#"<BFTypedBinder binderType="ZebraCommon_Young" abstract="true">
            <types><entity><actor><animal><ZebraCommon><ZebraCommon_Young/></ZebraCommon></animal></actor></entity></types>
            <binder><BFNamedBinder binderName="checkup"><instance>
                <ZTAIStaffRequestController attribName="health" thresholdValue="30" testType="4" cancelThresholdValue="20" cancelTestType="1">
                    <ZTStaffRequestData tokenKey="t_CheckupBaby" subjectType="Keeper" priority="5"/>
                </ZTAIStaffRequestController>
            </instance></BFNamedBinder></binder>
        </BFTypedBinder>"#;
        let document = lower_fixture(
            &[
                ("entities/units/animals/ai/animal.xml", animal.to_owned()),
                (
                    "entities/units/animals/ai/ZebraCommon_Young.xml",
                    young.to_owned(),
                ),
            ],
            &[],
            "entities/units/animals/ai/ZebraCommon_Young.xml",
        )?
        .ok_or("young document was dropped")?;
        assert_eq!(document.staff_requests.len(), 2);
        assert!(document.staff_requests.iter().any(|request| {
            request.binder == Some(id("sick")) && request.request.token == Some(id("t_HealAnimal"))
        }));
        assert!(document.staff_requests.iter().any(|request| {
            request.binder == Some(id("checkup"))
                && request.request.token == Some(id("t_CheckupBaby"))
        }));
        Ok(())
    }

    #[test]
    fn inherited_variant_request_is_retained_and_suppressed_records_drop_it(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let parent = r#"<BFTypedBinder binderType="FoodDish">
            <types><entity><food><FoodDish/></food></entity></types>
            <binder>
                <BFBinder><instance>
                    <ZTAIStaffRequestController attribName="f_FoodLevel" thresholdValue="25" testType="1" cancelThresholdValue="50" cancelTestType="2">
                        <ZTStaffRequestData tokenKey="t_FillFoodContainer" subjectType="Keeper" priority="4"/>
                    </ZTAIStaffRequestController>
                </instance></BFBinder>
            </binder>
        </BFTypedBinder>"#;
        let variant = r#"<BFTypedBinder binderType="FoodDish_Water">
            <types><entity><food><FoodDish><FoodDish_Water/></FoodDish></food></entity></types>
        </BFTypedBinder>"#;
        let suppressed_variant = variant.replace(
            "</BFTypedBinder>",
            "<binder><BFBinder><instance><ZTAIStaffRequestController triggerOnCreation=\"true\"><ZTStaffRequestData tokenKey=\"t_Own\" subjectType=\"Worker\" priority=\"1\"/></ZTAIStaffRequestController></instance></BFBinder></binder><suppressed><ZTAIStaffRequestController/></suppressed></BFTypedBinder>",
        );
        let inherited = lower_fixture(
            &[
                ("entities/objects/food/ai/FoodDish.xml", parent.to_owned()),
                (
                    "entities/objects/food/ai/FoodDish_Water.xml",
                    variant.to_owned(),
                ),
            ],
            &[],
            "entities/objects/food/ai/FoodDish_Water.xml",
        )?
        .ok_or("fixture document was dropped")?;
        let water_request = inherited
            .staff_requests
            .iter()
            .find(|request| request.id == id("FoodDish_Water"))
            .ok_or("inherited request absent")?;
        assert_eq!(water_request.request.token, Some(id("t_FillFoodContainer")));
        let suppressed = lower_fixture(
            &[
                ("entities/objects/food/ai/FoodDish.xml", parent.to_owned()),
                (
                    "entities/objects/food/ai/FoodDish_Water.xml",
                    suppressed_variant,
                ),
            ],
            &[],
            "entities/objects/food/ai/FoodDish_Water.xml",
        )?;
        let suppressed = suppressed.ok_or("own controller was incorrectly dropped")?;
        assert_eq!(suppressed.staff_requests.len(), 1);
        assert_eq!(
            suppressed.staff_requests[0].request.token,
            Some(id("t_Own"))
        );
        Ok(())
    }

    #[test]
    fn recycling_bin_archive_variants_retain_distinct_worker_tokens(
    ) -> Result<(), Box<dyn std::error::Error>> {
        fn source(token: &str) -> String {
            format!(
                r#"<BFTypedBinder binderType="RecyclingBin_df">
                <types><entity><object><RecyclingBin_df/></object></entity></types>
                <shared><ZTTransaction name="recycle" cost="1" type="debit" category="recycling"/></shared>
                <binder><BFNamedBinder binderName="mainObj"><instance><BFPhysObj>
                    <BFSimpleLODComponent modelfile="recycling-bin.nif"/>
                </BFPhysObj></instance></BFNamedBinder><BFBinder><instance>
                    <ZTAIStaffRequestController attribName="f_TrashLevel" thresholdValue="50" testType="2" cancelThresholdValue="25" cancelTestType="1">
                        <ZTStaffRequestData tokenKey="{token}" subjectType="Worker" priority="1"/>
                    </ZTAIStaffRequestController>
                </instance></BFBinder></binder>
            </BFTypedBinder>"#
            )
        }
        for (token, expected) in [
            ("t_EmptyTrash", "t_emptytrash"),
            ("t_EmptyRecyclingBin", "t_emptyrecyclingbin"),
        ] {
            let document = lower_fixture(
                &[(
                    "entities/objects/scenery/ai/RecyclingBin_df.xml",
                    source(token),
                )],
                &[("recycling-bin.nif", "recycling-bin.nif#scene")],
                "entities/objects/scenery/ai/RecyclingBin_df.xml",
            )?
            .ok_or("recycling-bin document was dropped")?;
            assert_eq!(document.staff_requests.len(), 1);
            assert_eq!(document.staff_requests[0].request.token, Some(id(expected)));
        }
        Ok(())
    }

    #[test]
    fn creation_only_controller_keeps_constructed_threshold_defaults(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let poop = r#"<BFTypedBinder binderType="Poop" abstract="true">
            <types><entity><item><Poop/></item></entity></types>
            <binder><BFBinder required="1"><instance>
                <ZTAIStaffRequestController triggerOnCreation="true">
                    <ZTStaffRequestData tokenKey="t_RakePoo" subjectType="Keeper" priority="6"/>
                </ZTAIStaffRequestController>
            </instance></BFBinder></binder>
        </BFTypedBinder>"#;
        let document = lower_fixture(
            &[("entities/objects/Items/ai/Poop.xml", poop.to_owned())],
            &[("poop.nif", "poop.nif#scene")],
            "entities/objects/Items/ai/Poop.xml",
        )?
        .ok_or("fixture document was dropped")?;
        let request = &document.staff_requests[0];
        assert_eq!(request.attribute_key, None);
        assert!(request.trigger_on_creation);
        assert_eq!(request.threshold, StaffRequestThresholdValue::Number(101.0));
        assert_eq!(
            request.cancel_threshold,
            StaffRequestThresholdValue::Number(0.0)
        );
        assert_eq!(
            request.threshold_comparison,
            StaffRequestThresholdComparison::ReachesOrAbove
        );
        assert_eq!(
            request.cancel_comparison,
            StaffRequestThresholdComparison::FallsBelow
        );
        assert_eq!(request.request.priority, 6.0);
        Ok(())
    }
}
