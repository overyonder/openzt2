use super::source_element_tree_search::{authored_type_family_elements_named, find_descendant};
use super::world_definition_source_value_reading_and_conversion::{element_bool, id};
use crate::assets::source_document::blue_fang_source_numeric_lexeme::parse_blue_fang_source_numeric_lexeme;
#[cfg(test)]
use crate::assets::source_document::resolved_source_record_index::SourceIndex;
use crate::assets::source_document::resolved_source_record_index::{BindError, RecordView};
use crate::assets::source_document::source_document_semantic_name::source_document_names_are_semantically_equal;
use openzt2_game_data::world_definitions::world_objects::WorldObjectInteractionSlotDefinition;
use openzt2_game_data::AssetId;

pub(super) fn lower_authored_world_object_interaction_slots(
    record: &RecordView<'_, '_>,
) -> Result<Vec<WorldObjectInteractionSlotDefinition>, BindError> {
    let mut slots = Vec::new();
    let binders = authored_type_family_elements_named(record, "BFNamedBinder")
        .into_iter()
        .chain(authored_type_family_elements_named(record, "BFBinder"));
    for binder in binders {
        let Some(container) = find_descendant(binder, "BFGEntityContainer") else {
            continue;
        };
        let reservation_tag = if source_document_names_are_semantically_equal(
            binder.name.as_str(),
            "BFNamedBinder",
        ) {
            id(binder.attribute_named_any(&["binderName"]).ok_or_else(|| {
                BindError::record(record, "interaction container has no binderName")
            })?)
        } else {
            AssetId::default()
        };
        for group in container.element_children().filter(|group| {
            source_document_names_are_semantically_equal(group.name.as_str(), "slots")
                || source_document_names_are_semantically_equal(group.name.as_str(), "queue")
        }) {
            for slot in group.element_children() {
                if ![
                    "BFGEntityContainerSlot",
                    "BFGEntityContainerSlotLight",
                    "BFGEntityContainerSlotHeavy",
                ]
                .iter()
                .any(|name| source_document_names_are_semantically_equal(slot.name.as_str(), name))
                {
                    return Err(BindError::record(
                        record,
                        format!("unmapped container slot {}", slot.name.as_str()),
                    ));
                }
                let capacity = slot
                    .attribute_named_any(&["capacity"])
                    .map(|value| {
                        parse_blue_fang_source_numeric_lexeme::<i32>(value).ok_or_else(|| {
                            BindError::record(record, "invalid interaction slot capacity")
                        })
                    })
                    .transpose()?
                    .unwrap_or(1)
                    .clamp(0, i32::from(u8::MAX));
                slots.push(WorldObjectInteractionSlotDefinition {
                    reservation_tag,
                    is_queue: source_document_names_are_semantically_equal(
                        group.name.as_str(),
                        "queue",
                    ),
                    capacity: u8::try_from(capacity).expect("capacity was clamped to a byte"),
                    exclusive_identifier: slot
                        .attribute_named_any(&["exclusiveID"])
                        .map(id)
                        .unwrap_or_default(),
                    owns_contents: element_bool(&slot, &["ownContents"], false)?,
                    hides_contents: element_bool(&slot, &["hideContents"], false)?,
                    entrance_behavior_set: slot
                        .attribute_named_any(&["enterBehSet"])
                        .map(id)
                        .unwrap_or_default(),
                    use_behavior_set: slot
                        .attribute_named_any(&["useBehSet"])
                        .map(id)
                        .unwrap_or_default(),
                    exit_behavior_set: slot
                        .attribute_named_any(&["exitBehSet"])
                        .map(id)
                        .unwrap_or_default(),
                    target_node_name: slot
                        .attribute_named_any(&["targetNode"])
                        .unwrap_or_default()
                        .trim()
                        .to_owned(),
                });
            }
        }
    }
    Ok(slots)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::source_document::blue_fang_source_document_parsing::parse_blue_fang_source_document;
    use crate::assets::source_document::path::AssetPath;

    #[test]
    fn actor_keeps_inherited_default_container_and_named_inventory() {
        let documents = [
            ("entities/entity.xml", br#"<BFTypedBinder binderType="entity" abstract="true"><binder><BFBinder required="1"><instance><BFGEntityContainer><slots><BFGEntityContainerSlot/></slots></BFGEntityContainer></instance></BFBinder></binder></BFTypedBinder>"#.as_slice()),
            ("entities/actor.xml", br#"<BFTypedBinder binderType="TestActor"><types><entity><TestActor/></entity></types><binder><BFNamedBinder binderName="inventory"><instance><BFGEntityContainer><slots><BFGEntityContainerSlot ownContents="true" hideContents="true" capacity="100"/></slots></BFGEntityContainer></instance></BFNamedBinder></binder></BFTypedBinder>"#.as_slice()),
        ].map(|(path, source)| parse_blue_fang_source_document(AssetPath::new(path), source).unwrap());
        let index = SourceIndex::build(&documents).unwrap();
        let slots =
            lower_authored_world_object_interaction_slots(&index.find("TestActor").unwrap())
                .unwrap();
        assert_eq!(slots.len(), 2);
        let inventory = slots
            .iter()
            .find(|slot| slot.reservation_tag == id("inventory"))
            .unwrap();
        assert!(inventory.owns_contents && inventory.hides_contents);
        assert_eq!(inventory.capacity, 100);
        let default = slots
            .iter()
            .find(|slot| slot.reservation_tag == AssetId::default())
            .unwrap();
        assert_eq!(default.capacity, 1);
        assert!(!default.owns_contents && !default.hides_contents);
    }

    #[test]
    fn shop_service_and_queue_slots_survive_absent_behavior_names() {
        let documents = [parse_blue_fang_source_document(
            AssetPath::new("entities/objects/buildings/ai/test_stand.xml"),
            br#"<BFTypedBinder binderType="Test_Stand"><binder>
              <BFNamedBinder binderName="Use_Stand"><instance><BFGEntityContainer>
                <slots><BFGEntityContainerSlot targetNode="Dock_Adult" capacity="1"/></slots>
                <queue><BFGEntityContainerSlot targetNode="Dock_Adult" capacity="5"/></queue>
              </BFGEntityContainer></instance></BFNamedBinder>
            </binder></BFTypedBinder>"#,
        )
        .expect("source document")];
        let index = SourceIndex::build(&documents).expect("source index");
        let record = index.find("test_stand").expect("stand");
        let slots = lower_authored_world_object_interaction_slots(&record).expect("slots");
        assert_eq!(slots.len(), 2);
        assert!(!slots[0].is_queue);
        assert_eq!(slots[0].capacity, 1);
        assert!(slots[1].is_queue);
        assert_eq!(slots[1].capacity, 5);
        for slot in slots {
            assert_eq!(slot.reservation_tag, id("Use_Stand"));
            assert_eq!(slot.target_node_name, "Dock_Adult");
            assert_eq!(slot.entrance_behavior_set, AssetId::default());
            assert_eq!(slot.use_behavior_set, AssetId::default());
        }
    }

    #[test]
    fn slot_defaults_limits_and_content_policies_match_source() {
        let documents = [parse_blue_fang_source_document(
            AssetPath::new("entities/objects/buildings/ai/test_stand.xml"),
            br#"<BFTypedBinder binderType="Test_Stand"><binder>
              <BFNamedBinder binderName="Use_Stand"><instance><BFGEntityContainer><slots>
                <BFGEntityContainerSlot/>
                <BFGEntityContainerSlotLight capacity="-1"/>
                <BFGEntityContainerSlotHeavy capacity="999" ownContents="true" hideContents="true" exclusiveID="seat" enterBehSet="Enter" useBehSet="Use" exitBehSet="Exit"/>
              </slots></BFGEntityContainer></instance></BFNamedBinder>
            </binder></BFTypedBinder>"#,
        ).expect("source document")];
        let index = SourceIndex::build(&documents).expect("source index");
        let record = index.find("test_stand").expect("stand");
        let slots = lower_authored_world_object_interaction_slots(&record).expect("slots");
        assert_eq!(slots.len(), 3);
        assert_eq!(slots[0].capacity, 1);
        assert!(slots[0].target_node_name.is_empty());
        assert_eq!(slots[1].capacity, 0);
        assert_eq!(slots[2].capacity, u8::MAX);
        assert!(slots[2].owns_contents && slots[2].hides_contents);
        assert_eq!(slots[2].exclusive_identifier, id("seat"));
        assert_eq!(slots[2].entrance_behavior_set, id("Enter"));
        assert_eq!(slots[2].use_behavior_set, id("Use"));
        assert_eq!(slots[2].exit_behavior_set, id("Exit"));
    }
}
