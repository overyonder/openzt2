use super::source_element_tree_search::{
    authored_type_family_elements_named, find_descendant, find_presentation_component,
};
use super::world_definition_source_value_reading_and_conversion::{element_bool, id};
use crate::assets::source_document::blue_fang_source_numeric_lexeme::parse_blue_fang_source_numeric_lexeme;
#[cfg(test)]
use crate::assets::source_document::resolved_source_record_index::SourceIndex;
use crate::assets::source_document::resolved_source_record_index::{BindError, RecordView};
use crate::assets::source_document::source_document_semantic_name::{
    canonicalize_source_document_record_key, source_document_names_are_semantically_equal,
};
use openzt2_game_data::world_definitions::world_objects::WorldObjectPresentationAttachmentDefinition;
use openzt2_game_data::AssetId;
use std::collections::BTreeMap;

pub(super) fn lower_authored_world_object_presentation_controller_states(
    record: &RecordView<'_, '_>,
    actor_scene_paths: &BTreeMap<String, String>,
    entity_scene_paths: &BTreeMap<String, String>,
) -> Result<Vec<openzt2_game_data::world_definitions::world_objects::WorldObjectPresentationControllerDefinition>, BindError>{
    let named_binders = authored_type_family_elements_named(record, "BFNamedBinder");
    let mut controllers = Vec::new();

    for controller in authored_type_family_elements_named(record, "BFGPhysAnimController") {
        let initial_state = controller
            .attribute_named_any(&["currState"])
            .or_else(|| controller.attribute_named_any(&["defaultState"]))
            .map(id);
        let default_state = controller.attribute_named_any(&["defaultState"]).map(id);
        let Some(state_list) = find_descendant(controller, "stateList") else {
            continue;
        };
        let mut states = Vec::new();
        for active_state in state_list.element_children() {
            if !active_state
                .attribute_named_any(&["parent"])
                .is_some_and(|parent| {
                    source_document_names_are_semantically_equal(parent, "mainObj")
                })
                || !active_state
                    .attribute_named_any(&["postype"])
                    .is_some_and(|mode| source_document_names_are_semantically_equal(mode, "use"))
                || !active_state
                    .attribute_named_any(&["rottype"])
                    .is_some_and(|mode| {
                        source_document_names_are_semantically_equal(mode, "use")
                            || source_document_names_are_semantically_equal(mode, "ignore")
                    })
            {
                return Err(BindError::record(
                    record,
                    format!(
                        "physical animation state {:?} requires unsupported attachment transform policy",
                        active_state.name.as_str()
                    ),
                ));
            }
            let (Some(child), Some(parent_attachment)) = (
                active_state.attribute_named_any(&["child"]),
                active_state.attribute_named_any(&["attachnode"]),
            ) else {
                return Err(BindError::record(
                    record,
                    format!(
                        "physical animation state {:?} has no child or attachment node",
                        active_state.name.as_str()
                    ),
                ));
            };
            let child_binder = named_binders.iter().copied().find(|binder| {
                binder
                    .attribute_named_any(&["binderName"])
                    .is_some_and(|name| source_document_names_are_semantically_equal(name, child))
            });
            let Some(component) = child_binder.and_then(find_presentation_component) else {
                return Err(BindError::record(
                    record,
                    format!("physical animation child {child:?} has no presentation component"),
                ));
            };
            let model = component
                .attribute_named_any(&["modelfile", "actorfile"])
                .expect("presentation-component lookup requires a model reference");
            let resolved_scenes = if source_document_names_are_semantically_equal(
                component.name.as_str(),
                "BFActorComponent",
            ) {
                actor_scene_paths
            } else {
                entity_scene_paths
            };
            let resolved_scene = resolved_scenes
            .get(&canonicalize_source_document_record_key(model))
            .ok_or_else(|| {
                BindError::record(
                    record,
                    format!(
                        "active authored child binder {child:?} presentation {model:?} has no resolved model scene"
                    ),
                )
            })?;
            let period = |name| -> Result<f32, BindError> {
                active_state
                    .attribute_named_any(&[name])
                    .map_or(Ok(-1.0), |value| {
                        parse_blue_fang_source_numeric_lexeme::<f32>(value)
                            .filter(|value| value.is_finite())
                            .ok_or_else(|| {
                                BindError::record(
                                    record,
                                    format!("invalid physical animation {name}"),
                                )
                            })
                    })
            };
            states.push(WorldObjectPresentationAttachmentDefinition {
                state: id(active_state.name.as_str()),
                parent_attachment: id(parent_attachment),
                inherit_parent_rotation: active_state
                    .attribute_named_any(&["rottype"])
                    .is_some_and(|mode| source_document_names_are_semantically_equal(mode, "use")),
                prefab: AssetId::from_virtual_path(resolved_scene),
                child_attachment: active_state
                    .attribute_named_any(&["childattachnode"])
                    .filter(|value| !value.is_empty())
                    .map(id),
                minimum_period_seconds: period("minPeriod")?,
                maximum_period_seconds: period("maxPeriod")?,
                event_trigger: active_state
                    .attribute_named_any(&["eventTrigger"])
                    .filter(|value| !value.is_empty())
                    .map(id),
                child_animation: child_binder
                    .and_then(|binder| find_descendant(binder, "BFAnimatedObjectControllerComponent"))
                    .map(|controller| {
                        let duration = controller.attribute_named_any(&["duration"])
                            .map_or(Some(0.0), parse_blue_fang_source_numeric_lexeme::<f64>)
                            .filter(|duration| duration.is_finite())
                            .ok_or_else(|| BindError::record(record, "invalid child animation duration"))?;
                        let duration = std::time::Duration::try_from_secs_f64(duration.max(0.0))
                            .map_err(|_| BindError::record(record, "child animation duration exceeds supported range"))?;
                        Ok::<_, BindError>(openzt2_game_data::world_definitions::world_objects::WorldObjectChildAnimationDefinition {
                            duration,
                            auto_start: element_bool(&controller, &["autoStart"], false)?,
                            looping: element_bool(&controller, &["loop"], false)?,
                        })
                    }).transpose()?,
            });
        }
        for requested in [initial_state, default_state].into_iter().flatten() {
            if !states.iter().any(|state| state.state == requested) {
                return Err(BindError::record(
                    record,
                    "physical animation initial/default state is absent from stateList",
                ));
            }
        }
        controllers.push(openzt2_game_data::world_definitions::world_objects::WorldObjectPresentationControllerDefinition {
            initial_state, default_state, states,
            overrides_animation_setting: element_bool(&controller, &["overrideSettings"], false)?,
        });
    }
    Ok(controllers)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::source_document::blue_fang_source_document_parsing::parse_blue_fang_source_document;
    use crate::assets::source_document::path::AssetPath;
    #[test]
    fn physical_animation_keeps_inactive_sign_state_and_default_selection() {
        let documents = [parse_blue_fang_source_document(
            AssetPath::new("entities/objects/buildings/ai/test_stand.xml"),
            br#"<BFTypedBinder binderType="Test_Stand"><binder>
              <BFBinder><instance><BFGPhysAnimController currState="idle" defaultState="idle"><stateList>
                <idle parent="mainObj" child="signidle" rottype="use" postype="use" attachnode="Link_Sign"/>
                <used parent="mainObj" child="signused" rottype="use" postype="use" attachnode="Link_Sign"/>
              </stateList></BFGPhysAnimController></instance></BFBinder>
              <BFNamedBinder binderName="signidle"><instance><BFPhysObj>
                <BFSimpleLODComponent modelfile="idle.nif"/>
              </BFPhysObj></instance></BFNamedBinder>
              <BFNamedBinder binderName="signused"><instance><BFPhysObj>
                <BFSimpleLODComponent modelfile="used.nif"/>
                <BFAnimatedObjectControllerComponent autoStart="true" duration="1.32" loop="false"/>
              </BFPhysObj></instance></BFNamedBinder>
            </binder></BFTypedBinder>"#,
        ).expect("source document")];
        let index = SourceIndex::build(&documents).expect("source index");
        let record = index.find("test_stand").expect("stand");
        let scenes = BTreeMap::from([
            (
                canonicalize_source_document_record_key("idle.nif"),
                "idle.scene".to_owned(),
            ),
            (
                canonicalize_source_document_record_key("used.nif"),
                "used.scene".to_owned(),
            ),
        ]);
        let controllers = lower_authored_world_object_presentation_controller_states(
            &record,
            &BTreeMap::new(),
            &scenes,
        )
        .expect("controller states");
        assert_eq!(controllers.len(), 1);
        assert_eq!(controllers[0].initial_state, Some(id("idle")));
        assert_eq!(controllers[0].default_state, Some(id("idle")));
        assert_eq!(controllers[0].states.len(), 2);
        assert_eq!(controllers[0].states[1].state, id("used"));
        let animation = controllers[0].states[1]
            .child_animation
            .expect("authored child controller");
        assert_eq!(animation.duration, std::time::Duration::from_millis(1320));
        assert!(animation.auto_start);
        assert!(!animation.looping);
        assert_eq!(
            controllers[0].states[1].prefab,
            AssetId::from_virtual_path("used.scene")
        );
    }
}
