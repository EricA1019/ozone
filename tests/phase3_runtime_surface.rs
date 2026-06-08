use std::fs;

const MAIN_RS_PATH: &str = "src/main.rs";
const LAUNCH_EXECUTION_FLOW_PATH: &str = "src/ui/launch_execution_flow.rs";
const LAUNCHER_RS_PATH: &str = "src/ui/launcher.rs";
const MONITOR_RS_PATH: &str = "src/ui/monitor.rs";
const SETTINGS_SCREEN_FLOW_PATH: &str = "src/ui/settings_screen_flow.rs";

#[test]
fn cli_surface_exposes_purge_last_model_command() {
    let main_rs = fs::read_to_string(MAIN_RS_PATH).expect("read main.rs");

    assert!(main_rs.contains("PurgeLastModel"));
}

#[test]
fn active_launcher_surface_no_longer_mentions_legacy_backends() {
    let launch_execution_flow =
        fs::read_to_string(LAUNCH_EXECUTION_FLOW_PATH).expect("read launch_execution_flow.rs");
    let settings_screen =
        fs::read_to_string(SETTINGS_SCREEN_FLOW_PATH).expect("read settings_screen_flow.rs");

    assert!(!launch_execution_flow.contains("BackendMode::KoboldCpp"));
    assert!(!launch_execution_flow.contains("BackendMode::Ollama"));
    assert!(!settings_screen.contains("BackendMode::KoboldCpp"));
    assert!(!settings_screen.contains("BackendMode::Ollama"));
}

#[test]
fn launcher_and_monitor_surface_only_report_managed_llamacpp_runtime() {
    let launcher = fs::read_to_string(LAUNCHER_RS_PATH).expect("read launcher.rs");
    let monitor = fs::read_to_string(MONITOR_RS_PATH).expect("read monitor.rs");

    for legacy_marker in ["KoboldCpp", "Ollama", "SillyTavern", ":8080", ":11434", ":8000"] {
        assert!(!launcher.contains(legacy_marker), "launcher still contains {legacy_marker}");
        assert!(!monitor.contains(legacy_marker), "monitor still contains {legacy_marker}");
    }
    assert!(launcher.contains(":8989"));
    assert!(monitor.contains(":8989"));
}