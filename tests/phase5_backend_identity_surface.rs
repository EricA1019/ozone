use std::fs;

const UI_MOD_RS_PATH: &str = "src/ui/mod.rs";

#[test]
fn backend_identity_surface_is_llamacpp_only() {
    let ui_mod_rs = fs::read_to_string(UI_MOD_RS_PATH).expect("read ui/mod.rs");

    assert!(ui_mod_rs.contains("pub enum BackendMode"));
    assert!(ui_mod_rs.contains("LlamaCpp"));
    assert!(!ui_mod_rs.contains("KoboldCpp"));
    assert!(!ui_mod_rs.contains("Ollama"));
}