use super::lower;
use crate::source_document::{
    blue_fang_source_document_parsing::parse_blue_fang_source_document, path::AssetPath,
};

#[test]
fn ambient_manager_reads_the_authored_enable_switch() {
    for (attribute, enabled) in [
        (" On=\"true\"", true),
        (" On=\"1\"", true),
        (" On=\"false\"", false),
        (" On=\"0\"", false),
        (" On=\"TRUE\"", false),
        (" On=\"2\"", false),
        ("", false),
    ] {
        let bytes = format!("<ZTAIAmbientsMgr{attribute} MaxAmbients=\"10\"/>");
        let document =
            parse_blue_fang_source_document(AssetPath::new("ai/ambientsmgr.xml"), bytes.as_bytes())
                .unwrap();
        let policy = lower(&document, &|_, _| None).unwrap().ambient.unwrap();
        assert_eq!(policy.enabled, enabled, "{attribute}");
        assert_eq!(policy.voice_limit, 10);
    }
}
