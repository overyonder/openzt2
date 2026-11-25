use openzt2_game_assets::{
    graphics_settings_presets::{GraphicsDetailPreset, GraphicsSettingsPresets},
    graphics_settings_types::GraphicsSettings,
};
use openzt2_game_data::AssetId;

#[test]
fn custom_choices_reuse_the_winning_preset_rows_in_source_order() {
    let table = GraphicsSettingsPresets::from_test_source(
        br#"<UserSettings><settings>
            <viewDistance labelLocID="options:viewdistance" settingWidget="View Distance Text"
                low="0" medium="1" high="2" min="0" max="2">
                <setting locid="options:low"/>
                <setting locid="options:medium"/>
                <setting locid="options:high"/>
            </viewDistance>
            <environmentDetail labelLocID="options:environmentdetail" settingWidget="Environment Text"
                low="0" medium="1" high="1" min="0" max="1">
                <setting locid="options:low">
                    <ambientsOn type="bool" value="true"/>
                    <skyboxQuality type="int" value="2"/>
                </setting>
                <setting locid="options:medium">
                    <ambientsOn type="bool" value="false"/>
                    <skyboxQuality type="int" value="1"/>
                </setting>
            </environmentDetail>
        </settings></UserSettings>"#,
    )
    .expect("authored graphics rows");

    let [distance, environment] = table.detail_rows() else {
        panic!("expected the two authored graphics rows");
    };
    assert_eq!(distance.label, AssetId::from_key("options:viewdistance"));
    assert_eq!(
        environment.value_label(1),
        AssetId::from_key("options:medium")
    );

    let mut graphics = GraphicsSettings::default();
    environment.apply_value(0, &mut graphics);
    assert!(graphics.ambient_effects);
    assert_eq!(graphics.skybox_quality, 2);
    environment.apply_value(1, &mut graphics);
    let preset = table.settings(GraphicsDetailPreset::Highest, graphics.multisample_count);
    assert_eq!(environment.selected_value(&preset), 1);
    assert_eq!(graphics.ambient_effects, preset.ambient_effects);
    assert_eq!(graphics.skybox_quality, preset.skybox_quality);
}
