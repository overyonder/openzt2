use std::collections::BTreeMap;

use openzt2_game_data::world_definitions::fences_and_gates::{
    FenceGatePolicy, FenceTraversalBlockingFlags, GateOpenCollisionException,
};

use crate::source_document::{
    resolved_source_record_index::{BindError, RecordView},
    source_document_semantic_name::source_document_names_are_semantically_equal,
};

use super::{
    source_element_tree_search::{authored_type_family_components, find_descendant},
    world_definition_source_value_reading_and_conversion::{
        id, parse, seconds_ns, source_distance_cm,
    },
};

pub(super) fn lower_authored_gate_policy<'document>(
    record: &RecordView<'_, 'document>,
) -> Result<Option<(&'document str, FenceGatePolicy)>, BindError> {
    let controllers = authored_type_family_components(record, "ZTGate");
    if controllers.is_empty() {
        return Ok(None);
    }
    let required = |name| {
        controllers
            .iter()
            .find_map(|controller| controller.attribute_named_any(&[name]))
            .filter(|value| !value.is_empty())
            .ok_or_else(|| BindError::record(record, format!("authored gate lacks {name}")))
    };
    let distance = |name| {
        let value = source_distance_cm(parse(required(name)?, record, name)?, record)?;
        u16::try_from(value)
            .map_err(|_| BindError::record(record, format!("invalid authored gate {name}")))
    };
    let trigger_distance_cm = distance("openDistance")?;
    let close_distance_cm = distance("closeDistance")?;
    let poll_interval_ns = seconds_ns(parse(required("pollFreq")?, record, "pollFreq")?, record)?;
    if trigger_distance_cm == 0 || close_distance_cm < trigger_distance_cm || poll_interval_ns == 0
    {
        return Err(BindError::record(
            record,
            "invalid authored gate distances or poll interval",
        ));
    }
    let mut conditional_scores = BTreeMap::new();
    let mut has_conditional_scores = false;
    for component in authored_type_family_components(record, "BFGCollisionData") {
        let Some(scores) = find_descendant(component, "typeAttrScores") else {
            continue;
        };
        for score in scores.element_children() {
            // Gate passage is conditioned on b_Open alone. Scores keyed on
            // other entity attributes, such as the b_Walkable scores tank
            // gates inherit from tankgate, are not part of the gate policy.
            if !score
                .attribute_named_any(&["attr"])
                .is_some_and(|attribute| {
                    source_document_names_are_semantically_equal(attribute, "b_Open")
                })
            {
                continue;
            }
            has_conditional_scores = true;
            let actor_type = id(score.name.as_str());
            if conditional_scores.contains_key(&actor_type) {
                continue;
            }
            let value: f64 = parse(
                score
                    .attribute_named_any(&["score"])
                    .ok_or_else(|| BindError::record(record, "gate collision lacks score"))?,
                record,
                "gate collision score",
            )?;
            if !value.is_finite() || value < 0.0 {
                return Err(BindError::record(
                    record,
                    "invalid authored gate collision score",
                ));
            }
            let exception = if value == 0.0 {
                let type_record = record
                    .find_resolved_source_record_by_reference(score.name.as_str())
                    .ok_or_else(|| {
                        BindError::record(record, "gate collision type has no authored ancestry")
                    })?;
                let mut bits = 0;
                for (name, flag) in [
                    ("guest", FenceTraversalBlockingFlags::GUEST),
                    ("staff", FenceTraversalBlockingFlags::STAFF),
                    ("animal", FenceTraversalBlockingFlags::ANIMAL),
                    ("vehicle", FenceTraversalBlockingFlags::VEHICLE),
                ] {
                    if type_record.has_type_token(name) {
                        bits |= flag.raw_flag_bits();
                    }
                }
                Some(GateOpenCollisionException {
                    entity_type: actor_type,
                    traversal_types: FenceTraversalBlockingFlags::from_raw_flag_bits(bits)
                        .expect("known traversal categories"),
                })
            } else {
                None
            };
            conditional_scores.insert(actor_type, exception);
        }
    }
    Ok(Some((
        required("physObj")?,
        FenceGatePolicy {
            open_animation: required("openAnim")?.to_owned(),
            close_animation: required("closeAnim")?.to_owned(),
            trigger_distance_cm,
            close_distance_cm,
            poll_interval_ns,
            open_for_entity_type: Some(id(required("openForEntityType")?)),
            open_passage_types: has_conditional_scores
                .then(|| conditional_scores.into_values().flatten().collect()),
            ..Default::default()
        },
    )))
}

#[cfg(test)]
mod tests {
    use crate::source_document::blue_fang_source_document_parsing::parse_blue_fang_source_document;
    use crate::source_document::path::AssetPath;
    use crate::source_document::resolved_source_record_index::SourceIndex;

    use super::*;

    #[test]
    fn tank_gate_keeps_inherited_open_passage_and_skips_walkable_scores() {
        let documents = [
            (
                "entities/units/staff/ai/staff.xml",
                r#"<BFTypedBinder binderType="Staff" abstract="true">
                    <types><entity><staff/></entity></types>
                </BFTypedBinder>"#,
            ),
            (
                "entities/objects/fences/ai/gate.xml",
                r#"<BFTypedBinder binderType="gate" abstract="true">
                    <types><entity><fence><gate/></fence></entity></types>
                    <shared><BFGCollisionData><typeAttrScores>
                        <Staff attr="b_Open" score="0"/>
                    </typeAttrScores></BFGCollisionData></shared>
                    <binder><BFBinder><instance>
                        <ZTGate openDistance="4" closeDistance="6" pollFreq="0.1" physObj="gate_animation" openAnim="Bip01_open" closeAnim="Bip01_close" openForEntityType="Staff"/>
                    </instance></BFBinder></binder>
                </BFTypedBinder>"#,
            ),
            (
                "entities/objects/fences/ai/tankgate.xml",
                r#"<BFTypedBinder binderType="tankgate" abstract="true">
                    <types><entity><fence><gate><tankgate/></gate></fence></entity></types>
                    <shared><BFGCollisionData isWaterBoundary="true"><typeAttrScores>
                        <animal attr="b_Walkable" score="0" alwaysValidate="true"/>
                        <guest attr="b_Walkable" score="10000" alwaysValidate="true"/>
                        <Staff attr="b_Walkable" score="0" alwaysValidate="true"/>
                    </typeAttrScores></BFGCollisionData></shared>
                </BFTypedBinder>"#,
            ),
            (
                "entities/objects/fences/ai/tankseg_gate_mm.xml",
                r#"<BFTypedBinder binderType="tankseg_gate_mm">
                    <types><entity><fence><gate><tankgate><tankseg_gate_mm/></tankgate></gate></fence></entity></types>
                </BFTypedBinder>"#,
            ),
        ]
        .map(|(path, source)| {
            parse_blue_fang_source_document(AssetPath::new(path), source.as_bytes())
                .expect("valid test document")
        });
        let index = SourceIndex::build(&documents).expect("valid source index");

        let (_, policy) =
            lower_authored_gate_policy(&index.find("tankseg_gate_mm").expect("indexed tank gate"))
                .expect("tank gate lowers")
                .expect("tank gate has a gate policy");

        assert_eq!(
            policy.open_passage_types,
            Some(vec![GateOpenCollisionException {
                entity_type: id("Staff"),
                traversal_types: FenceTraversalBlockingFlags::STAFF,
            }])
        );
    }
}
