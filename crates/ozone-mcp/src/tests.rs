#[cfg(feature = "legacy-tools")]
use super::tool_dispatch;
use super::{
    capturable_screen_journey_builders, default_preferences_json, merge_json_objects,
    mock_user_capture_settings, normalize_preferences_json, scoped_capture_targets,
    scoped_tool_definitions, screenshot_capture_config, EnvOverrideGuard, OzoneMcpServer, Sandbox,
    LEGACY_TOOL_NAMES,
};
use crate::testing::MockUserAction;
use serde_json::json;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use uuid::Uuid;

fn with_front_door_profile<T>(profile: &str, f: impl FnOnce() -> T) -> T {
    let mut overrides = BTreeMap::new();
    overrides.insert(
        "OZONE_MCP_FRONT_DOOR_PROFILE".to_owned(),
        profile.to_owned(),
    );
    let _guard = EnvOverrideGuard::new(overrides);
    f()
}

fn assert_release_front_door_binaries_exist(server: &OzoneMcpServer) {
    for binary in ["ozone", "ozone-mcp"] {
        let path = server.repo_root.join("target/release").join(binary);
        assert!(
            path.is_file(),
            "missing release binary {} for release smoke; build target/release/{} first",
            path.display(),
            binary
        );
    }
}

fn run_release_binary(server: &OzoneMcpServer, binary: &str, args: &[&str]) -> String {
    let program = server.repo_root.join("target/release").join(binary);
    let output = Command::new(&program)
        .args(args)
        .current_dir(&server.repo_root)
        .output()
        .unwrap_or_else(|error| panic!("failed to run {}: {error}", program.display()));
    assert!(
        output.status.success(),
        "release command failed: {} {}\nstdout:\n{}\nstderr:\n{}",
        program.display(),
        args.join(" "),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
fn mock_user_launcher_monitor_journey_contains_monitor_markers() {
    let server = OzoneMcpServer::new().expect("server");
    let journey = server
        .build_mock_user_journey("launcher_monitor_roundtrip", &json!({}))
        .expect("journey");
    assert!(matches!(
        journey.steps[2].action,
        MockUserAction::Key { .. }
    ));
    assert!(journey.steps[2]
        .expect_any
        .iter()
        .any(|marker| marker == "Confirm Launch"));
    assert!(journey.steps[3]
        .expect_any
        .iter()
        .any(|marker| marker == "Ozone Monitor"));
}

#[test]
fn mock_user_chat_journey_includes_insert_and_response_markers() {
    let server = OzoneMcpServer::new().expect("server");
    let journey = server
        .build_mock_user_journey(
            "ozone_plus_chat_journey",
            &json!({ "prompt": "Check the observatory key" }),
        )
        .expect("journey");
    assert!(matches!(
        journey.steps[4].action,
        MockUserAction::Key { .. }
    ));
    assert!(journey.steps[4]
        .expect_any
        .iter()
        .any(|marker| marker == "Composer"));
    assert!(matches!(
        journey.steps[5].action,
        MockUserAction::Text { .. }
    ));
    assert!(journey.steps[5].expect_any.is_empty());
    assert!(matches!(
        journey.steps[6].action,
        MockUserAction::Key { .. }
    ));
    assert!(journey.steps[6]
        .expect_any
        .iter()
        .any(|marker| marker == "assistant"));
}

#[test]
fn capturable_screen_library_covers_base_and_ozone_plus_entry_surfaces() {
    let screens: Vec<_> = capturable_screen_journey_builders()
        .iter()
        .map(|entry| entry.target_screen)
        .collect();
    assert_eq!(
        screens,
        vec![
            "base_splash",
            "base_tier_picker",
            "base_launcher",
            "base_exit_confirm",
            "base_settings",
            "base_model_picker_launch",
            "base_confirm_launch",
            "base_frontend_choice",
            "base_launching",
            "base_monitor",
            "base_model_picker_profile",
            "base_profile_advisory",
            "base_profile_confirm",
            "base_profile_running",
            "base_profile_failure",
            "base_ozone_plus_shell",
            "ozone_plus_main_menu",
            "ozone_plus_sessions",
            "ozone_plus_characters",
            "ozone_plus_character_create",
            "ozone_plus_character_import",
            "ozone_plus_settings",
            "ozone_plus_conversation",
            "ozone_plus_help",
        ]
    );
}

#[test]
fn default_tool_definitions_hide_legacy_archived_tools() {
    let definitions = scoped_tool_definitions(false);
    let names: Vec<_> = definitions.iter().map(|tool| tool.name).collect();

    for legacy_tool in LEGACY_TOOL_NAMES {
        assert!(
            !names.contains(legacy_tool),
            "default tools/list should hide legacy tool {legacy_tool}"
        );
    }
    assert!(names.contains(&"workspace_status"));
    assert!(names.contains(&"mock_user_tool"));
    assert!(names.contains(&"screenshot_tool"));
}

#[test]
fn legacy_tool_definitions_are_available_when_opted_in() {
    let definitions = scoped_tool_definitions(true);
    let names: Vec<_> = definitions.iter().map(|tool| tool.name).collect();

    for legacy_tool in LEGACY_TOOL_NAMES {
        assert!(
            names.contains(legacy_tool),
            "legacy opt-in should expose tool {legacy_tool}"
        );
    }
}

#[test]
fn active_tool_schemas_hide_legacy_screen_targets_by_default() {
    let definitions = scoped_tool_definitions(false);
    let screenshot_tool = definitions
        .iter()
        .find(|tool| tool.name == "screenshot_tool")
        .expect("screenshot_tool");
    let target_enum = screenshot_tool.input_schema["properties"]["target"]["enum"]
        .as_array()
        .expect("target enum");

    assert_eq!(target_enum.len(), scoped_capture_targets(false).len());
    assert!(!target_enum.contains(&json!("base_ozone_plus_shell")));
    assert!(!target_enum.contains(&json!("ozone_plus_help")));
}

#[test]
fn legacy_tool_schemas_restore_archived_screen_targets_when_opted_in() {
    let definitions = scoped_tool_definitions(true);
    let screenshot_tool = definitions
        .iter()
        .find(|tool| tool.name == "screenshot_tool")
        .expect("screenshot_tool");
    let target_enum = screenshot_tool.input_schema["properties"]["target"]["enum"]
        .as_array()
        .expect("target enum");

    assert_eq!(target_enum.len(), scoped_capture_targets(true).len());
    assert!(target_enum.contains(&json!("base_ozone_plus_shell")));
    assert!(target_enum.contains(&json!("ozone_plus_help")));
}

#[cfg(feature = "legacy-tools")]
#[test]
fn legacy_tool_dispatch_is_blocked_without_opt_in() {
    let mut server = OzoneMcpServer::new().expect("server");
    let reply = tool_dispatch::dispatch_tool_call_with_legacy_mode(
        &mut server,
        "session_tool",
        &json!({ "action": "unsupported" }),
        false,
    )
    .expect("dispatch reply");

    assert!(reply.is_error);
    assert_eq!(reply.summary, "Legacy MCP tool is archived");
    assert_eq!(reply.data["scope"], json!("legacy-archived"));
}

#[cfg(feature = "legacy-tools")]
#[test]
fn legacy_tool_dispatch_reaches_handler_when_opted_in() {
    let mut server = OzoneMcpServer::new().expect("server");
    let reply = tool_dispatch::dispatch_tool_call_with_legacy_mode(
        &mut server,
        "session_tool",
        &json!({ "action": "unsupported" }),
        true,
    )
    .expect("dispatch reply");

    assert!(reply.is_error);
    assert_eq!(reply.summary, "Session action failed");
    assert_eq!(
        reply.data["error"],
        json!("unsupported session action `unsupported`")
    );
}

#[test]
fn capturable_screen_journeys_build_expected_commands_and_markers() {
    let server = OzoneMcpServer::new().expect("server");
    let cases = [
        ("base_splash", "ozone", "Continue"),
        ("base_tier_picker", "ozone", "Choose Your Tier"),
        ("base_launcher", "ozone", "Open ozone+"),
        ("base_exit_confirm", "ozone", "Confirm Exit"),
        ("base_settings", "ozone", "Active Defaults"),
        ("base_model_picker_launch", "ozone", "Model Picker · Launch"),
        ("base_confirm_launch", "ozone", "Confirm Launch"),
        ("base_frontend_choice", "ozone", "Choose Frontend"),
        ("base_launching", "ozone", "Launching KoboldCpp"),
        ("base_monitor", "ozone", "Ozone Monitor"),
        (
            "base_model_picker_profile",
            "ozone",
            "Model Picker · Profile",
        ),
        ("base_profile_advisory", "ozone", "Profiling Advisor"),
        ("base_profile_confirm", "ozone", "Confirm Profiling Step"),
        ("base_profile_running", "ozone", "Profiling In Progress"),
        ("base_profile_failure", "ozone", "Profiling Failed"),
        ("base_ozone_plus_shell", "ozone", "New Chat"),
        ("ozone_plus_main_menu", "ozone-plus", "New Chat"),
        ("ozone_plus_sessions", "ozone-plus", "Sessions"),
        ("ozone_plus_characters", "ozone-plus", "Characters"),
        ("ozone_plus_character_create", "ozone-plus", "New Character"),
        (
            "ozone_plus_character_import",
            "ozone-plus",
            "Import Character Card",
        ),
        ("ozone_plus_settings", "ozone-plus", "config.toml"),
        ("ozone_plus_conversation", "ozone-plus", "Conversation"),
        ("ozone_plus_help", "ozone-plus", "Slash Commands"),
    ];

    for (screen, command_fragment, marker) in cases {
        let journey = server
            .build_capturable_screen_journey(screen, &json!({}), screen)
            .unwrap_or_else(|error| panic!("failed to build {screen}: {error}"));
        assert!(
            journey
                .command
                .iter()
                .any(|part| part.contains(command_fragment)),
            "{screen} should use {command_fragment:?}: {:?}",
            journey.command
        );
        assert!(
            journey
                .steps
                .iter()
                .flat_map(|step| step.expect_any.iter())
                .any(|value| value == marker),
            "{screen} should expect marker {marker:?}"
        );
    }
}

#[test]
fn launcher_to_ozone_plus_journey_reuses_capturable_screen_spec() {
    let server = OzoneMcpServer::new().expect("server");
    let from_screen = server
        .build_capturable_screen_journey(
            "base_ozone_plus_shell",
            &json!({ "prompt": "ignored" }),
            "launcher_to_ozone_plus",
        )
        .expect("screen journey");
    let from_mock_user = server
        .build_mock_user_journey("launcher_to_ozone_plus", &json!({ "prompt": "ignored" }))
        .expect("mock-user journey");
    assert_eq!(from_mock_user, from_screen);
}

#[test]
fn mock_user_target_lookup_builds_screen_journey() {
    let server = OzoneMcpServer::new().expect("server");
    let journey = server
        .build_mock_user_target_journey("ozone_plus_help")
        .expect("screen journey");
    assert_eq!(journey.name, "ozone_plus_help");
    assert!(journey
        .steps
        .iter()
        .flat_map(|step| step.expect_any.iter())
        .any(|value| value == "Slash Commands"));
}

#[test]
fn mock_user_tool_is_listed_with_capture_inputs() {
    let definition = scoped_tool_definitions(false)
        .into_iter()
        .find(|tool| tool.name == "mock_user_tool")
        .expect("mock_user_tool");
    assert_eq!(
        definition.input_schema["properties"]["captureScreenshots"]["default"],
        json!(false)
    );
    assert!(definition.input_schema["properties"]
        .get("outputDir")
        .is_some());
    assert!(definition.input_schema["properties"].get("rows").is_some());
    assert!(definition.input_schema["properties"]
        .get("columns")
        .is_some());
    assert!(definition.input_schema["properties"]
        .get("fontSize")
        .is_some());
    assert!(definition.input_schema.get("required").is_none());
    assert_eq!(
        definition.input_schema["anyOf"],
        json!([{ "required": ["journey"] }, { "required": ["target"] }])
    );
}

#[test]
fn mock_user_capture_settings_add_step_artifacts_when_enabled() {
    let server = OzoneMcpServer::new().expect("server");
    let journey = server
        .build_mock_user_journey("launcher_to_ozone_plus", &json!({}))
        .expect("journey");
    let sandbox = Sandbox {
        id: "sandbox-123".to_owned(),
        root: PathBuf::from("/sandbox"),
        data_home: PathBuf::from("/sandbox/data"),
        home: PathBuf::from("/sandbox/home"),
        models_dir: PathBuf::from("/sandbox/models"),
        launcher_script: None,
        backend: None,
    };
    let settings = mock_user_capture_settings(
        &json!({
            "captureScreenshots": true,
            "outputDir": "captures/custom",
            "rows": 55,
            "columns": 140,
            "fontSize": 18
        }),
        &sandbox,
        &journey,
        None,
    )
    .expect("capture settings");
    assert!(settings.capture_screenshots);
    assert_eq!(
        settings.output_dir.as_deref(),
        Some("/sandbox/captures/custom")
    );
    assert_eq!(settings.capture.rows, 55);
    assert_eq!(settings.capture.columns, 140);
    assert_eq!(settings.capture.font_size, 18);
    assert_eq!(settings.step_captures.len(), journey.steps.len());
    assert_eq!(
        settings.capture.png_path.as_deref(),
        Some("/sandbox/captures/custom/final.png")
    );
    assert_eq!(
        settings.capture.json_path.as_deref(),
        Some("/sandbox/captures/custom/final.json")
    );
}

#[test]
fn screenshot_tool_is_listed_with_required_inputs() {
    let definition = scoped_tool_definitions(false)
        .into_iter()
        .find(|tool| tool.name == "screenshot_tool")
        .expect("screenshot tool");
    assert_eq!(
        definition.input_schema["required"],
        json!(["target", "outputDir"])
    );
    assert_eq!(
        definition.input_schema["properties"]["target"]["enum"]
            .as_array()
            .expect("target enum")
            .len(),
        scoped_capture_targets(false).len()
    );
}

#[test]
fn screenshot_capture_config_uses_requested_output_settings() {
    let config = screenshot_capture_config(
        &json!({
            "filename": "launcher.png",
            "dimensions": { "rows": 55, "columns": 140 },
            "fontSize": 18,
            "tailChars": 2048
        }),
        &PathBuf::from("/repo/captures"),
        "base_launcher",
    )
    .expect("capture config");
    assert_eq!(config.rows, 55);
    assert_eq!(config.columns, 140);
    assert_eq!(config.font_size, 18);
    assert_eq!(config.tail_chars, 2048);
    assert_eq!(
        config.png_path.as_deref(),
        Some("/repo/captures/launcher.png")
    );
    assert_eq!(
        config.json_path.as_deref(),
        Some("/repo/captures/launcher.json")
    );
}

#[test]
fn screenshot_tool_reports_clear_error_for_unknown_target() {
    let mut server = OzoneMcpServer::new().expect("server");
    let error = server
        .screenshot_tool(&json!({
            "sandboxId": "sandbox-123",
            "target": "does_not_exist",
            "outputDir": "captures"
        }))
        .expect_err("invalid target should fail");
    assert!(error
        .to_string()
        .contains("use `screen_nav_targets` to list valid targets"));
}

#[test]
fn screen_check_tool_is_listed_with_required_inputs() {
    let definition = scoped_tool_definitions(false)
        .into_iter()
        .find(|tool| tool.name == "screen_check_tool")
        .expect("screen check tool");
    assert_eq!(definition.input_schema["required"], json!(["checks"]));
    assert_eq!(
        definition.input_schema["anyOf"],
        json!([
            { "required": ["artifactPath"] },
            { "required": ["path"] },
            { "required": ["sidecarPath"] }
        ])
    );
    assert_eq!(
        definition.input_schema["properties"]["checks"]["items"]["properties"]["type"]["enum"],
        json!([
            "text_present",
            "text_absent",
            "color_at",
            "border_intact",
            "layout_columns",
            "no_overlap",
            "baseline_compare"
        ])
    );
}

#[test]
fn chat_journey_auto_setup_enables_mock_backend() {
    let server = OzoneMcpServer::new().expect("server");
    let setup = server
        .recommended_mock_user_journey_sandbox_setup("ozone_plus_chat_journey")
        .expect("setup");
    assert_eq!(setup["models"], json!(["mock-model.gguf"]));
    assert_eq!(setup["createLauncherStub"], json!(true));
    assert_eq!(setup["requiresMockBackend"], json!(true));
}

#[test]
fn normalize_preferences_json_converts_camel_case_keys_and_enum_values() {
    let normalized = merge_json_objects(
        default_preferences_json(),
        normalize_preferences_json(&json!({
            "preferredTier": "base",
            "preferredBackend": "KoboldCpp",
            "preferredFrontend": "OzonePlus",
            "sideBySideMonitor": true
        })),
    );
    assert_eq!(normalized["version"], json!(1));
    assert_eq!(normalized["preferred_tier"], json!("base"));
    assert_eq!(normalized["preferred_backend"], json!("kobold-cpp"));
    assert_eq!(normalized["preferred_frontend"], json!("ozone-plus"));
    assert_eq!(normalized["side_by_side_monitor"], json!(true));
}

#[test]
fn screen_check_tool_passes_core_checks_against_fixture() {
    let fixture = screen_check_fixture_path();
    let server = OzoneMcpServer::new().expect("server");
    let reply = server
        .screen_check_tool(&json!({
            "artifactPath": fixture.with_extension("png").display().to_string(),
            "checks": [
                { "type": "text_present", "text": "Menu" },
                { "type": "text_absent", "text": "Danger" },
                { "type": "color_at", "row": 1, "column": 1, "fg": "yellow", "bg": [12, 12, 12] },
                { "type": "border_intact", "region": { "top": 0, "left": 0, "bottom": 4, "right": 17 } },
                { "type": "layout_columns", "count": 2, "region": { "top": 1, "left": 1, "bottom": 2, "right": 15 }, "minGap": 2 },
                {
                    "type": "no_overlap",
                    "regions": [
                        { "name": "left", "top": 1, "left": 1, "bottom": 2, "right": 2 },
                        { "name": "right", "top": 1, "left": 6, "bottom": 2, "right": 7 }
                    ]
                }
            ]
        }))
        .expect("screen check reply");
    assert!(!reply.is_error, "{}", reply.summary);
    assert_eq!(reply.data["summary"]["passed"], json!(6));
    assert_eq!(reply.data["summary"]["failed"], json!(0));
    assert_eq!(reply.data["checks"].as_array().expect("checks").len(), 6);
}

#[test]
fn screen_check_tool_returns_error_reply_when_check_fails() {
    let fixture = screen_check_fixture_path();
    let server = OzoneMcpServer::new().expect("server");
    let reply = server
        .screen_check_tool(&json!({
            "artifactPath": fixture.display().to_string(),
            "checks": [{ "type": "text_absent", "text": "Menu" }]
        }))
        .expect("screen check reply");
    assert!(reply.is_error, "{}", reply.summary);
    assert_eq!(reply.data["summary"]["failed"], json!(1));
    assert_eq!(reply.data["checks"][0]["passed"], json!(false));
}

#[test]
fn screen_check_tool_passes_baseline_compare_against_matching_sidecar() {
    let fixture = screen_check_fixture_path();
    let server = OzoneMcpServer::new().expect("server");
    let reply = server
        .screen_check_tool(&json!({
            "artifactPath": fixture.display().to_string(),
            "checks": [
                { "type": "baseline_compare", "baselinePath": fixture.display().to_string() }
            ]
        }))
        .expect("screen check reply");
    assert!(!reply.is_error, "{}", reply.summary);
    assert_eq!(reply.data["summary"]["passed"], json!(1));
    assert_eq!(reply.data["checks"][0]["passed"], json!(true));
    assert_eq!(reply.data["checks"][0]["detail"]["diffCount"], json!(0));
    assert_eq!(reply.data["checks"][0]["detail"]["changedCells"], json!([]));
    assert_eq!(
        reply.data["checks"][0]["detail"]["differenceSummary"],
        json!("No grid differences detected")
    );
    assert_eq!(
        reply.data["checks"][0]["detail"]["matchPercent"],
        json!(100.0)
    );
}

#[test]
fn screen_check_tool_reports_baseline_compare_differences() {
    let fixture = screen_check_fixture_path();
    let differing_baseline = write_modified_baseline_fixture();
    let server = OzoneMcpServer::new().expect("server");
    let reply = server
        .screen_check_tool(&json!({
            "artifactPath": fixture.display().to_string(),
            "checks": [
                {
                    "type": "baseline_compare",
                    "baselineSidecarPath": differing_baseline.path().display().to_string()
                }
            ]
        }))
        .expect("screen check reply");
    assert!(reply.is_error, "{}", reply.summary);
    assert_eq!(reply.data["summary"]["failed"], json!(1));
    assert_eq!(reply.data["checks"][0]["passed"], json!(false));
    assert_eq!(reply.data["checks"][0]["detail"]["diffCount"], json!(1));
    assert_eq!(
        reply.data["checks"][0]["detail"]["changedCells"][0],
        json!({ "row": 0, "column": 2 })
    );
    assert_eq!(
        reply.data["checks"][0]["detail"]["sampleDiffs"][0]["kind"],
        json!("changed")
    );
    assert!(reply.data["checks"][0]["detail"]["differenceSummary"]
        .as_str()
        .expect("difference summary")
        .contains("1 cell diff(s)"));
}

#[test]
fn screen_check_tool_reports_clear_error_for_missing_sidecar() {
    let server = OzoneMcpServer::new().expect("server");
    let error = server
        .screen_check_tool(&json!({
            "artifactPath": "does/not/exist.png",
            "checks": [{ "type": "text_present", "text": "Menu" }]
        }))
        .expect_err("missing sidecar should fail");
    assert!(error.to_string().contains("screen capture sidecar"));
}

#[test]
#[ignore = "release smoke"]
fn release_smoke_gate_current_rc_surface() {
    with_front_door_profile("release", || {
        let server = OzoneMcpServer::new().expect("server");
        assert_release_front_door_binaries_exist(&server);

        let root_help = run_release_binary(&server, "ozone", &["--help"]);
        for expected in [
            "local AI stack operator",
            "bench",
            "sweep",
            "eval",
            "eval-run",
            "model",
            "old chat shell is deprecated and archived",
        ] {
            assert!(
                root_help.contains(expected),
                "release help missing expected text `{expected}`\n{root_help}"
            );
        }
        for deprecated in ["ozone-plus", "SillyTavern", "KoboldCpp"] {
            assert!(
                !root_help.contains(deprecated),
                "release help should not expose deprecated text `{deprecated}`\n{root_help}"
            );
        }

        let bench_help = run_release_binary(&server, "ozone", &["bench", "--help"]);
        assert!(bench_help.contains("Benchmark a model with specific settings"));
        assert!(bench_help.contains("--context"));
        assert!(bench_help.contains("--quant-kv"));

        let eval_help = run_release_binary(&server, "ozone", &["eval-run", "--help"]);
        assert!(eval_help.contains("Run the native eval pipeline"));
        assert!(eval_help.contains("--base-url"));
        assert!(eval_help.contains("--context-length"));
    });
}

fn screen_check_fixture_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/screen-check-fixture.json")
}

struct TestSidecarFile {
    path: PathBuf,
}

impl TestSidecarFile {
    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TestSidecarFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn write_modified_baseline_fixture() -> TestSidecarFile {
    let fixture_path = screen_check_fixture_path();
    let mut capture: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(&fixture_path).expect("read screen-check fixture"),
    )
    .expect("parse screen-check fixture");
    capture["rows"][0]["cells"][2]["text"] = json!("X");
    capture["rows"][0]["text"] = json!("┌─Xenu───────────┐");
    capture["display"][0] = json!("┌─Xenu───────────┐");
    capture["text"] = json!(capture["text"]
        .as_str()
        .expect("fixture text")
        .replacen("Menu", "Xenu", 1));
    capture["tailText"] = json!(capture["tailText"]
        .as_str()
        .expect("fixture tailText")
        .replacen("Menu", "Xenu", 1));

    let output_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("repo root")
        .join("target/test-artifacts/ozone-mcp");
    fs::create_dir_all(&output_dir).expect("create test-artifact dir");
    let path = output_dir.join(format!("baseline-compare-{}.json", Uuid::new_v4()));
    fs::write(
        &path,
        serde_json::to_vec_pretty(&capture).expect("serialize modified baseline fixture"),
    )
    .expect("write modified baseline fixture");
    TestSidecarFile { path }
}


// ── Protocol integration tests ───────────────────────────────────────────
// These tests exercise the JSON-RPC protocol layer through the server's
// handle_request method, covering initialize, tools/list, ping, and error paths.


fn send_request(server: &mut super::OzoneMcpServer, method: &str, params: Option<serde_json::Value>) -> serde_json::Value {
    let request = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": method,
        "params": params.unwrap_or(json!({})),
    });
    let req: super::JsonRpcRequest = serde_json::from_value(request).unwrap();
    server.handle_request(req).unwrap_or_else(|| {
        json!({"error": {"message": "no response"}})
    })
}

#[test]
fn protocol_initialize_returns_server_info() {
    let mut server = super::OzoneMcpServer::new().expect("server should create");
    let response = send_request(&mut server, "initialize", Some(json!({
        "protocolVersion": "2024-11-05",
        "capabilities": {},
        "clientInfo": {"name": "test", "version": "0.0.0"}
    })));
    assert!(response.get("result").is_some(),
        "initialize should return a result");
    assert!(response["result"]["serverInfo"]["name"].as_str().unwrap_or("").contains("ozone"),
        "server info should identify as ozone");
}

#[test]
fn protocol_tools_list_returns_tool_definitions() {
    let mut server = super::OzoneMcpServer::new().expect("server should create");
    send_request(&mut server, "initialize", Some(json!({
        "protocolVersion": "2024-11-05",
        "capabilities": {},
        "clientInfo": {"name": "test", "version": "0.0.0"}
    })));
    let response = send_request(&mut server, "tools/list", None);
    let tools = response["result"]["tools"].as_array()
        .expect("tools/list should return a tools array");
    assert!(!tools.is_empty(), "should advertise at least one tool");
    assert!(tools.iter().any(|t| t["name"] == "workspace_status"),
        "tools should include workspace_status");
}

#[test]
fn protocol_ping_returns_result() {
    let mut server = super::OzoneMcpServer::new().expect("server should create");
    let response = send_request(&mut server, "ping", None);
    assert!(response.get("result").is_some(),
        "ping should return a result");
}

#[test]
fn protocol_unknown_method_returns_error() {
    let mut server = super::OzoneMcpServer::new().expect("server should create");
    let request = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "nonexistent_method",
        "params": {},
    });
    let req: super::JsonRpcRequest = serde_json::from_value(request).unwrap();
    let response = server.handle_request(req);
    assert!(response.is_some(), "should respond with error for unknown method");
    if let Some(resp) = response {
        assert!(resp.get("error").is_some(),
            "unknown method should return an error response");
    }
}
