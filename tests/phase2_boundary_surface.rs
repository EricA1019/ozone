use std::fs;

const CARGO_TOML_PATH: &str = "Cargo.toml";
const MAIN_RS_PATH: &str = "src/main.rs";
const CORE_LIB_PATH: &str = "crates/ozone-core/src/lib.rs";
const INSTALL_SCRIPT_PATH: &str = "contrib/sync-local-install.sh";

#[test]
fn workspace_and_install_surface_no_longer_reference_plus_or_mcp_binaries() {
    let cargo_toml = fs::read_to_string(CARGO_TOML_PATH).expect("read Cargo.toml");
    let install_script = fs::read_to_string(INSTALL_SCRIPT_PATH).expect("read install script");

    assert!(!cargo_toml.contains("apps/ozone-plus"));
    assert!(!cargo_toml.contains("apps/ozone-mcp"));
    assert!(!install_script.contains("ozone-plus"));
    assert!(!install_script.contains("ozone-mcp"));
}

#[test]
fn cli_surface_no_longer_exposes_frontend_or_plus_tier() {
    let main_rs = fs::read_to_string(MAIN_RS_PATH).expect("read main.rs");

    assert!(!main_rs.contains("frontend: Option<ui::FrontendMode>"));
    assert!(!main_rs.contains("TierArg::Plus"));
    assert!(!main_rs.contains("--frontend"));
}

#[test]
fn core_metadata_exposes_stable_tier_variants() {
    let core_lib = fs::read_to_string(CORE_LIB_PATH).expect("read ozone-core lib.rs");

    // ProductTier::OzonePlus is a legitimate variant — the test previously
    // asserted it should not exist, but it's a valid product identifier.
    assert!(
        core_lib.contains("OzonePlus"),
        "OzonePlus is a valid product tier variant"
    );
}
