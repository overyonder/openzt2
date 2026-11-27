use super::*;

#[test]
fn only_the_mode_declares_an_economy_configuration() {
    for (source, expected) in [
        (
            r#"<BFContext><ZTEconomyMgr version="0"><ZTEconomyComponent category="generalEconomyComponent"/></ZTEconomyMgr></BFContext>"#,
            None,
        ),
        (
            r#"<BFContext><ZTEconomyMgr configData="config/economy_freeform.xml" initialCash="10000"/></BFContext>"#,
            Some("config/economy_freeform.xml"),
        ),
    ] {
        let document =
            parse_blue_fang_source_document(AssetPath::new("context.xml"), source.as_bytes())
                .unwrap();
        assert_eq!(mode_economy_configuration_reference(&document), expected);
    }
}
