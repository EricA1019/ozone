use super::{
    capturable_screen_journey_builders, default_preferences_json, merge_json_objects,
    mock_user_capture_settings, normalize_preferences_json, screenshot_capture_config,
    tool_definitions, EnvOverrideGuard, OzoneMcpServer, Sandbox,
};
use crate::testing::{MockUserAction, MockUserJourneyStep};
use ozone_core::session::SessionId;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
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
    for binary in ["ozone", "ozone-plus"] {
        let path = server.repo_root.join("target/release").join(binary);
        assert!(
            path.is_file(),
            "missing release binary {} for release smoke; build target/release/{} first",
            path.display(),
            binary
        );
    }
}

fn assert_mock_user_success(data: &Value, context: &str) {
    let final_tail = data
        .get("finalTail")
        .and_then(Value::as_str)
        .unwrap_or_default();
    assert_eq!(
        data.get("runnerOk").and_then(Value::as_bool),
        Some(true),
        "{context}: PTY runner failed\n{}",
        final_tail
    );
    assert_eq!(
        data.get("success").and_then(Value::as_bool),
        Some(true),
        "{context}: journey markers were not reached\n{}",
        final_tail
    );
}

fn build_fresh_base_launcher_journey(
    server: &OzoneMcpServer,
) -> crate::testing::MockUserJourneySpec {
    crate::testing::MockUserJourneySpec {
        name: "release_fresh_base_launcher".to_owned(),
        cwd: server.repo_root.to_string_lossy().into_owned(),
        command: crate::testing::append_args(
            &crate::testing::front_door_binary_command(
                &server.repo_root,
                "ozone",
                &["--mode", "base"],
            ),
            &["--no-browser"],
        ),
        steps: vec![MockUserJourneyStep::wait_for(
            "reach launcher",
            35000,
            ["Launch", "Open ozone+", "Settings"],
        )],
    }
}

fn run_release_binary(
    server: &OzoneMcpServer,
    sandbox_id: &str,
    binary: &str,
    args: &[&str],
) -> super::CommandOutput {
    let program = server.repo_root.join("target/release").join(binary);
    let program = program.display().to_string();
    let args = args
        .iter()
        .map(|value| (*value).to_owned())
        .collect::<Vec<_>>();
    let output = server
        .run_workspace_command(&program, &args, Some(sandbox_id))
        .expect("release binary command");
    assert!(
        output.success,
        "release command failed: {}\nstdout:\n{}\nstderr:\n{}",
        output.command, output.stdout, output.stderr
    );
    output
}

fn first_session_id(server: &OzoneMcpServer, sandbox_id: &str) -> String {
    server
        .with_repo(Some(sandbox_id), |repo| {
            Ok(repo
                .list_sessions()?
                .into_iter()
                .next()
                .map(|session| session.session_id.to_string()))
        })
        .expect("list sessions")
        .expect("persisted session")
}

fn active_transcript_len(server: &OzoneMcpServer, sandbox_id: &str, session_id: &str) -> usize {
    let session_id = SessionId::parse(session_id).expect("valid session id");
    server
        .with_repo(Some(sandbox_id), |repo| {
            Ok(repo.get_active_branch_transcript(&session_id)?.len())
        })
        .expect("active transcript")
}

fn release_smoke_artifact_dir(name: &str) -> PathBuf {
    let output_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("repo root")
        .join("target/test-artifacts/ozone-mcp")
        .join("release-smoke")
        .join(format!("{name}-{}", Uuid::new_v4()));
    fs::create_dir_all(&output_dir).expect("create release-smoke artifact dir");
    output_dir
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
    let definition = tool_definitions()
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
    let definition = tool_definitions()
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
        capturable_screen_journey_builders().len()
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
    let definition = tool_definitions()
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
fn release_smoke_gate_fresh_temp_xdg_user_path() {
    with_front_door_profile("release", || {
        let mut server = OzoneMcpServer::new().expect("server");
        assert_release_front_door_binaries_exist(&server);

        let launcher_sandbox = server
            .prepare_sandbox_from_setup(None, crate::testing::sandbox_setup_base_launch_path())
            .expect("launcher sandbox");
        let launcher_journey = build_fresh_base_launcher_journey(&server);
        let launcher_capture_args = json!({
            "filename": "base-launcher-release",
            "rows": 55,
            "columns": 140,
            "fontSize": 18,
            "tailChars": 2048
        });
        let launcher_capture_dir = release_smoke_artifact_dir("base-launcher");
        let launcher_capture = screenshot_capture_config(
            &launcher_capture_args,
            &launcher_capture_dir,
            "base_launcher",
        )
        .expect("launcher capture config");
        let launcher_run = server
            .run_mock_user_journey(
                &launcher_sandbox.sandbox_id,
                &launcher_journey,
                Some("base_launcher".to_owned()),
                &launcher_capture_args,
                Some(launcher_capture),
            )
            .expect("launcher run");
        assert_mock_user_success(&launcher_run, "fresh temp-XDG base launcher");

        let launcher_sidecar = launcher_run["paths"]["json"].as_str().unwrap_or_else(|| {
            panic!(
                "fresh temp-XDG base launcher capture did not produce a sidecar\n{}",
                serde_json::to_string_pretty(&launcher_run)
                    .unwrap_or_else(|_| launcher_run.to_string())
            )
        });
        assert!(
            Path::new(launcher_sidecar).is_file(),
            "fresh temp-XDG base launcher sidecar missing: {launcher_sidecar}"
        );
        let launcher_screen = server
            .screen_check_tool(&json!({
                "sidecarPath": launcher_sidecar,
                "checks": [
                    { "type": "text_present", "text": "Launch" },
                    { "type": "text_present", "text": "Open ozone+" },
                    { "type": "text_present", "text": "Settings" },
                    { "type": "text_absent", "text": "local-first AI tooling" }
                ]
            }))
            .expect("launcher screen check");
        assert!(
            !launcher_screen.is_error,
            "fresh temp-XDG launcher screen check failed: {}\n{}",
            launcher_screen.summary,
            serde_json::to_string_pretty(&launcher_screen.data)
                .unwrap_or_else(|_| launcher_screen.data.to_string())
        );

        let mut settings_journey = crate::testing::build_base_settings_screen_journey(
            &server.repo_root,
            "release_fresh_base_settings",
            &json!({}),
        )
        .expect("settings journey");
        if let Some(step) = settings_journey.steps.last_mut() {
            step.expect_any.clear();
            step.settle_ms = step.settle_ms.max(1000);
        }
        let settings_capture_args = json!({
            "filename": "base-settings-release",
            "rows": 55,
            "columns": 140,
            "fontSize": 18,
            "tailChars": 2048
        });
        let settings_capture_dir = release_smoke_artifact_dir("base-settings");
        let settings_capture = screenshot_capture_config(
            &settings_capture_args,
            &settings_capture_dir,
            "base_settings",
        )
        .expect("settings capture config");
        let settings_run = server
            .run_mock_user_journey(
                &launcher_sandbox.sandbox_id,
                &settings_journey,
                Some("base_settings".to_owned()),
                &settings_capture_args,
                Some(settings_capture),
            )
            .expect("settings run");
        assert_mock_user_success(&settings_run, "fresh temp-XDG base settings");

        let settings_sidecar = settings_run["paths"]["json"].as_str().unwrap_or_else(|| {
            panic!(
                "fresh temp-XDG base settings capture did not produce a sidecar\n{}",
                serde_json::to_string_pretty(&settings_run)
                    .unwrap_or_else(|_| settings_run.to_string())
            )
        });
        assert!(
            Path::new(settings_sidecar).is_file(),
            "fresh temp-XDG base settings sidecar missing: {settings_sidecar}"
        );
        let settings_screen = server
            .screen_check_tool(&json!({
                "sidecarPath": settings_sidecar,
                "checks": [
                    { "type": "text_present", "text": "Settings" },
                    { "type": "text_present", "text": "Active Defaults" },
                    { "type": "text_present", "text": "Backend" },
                    { "type": "text_present", "text": "Frontend" }
                ]
            }))
            .expect("settings screen check");
        assert!(
            !settings_screen.is_error,
            "fresh temp-XDG settings screen check failed: {}\n{}",
            settings_screen.summary,
            serde_json::to_string_pretty(&settings_screen.data)
                .unwrap_or_else(|_| settings_screen.data.to_string())
        );

        let confirm_launch_journey = crate::testing::build_base_confirm_launch_screen_journey(
            &server.repo_root,
            "release_fresh_base_confirm_launch",
            &json!({}),
        )
        .expect("confirm launch journey");
        let confirm_launch_capture_args = json!({
            "filename": "base-confirm-launch-release",
            "rows": 55,
            "columns": 140,
            "fontSize": 18,
            "tailChars": 2048
        });
        let confirm_launch_capture_dir = release_smoke_artifact_dir("base-confirm-launch");
        let confirm_launch_capture = screenshot_capture_config(
            &confirm_launch_capture_args,
            &confirm_launch_capture_dir,
            "base_confirm_launch",
        )
        .expect("confirm launch capture config");
        let confirm_launch_run = server
            .run_mock_user_journey(
                &launcher_sandbox.sandbox_id,
                &confirm_launch_journey,
                Some("base_confirm_launch".to_owned()),
                &confirm_launch_capture_args,
                Some(confirm_launch_capture),
            )
            .expect("confirm launch run");
        assert_mock_user_success(&confirm_launch_run, "fresh temp-XDG base confirm launch");

        let confirm_launch_sidecar =
            confirm_launch_run["paths"]["json"]
                .as_str()
                .unwrap_or_else(|| {
                    panic!(
                        "fresh temp-XDG base confirm launch capture did not produce a sidecar\n{}",
                        serde_json::to_string_pretty(&confirm_launch_run)
                            .unwrap_or_else(|_| confirm_launch_run.to_string())
                    )
                });
        assert!(
            Path::new(confirm_launch_sidecar).is_file(),
            "fresh temp-XDG base confirm launch sidecar missing: {confirm_launch_sidecar}"
        );
        let confirm_launch_screen = server
            .screen_check_tool(&json!({
                "sidecarPath": confirm_launch_sidecar,
                "checks": [
                    { "type": "text_present", "text": "Confirm Launch" },
                    { "type": "text_present", "text": "Context:" },
                    { "type": "text_present", "text": "QuantKV:" },
                    { "type": "text_absent", "text": "Choose Frontend" }
                ]
            }))
            .expect("confirm launch screen check");
        assert!(
            !confirm_launch_screen.is_error,
            "fresh temp-XDG confirm launch screen check failed: {}\n{}",
            confirm_launch_screen.summary,
            serde_json::to_string_pretty(&confirm_launch_screen.data)
                .unwrap_or_else(|_| confirm_launch_screen.data.to_string())
        );

        let frontend_choice_journey = crate::testing::build_base_frontend_choice_screen_journey(
            &server.repo_root,
            "release_fresh_base_frontend_choice",
            &json!({}),
        )
        .expect("frontend choice journey");
        let frontend_choice_capture_args = json!({
            "filename": "base-frontend-choice-release",
            "rows": 55,
            "columns": 140,
            "fontSize": 18,
            "tailChars": 2048
        });
        let frontend_choice_capture_dir = release_smoke_artifact_dir("base-frontend-choice");
        let frontend_choice_capture = screenshot_capture_config(
            &frontend_choice_capture_args,
            &frontend_choice_capture_dir,
            "base_frontend_choice",
        )
        .expect("frontend choice capture config");
        let frontend_choice_run = server
            .run_mock_user_journey(
                &launcher_sandbox.sandbox_id,
                &frontend_choice_journey,
                Some("base_frontend_choice".to_owned()),
                &frontend_choice_capture_args,
                Some(frontend_choice_capture),
            )
            .expect("frontend choice run");
        assert_mock_user_success(&frontend_choice_run, "fresh temp-XDG base frontend choice");

        let frontend_choice_sidecar = frontend_choice_run["paths"]["json"]
            .as_str()
            .unwrap_or_else(|| {
                panic!(
                    "fresh temp-XDG base frontend choice capture did not produce a sidecar\n{}",
                    serde_json::to_string_pretty(&frontend_choice_run)
                        .unwrap_or_else(|_| frontend_choice_run.to_string())
                )
            });
        assert!(
            Path::new(frontend_choice_sidecar).is_file(),
            "fresh temp-XDG base frontend choice sidecar missing: {frontend_choice_sidecar}"
        );
        let frontend_choice_screen = server
            .screen_check_tool(&json!({
                "sidecarPath": frontend_choice_sidecar,
                "checks": [
                    { "type": "text_present", "text": "Choose Frontend" },
                    { "type": "text_present", "text": "SillyTavern" },
                    { "type": "text_present", "text": "ozone+" },
                    { "type": "text_absent", "text": "Launching KoboldCpp" }
                ]
            }))
            .expect("frontend choice screen check");
        assert!(
            !frontend_choice_screen.is_error,
            "fresh temp-XDG frontend choice screen check failed: {}\n{}",
            frontend_choice_screen.summary,
            serde_json::to_string_pretty(&frontend_choice_screen.data)
                .unwrap_or_else(|_| frontend_choice_screen.data.to_string())
        );

        let chat_sandbox = server
            .prepare_mock_user_sandbox(None, Some("ozone_plus_chat_journey"), None)
            .expect("chat sandbox");
        run_release_binary(
            &server,
            &chat_sandbox.sandbox_id,
            "ozone-plus",
            &["create", "Release Smoke Fresh"],
        );
        let session_id = first_session_id(&server, &chat_sandbox.sandbox_id);
        run_release_binary(
            &server,
            &chat_sandbox.sandbox_id,
            "ozone-plus",
            &["send", session_id.as_str(), "Check the observatory key"],
        );

        let transcript_len = active_transcript_len(&server, &chat_sandbox.sandbox_id, &session_id);
        assert!(
            transcript_len >= 2,
            "fresh-user ozone+ smoke should persist a non-empty transcript"
        );
    });
}

#[test]
#[ignore = "release smoke"]
fn release_smoke_gate_existing_user_data_path() {
    with_front_door_profile("release", || {
        let mut server = OzoneMcpServer::new().expect("server");
        assert_release_front_door_binaries_exist(&server);

        let first_args = json!({
            "prompt": "Remember the observatory key"
        });
        let prepared = server
            .prepare_mock_user_sandbox(None, Some("ozone_plus_chat_journey"), None)
            .expect("prepared sandbox");
        run_release_binary(
            &server,
            &prepared.sandbox_id,
            "ozone-plus",
            &["create", "Release Smoke Existing"],
        );
        let session_id = first_session_id(&server, &prepared.sandbox_id);
        run_release_binary(
            &server,
            &prepared.sandbox_id,
            "ozone-plus",
            &[
                "send",
                session_id.as_str(),
                first_args["prompt"].as_str().expect("first prompt"),
            ],
        );

        let existing_session_count = server
            .with_repo(Some(&prepared.sandbox_id), |repo| {
                Ok(repo.list_sessions()?.len())
            })
            .expect("existing session count");
        assert!(
            existing_session_count >= 1,
            "existing-user smoke needs persisted data before the second pass"
        );
        let initial_transcript_len =
            active_transcript_len(&server, &prepared.sandbox_id, &session_id);

        let second_args = json!({
            "prompt": "Use the existing data path and answer again"
        });
        let list_output =
            run_release_binary(&server, &prepared.sandbox_id, "ozone-plus", &["list"]);
        assert!(
            list_output.stdout.contains(&session_id),
            "existing-user smoke should list the persisted session"
        );
        run_release_binary(
            &server,
            &prepared.sandbox_id,
            "ozone-plus",
            &[
                "send",
                session_id.as_str(),
                second_args["prompt"].as_str().expect("second prompt"),
            ],
        );

        let final_session_count = server
            .with_repo(Some(&prepared.sandbox_id), |repo| {
                Ok(repo.list_sessions()?.len())
            })
            .expect("final session count");
        assert!(
            final_session_count >= existing_session_count,
            "existing-user smoke should preserve or grow persisted session state"
        );
        let final_transcript_len =
            active_transcript_len(&server, &prepared.sandbox_id, &session_id);
        assert!(
            final_transcript_len > initial_transcript_len,
            "existing-user smoke should append to the persisted transcript"
        );
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
