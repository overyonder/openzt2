use crate::source_document::{
    blue_fang_actor_manifest_model_and_scene_resolution_index::BlueFangActorManifestModelAndSceneResolutionIndex,
    blue_fang_source_document_parsing::parse_blue_fang_source_document, path::AssetPath,
};
use crate::world_definitions::world_definition_source_document_lowering::lower_resolved_world_definition_source_document_closure_to_canonical_document;
use openzt2_game_data::AssetId;

#[test]
fn family_purchase_documents_publish_species_and_preserve_concrete_scene_references(
) -> Result<(), Box<dyn std::error::Error>> {
    // Separate source documents repeat a family hierarchy or inherit it. Each
    // concrete binder owns a button, while the species owns the shared offer.
    for explicit_inheritance in [false, true] {
        for species_has_placement_control in [false, true] {
            let placement_control = if species_has_placement_control {
                r#"<on><event msg="ZT_SETPLACEMENTOBJECT" string="TestSpecies_Adult_F"/></on>"#
            } else {
                ""
            };
            let mut sources = vec![(
                "entities/units/animals/ai/testspecies.xml".to_owned(),
                format!(
                    r#"<BFTypedBinder binderType="TestSpecies" abstract="true">
                    <types><entity><animal><TestSpecies/></animal></entity></types>
                    <shared><BFAIEntityDataShared s_Species="TestSpecies"/>
                        <UIToggleButton><UIHelpInfo ids="entityname:TestSpecies"/>
                            <UIAspect><default image="species.dds"/></UIAspect>{placement_control}</UIToggleButton>
                        <ZTEconomyComponent cost="12.50"/></shared>
                    <binder><BFNamedBinder binderName="mainObj"><instance><BFPhysObj>
                        <BFActorComponent actorfile="body.bfm"/>
                    </BFPhysObj></instance></BFNamedBinder></binder>
                </BFTypedBinder>"#
                ),
            )];
            for (key, male) in [
                ("TestSpecies_Adult_F", "false"),
                ("TestSpecies_Adult_M", "true"),
            ] {
                let inheritance = if explicit_inheritance {
                    r#" extends="TestSpecies""#.to_owned()
                } else {
                    String::new()
                };
                let types = if explicit_inheritance {
                    format!("<types><{key}/></types>")
                } else {
                    format!("<types><entity><animal><TestSpecies><{key}/></TestSpecies></animal></entity></types>")
                };
                sources.push((
                    format!("entities/units/animals/ai/{}.xml", key.to_ascii_lowercase()),
                    format!(r#"<BFTypedBinder binderType="{key}"{inheritance}>
                        {types}<shared><BFAIEntityDataShared b_Male="{male}"/>
                            <UIToggleButton><UIAspect><default image="species.dds"/></UIAspect>
                                <on><event msg="ZT_SETPLACEMENTOBJECT" string="{key}"/></on></UIToggleButton></shared>
                        <binder><BFNamedBinder binderName="mainObj"><instance><BFPhysObj>
                            <BFActorComponent actorfile="body.bfm"/>
                        </BFPhysObj></instance></BFNamedBinder></binder>
                    </BFTypedBinder>"#),
                ));
            }
            sources.push((
                "entities/units/animals/ai/orphan_adult_f.xml".to_owned(),
                r#"<BFTypedBinder binderType="Orphan_Adult_F">
                    <types><entity><animal><Orphan><Orphan_Adult_F/></Orphan></animal></entity></types>
                    <shared><BFAIEntityDataShared s_Species="Orphan" b_Male="false"/>
                        <UIToggleButton><UIAspect><default image="orphan.dds"/></UIAspect>
                            <on><event msg="ZT_SETPLACEMENTOBJECT" string="Orphan_Adult_F"/></on></UIToggleButton></shared>
                    <binder><BFNamedBinder binderName="mainObj"><instance><BFPhysObj>
                        <BFActorComponent actorfile="body.bfm"/>
                    </BFPhysObj></instance></BFNamedBinder></binder>
                </BFTypedBinder>"#.to_owned(),
            ));
            let documents = sources
                .iter()
                .map(|(path, xml)| {
                    parse_blue_fang_source_document(AssetPath::new(path), xml.as_bytes())
                })
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| std::io::Error::other(format!("{error:?}")))?;
            let mut scenes = BlueFangActorManifestModelAndSceneResolutionIndex::default();
            scenes.register_archive_resolved_actor_scene_asset_path(
                "body.bfm",
                "body.nif#scene".into(),
            );
            let lowered = sources
                .iter()
                .enumerate()
                .map(|(index, (path, _))| {
                    let closure = if index == 1 || index == 2 {
                        vec![documents[0].clone(), documents[index].clone()]
                    } else {
                        vec![documents[index].clone()]
                    };
                    lower_resolved_world_definition_source_document_closure_to_canonical_document(
                        &closure,
                        &scenes,
                        path,
                        [0, 0],
                    )?
                    .ok_or_else(|| std::io::Error::other("purchase source produced no document"))
                })
                .collect::<Result<Vec<_>, _>>()?;
            let published = lowered
                .iter()
                .flat_map(|document| &document.catalogue)
                .map(|entry| entry.definition)
                .collect::<Vec<_>>();
            let expected = if species_has_placement_control {
                vec![
                    AssetId::from_key("testspecies"),
                    AssetId::from_key("orphan_adult_f"),
                ]
            } else {
                vec![
                    AssetId::from_key("testspecies_adult_f"),
                    AssetId::from_key("testspecies_adult_m"),
                    AssetId::from_key("orphan_adult_f"),
                ]
            };
            assert_eq!(published, expected);
            for (path, key) in sources.iter().skip(1).zip([
                "testspecies_adult_f",
                "testspecies_adult_m",
                "orphan_adult_f",
            ]) {
                let object = lowered
                    .iter()
                    .flat_map(|document| &document.objects)
                    .find(|object| object.id == AssetId::from_key(key))
                    .ok_or_else(|| {
                        std::io::Error::other(format!("{} lost its concrete object", path.0))
                    })?;
                assert_eq!(object.prefab, AssetId::from_virtual_path("body.nif#scene"));
            }
            if species_has_placement_control {
                let species = lowered[0]
                    .objects
                    .first()
                    .ok_or_else(|| std::io::Error::other("species offer has no object"))?;
                assert_eq!(species.id, AssetId::from_key("testspecies"));
                assert_eq!(species.price_cents, 1250);
                assert_eq!(
                    species.name_key,
                    AssetId::from_key("entityname:testspecies")
                );
            }
        }
    }
    Ok(())
}

#[test]
fn descendant_actor_closures_retain_offers_when_the_species_has_no_own_presentation(
) -> Result<(), Box<dyn std::error::Error>> {
    use crate::source_document::{
        authored_species_purchase_family_queries::find_authored_species_purchase_family_root,
        resolved_source_record_index::SourceIndex,
    };

    for abstract_value in ["true", "yes"] {
        let mut sources = vec![(
            "entities/units/animals/ai/testspecies.xml".to_owned(),
            format!(
                r#"<BFTypedBinder binderType="TestSpecies" abstract="{abstract_value}">
                <types><entity><animal><TestSpecies/></animal></entity></types>
                <shared><BFAIEntityDataShared s_Species="TestSpecies"/>
                    <UIToggleButton><UIAspect><default image="species.dds"/></UIAspect>
                        <on><event msg="ZT_SETPLACEMENTOBJECT" string="TestSpecies_Adult_F"/></on>
                    </UIToggleButton></shared>
            </BFTypedBinder>"#
            ),
        )];
        for (key, male, actor) in [
            ("TestSpecies_Adult_F", "false", "female"),
            ("TestSpecies_Adult_M", "true", "male"),
        ] {
            sources.push((
                format!("entities/units/animals/ai/{}.xml", key.to_ascii_lowercase()),
                format!(
                    r#"<BFTypedBinder binderType="{key}" extends="TestSpecies">
                    <types><{key}/></types>
                    <shared><BFAIEntityDataShared s_Species="TestSpecies" b_Male="{male}"/>
                        <UIToggleButton><UIAspect><default image="species.dds"/></UIAspect>
                            <on><event msg="ZT_SETPLACEMENTOBJECT" string="{key}"/></on>
                        </UIToggleButton></shared>
                    <binder><BFNamedBinder binderName="mainObj"><instance><BFPhysObj>
                        <BFActorComponent actorfile="{actor}.bfm"/>
                    </BFPhysObj></instance></BFNamedBinder></binder>
                </BFTypedBinder>"#
                ),
            ));
        }
        let documents = sources
            .iter()
            .map(|(path, xml)| {
                parse_blue_fang_source_document(AssetPath::new(path), xml.as_bytes())
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| std::io::Error::other(format!("{error:?}")))?;
        let mut scenes = BlueFangActorManifestModelAndSceneResolutionIndex::default();
        for actor in ["female", "male"] {
            scenes.register_archive_resolved_actor_scene_asset_path(
                &format!("{actor}.bfm"),
                format!("{actor}.nif#scene"),
            );
        }
        // Roots discover ancestors, not animal descendants. Child closures add
        // their own actor without publishing their dependency's declarations.
        let root = lower_resolved_world_definition_source_document_closure_to_canonical_document(
            &documents[..1],
            &scenes,
            &sources[0].0,
            [0, 0],
        )?
        .ok_or_else(|| std::io::Error::other("species root produced no document"))?;
        if abstract_value == "true" {
            assert!(root.catalogue.is_empty());
            assert!(root.objects.is_empty());
        } else {
            assert_eq!(root.catalogue.len(), 1);
            assert_eq!(
                root.catalogue[0].definition,
                AssetId::from_key("testspecies")
            );
            assert_eq!(root.objects.len(), 1);
            assert_eq!(root.objects[0].prefab, AssetId::default());
            assert!(root.objects[0].named_physical_presentations.is_empty());
        }
        for (index, key, actor) in [
            (1, "testspecies_adult_f", "female"),
            (2, "testspecies_adult_m", "male"),
        ] {
            let closure = [documents[0].clone(), documents[index].clone()];
            let source_index = SourceIndex::build(&closure)?;
            let child = source_index
                .document_root(&sources[index].0)
                .ok_or_else(|| std::io::Error::other("child source record missing"))?;
            let species =
                find_authored_species_purchase_family_root(&child, source_index.records())
                    .ok_or_else(|| {
                        std::io::Error::other("child lost its authored species family")
                    })?;
            assert_eq!(species.key, "testspecies");
            let child =
                lower_resolved_world_definition_source_document_closure_to_canonical_document(
                    &closure,
                    &scenes,
                    &sources[index].0,
                    [0, 0],
                )?
                .ok_or_else(|| std::io::Error::other("child source produced no document"))?;
            assert_eq!(child.catalogue.len(), 1);
            assert_eq!(child.catalogue[0].definition, AssetId::from_key(key));
            assert_eq!(child.objects.len(), 1);
            assert_eq!(child.objects[0].id, AssetId::from_key(key));
            assert_eq!(
                child.objects[0].prefab,
                AssetId::from_virtual_path(&format!("{actor}.nif#scene"))
            );
        }
    }
    Ok(())
}
