use std::fs;

const UI_MODULE_PATH: &str = "src/ui/mod.rs";

#[test]
fn ui_source_no_longer_contains_plus_and_frontend_action_markers() {
    let source = fs::read_to_string(UI_MODULE_PATH).expect("read ui module source");

    assert!(!source.contains("OpenOzonePlus"));
    assert!(!source.contains("OpenOzonePlusSideBySide"));
    assert!(!source.contains("FrontendChoice"));
    assert!(!source.contains("FrontendMode"));
}
