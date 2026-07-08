//! Surface-level regression tests for legacy-backend removal and RC scope.
//!
//! These verify that deprecated ozone+ chat, KoboldCpp, and Ollama references
//! are not present in the current codebase. They are gate tests that prevent
//! accidental re-introduction during refactoring.
//!
//! Behavioral equivalent tests exist as inline unit tests in their respective
//! modules (e.g., `BackendMode::deserialize` is tested in `src/ui/mod.rs`).

use std::fs;

use ozone::ui::BackendMode;

// -- File paths used by the surface checks --
const CARGO_TOML_PATH: &str = "Cargo.toml";
const LIB_RS_PATH: &str = "src/lib.rs";
const MAIN_RS_PATH: &str = "src/main.rs";
const INSTALL_SCRIPT_PATH: &str = "contrib/sync-local-install.sh";
const UI_MOD_RS_PATH: &str = "src/ui/mod.rs";
const BENCH_RS_PATH: &str = "src/bench.rs";
const PROFILING_RS_PATH: &str = "src/profiling.rs";
const ANALYZE_RS_PATH: &str = "src/analyze.rs";
const LAUNCHER_RS_PATH: &str = "src/ui/launcher.rs";
const MONITOR_RS_PATH: &str = "src/ui/monitor.rs";

// ---------------------------------------------------------------------------
// Phase 2 boundary: deprecated ozone+ binary and CLI surface
// ---------------------------------------------------------------------------

#[test]
fn workspace_and_install_no_longer_reference_ozone_plus() {
    let cargo_toml = fs::read_to_string(CARGO_TOML_PATH).expect("read Cargo.toml");
    let install_script = fs::read_to_string(INSTALL_SCRIPT_PATH).expect("read install script");

    assert!(!cargo_toml.contains("apps/ozone-plus"));
    assert!(!install_script.contains("ozone-plus"));
    assert!(cargo_toml.contains("apps/ozone-mcp"), "ozone-mcp should stay in workspace");
    assert!(install_script.contains("ozone-mcp"), "install script should keep ozone-mcp");
}

#[test]
fn cli_surface_no_longer_exposes_frontend_or_plus_tier() {
    let main_rs = fs::read_to_string(MAIN_RS_PATH).expect("read main.rs");
    assert!(!main_rs.contains("frontend: Option<ui::FrontendMode>"));
    assert!(!main_rs.contains("TierArg::Plus"));
    assert!(!main_rs.contains("--frontend"));
}

#[test]
fn ui_source_no_longer_contains_plus_action_markers() {
    let source = fs::read_to_string(UI_MOD_RS_PATH).expect("read ui/mod.rs");
    assert!(!source.contains("OpenOzonePlus"));
    assert!(!source.contains("OpenOzonePlusSideBySide"));
    assert!(!source.contains("FrontendChoice"));
    assert!(!source.contains("FrontendMode"));
}

// ---------------------------------------------------------------------------
// Phase 3 runtime: active CLI commands and backend surface
// ---------------------------------------------------------------------------

#[test]
fn cli_surface_exposes_purge_last_model() {
    let lib_rs = fs::read_to_string(LIB_RS_PATH).expect("read lib.rs");
    assert!(lib_rs.contains("PurgeLastModel"));
}

#[test]
fn launcher_and_monitor_only_reference_managed_llamacpp() {
    for path in [LAUNCHER_RS_PATH, MONITOR_RS_PATH] {
        let content = fs::read_to_string(path).expect("read file");
        for legacy in ["KoboldCpp", "Ollama", "SillyTavern", ":8080", ":11434", ":8000"] {
            assert!(!content.contains(legacy), "{path} still contains {legacy}");
        }
        assert!(content.contains("DEFAULT_LLAMACPP_PORT"), "{path} missing DEFAULT_LLAMACPP_PORT");
    }
}

// ---------------------------------------------------------------------------
// Phase 4 bench: native benchmark backend identity
// ---------------------------------------------------------------------------

#[test]
fn native_benchmark_surface_is_llamacpp_only() {
    for path in [BENCH_RS_PATH, MAIN_RS_PATH, PROFILING_RS_PATH] {
        let content = fs::read_to_string(path).expect("read file");
        assert!(!content.contains("BenchBackend::KoboldCpp"), "{path} still contains KoboldCpp");
    }
    let bench_rs = fs::read_to_string(BENCH_RS_PATH).expect("read bench.rs");
    assert!(bench_rs.contains("DEFAULT_LLAMACPP_PORT"));
}

#[test]
fn export_surface_drops_stale_kobold_labels() {
    for path in [MAIN_RS_PATH, ANALYZE_RS_PATH, PROFILING_RS_PATH] {
        let content = fs::read_to_string(path).expect("read file");
        for stale in [
            "Export profiles to koboldcpp-presets.conf",
            "Write the best profile into koboldcpp-presets.conf.",
            "Exporting KoboldCpp presets",
            "export it to koboldcpp-presets.conf",
            "KoboldCpp install is broken",
            "KoboldCpp never became ready",
        ] {
            assert!(!content.contains(stale), "{path} still contains: {stale}");
        }
    }
}

// ---------------------------------------------------------------------------
// Phase 5 backend identity: BackendMode enum surface
// ---------------------------------------------------------------------------

#[test]
fn backend_identity_surface_is_llamacpp_only() {
    let ui_mod_rs = fs::read_to_string(UI_MOD_RS_PATH).expect("read ui/mod.rs");
    assert!(ui_mod_rs.contains("pub enum BackendMode"));
    assert!(ui_mod_rs.contains("LlamaCpp"));
    assert!(!ui_mod_rs.contains("KoboldCpp"));
    assert!(!ui_mod_rs.contains("Ollama"));
}

// ---------------------------------------------------------------------------
// Behavioral regression: BackendMode deserialization maps legacy to LlamaCpp
// ---------------------------------------------------------------------------

#[test]
fn backend_deserialize_legacy_koboldcpp_maps_to_llamacpp() {
    let result: Result<BackendMode, _> = serde_json::from_str("\"kobold-cpp\"");
    assert!(matches!(result, Ok(BackendMode::LlamaCpp)));
}

#[test]
fn backend_deserialize_legacy_ollama_maps_to_llamacpp() {
    let result: Result<BackendMode, _> = serde_json::from_str("\"ollama\"");
    assert!(matches!(result, Ok(BackendMode::LlamaCpp)));
}

#[test]
fn backend_deserialize_llamacpp_works() {
    let result: Result<BackendMode, _> = serde_json::from_str("\"llama-cpp\"");
    assert!(matches!(result, Ok(BackendMode::LlamaCpp)));
}

#[test]
fn backend_deserialize_unknown_variant_returns_error() {
    let result: Result<BackendMode, _> = serde_json::from_str("\"silly-tavern\"");
    assert!(result.is_err());
}
