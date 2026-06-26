//! Sandbox setup functions for mock user journeys.
//!
//! These functions define the test environment configuration for each
//! capturable screen journey.

use serde_json::json;
use serde_json::Value;

/// Base sandbox setup function that creates a JSON specification for sandbox configuration.
pub fn sandbox_setup(
    models: &[&str],
    preferences: Option<Value>,
    create_launcher_stub: bool,
    requires_mock_backend: bool,
) -> Value {
    json!({
        "models": models,
        "preferences": preferences,
        "createLauncherStub": create_launcher_stub,
        "requiresMockBackend": requires_mock_backend
    })
}

/// Sandbox setup for base splash screen journey - minimal config with base tier preference.
pub fn sandbox_setup_base_splash() -> Value {
    sandbox_setup(&[], Some(json!({ "preferredTier": "base" })), false, false)
}

/// Sandbox setup for base tier picker journey - no preferences, clean state.
pub fn sandbox_setup_base_tier_picker() -> Value {
    sandbox_setup(&[], None, false, false)
}

/// Sandbox setup for base launcher journey - base tier preference.
pub fn sandbox_setup_base_launcher() -> Value {
    sandbox_setup(&[], Some(json!({ "preferredTier": "base" })), false, false)
}

/// Sandbox setup for base launch path - includes mock model and KoboldCpp backend.
pub fn sandbox_setup_base_launch_path() -> Value {
    sandbox_setup(
        &["mock-model.gguf"],
        Some(json!({
            "preferredTier": "base",
            "preferredBackend": "kobold-cpp"
        })),
        true,
        false,
    )
}

/// Sandbox setup for base profile review - mock model with KoboldCpp backend, no launcher stub.
pub fn sandbox_setup_base_profile_review() -> Value {
    sandbox_setup(
        &["mock-model.gguf"],
        Some(json!({
            "preferredTier": "base",
            "preferredBackend": "kobold-cpp"
        })),
        false,
        false,
    )
}

/// Sandbox setup for base profile run - reuses launch path configuration.
pub fn sandbox_setup_base_profile_run() -> Value {
    sandbox_setup_base_launch_path()
}

/// Sandbox setup for base ozone+ shell - includes ozone+ frontend preference.
pub fn sandbox_setup_base_ozone_plus_shell() -> Value {
    sandbox_setup(
        &["mock-model.gguf"],
        Some(json!({
            "preferredTier": "base",
            "preferredBackend": "kobold-cpp",
            "preferredFrontend": "ozone-plus"
        })),
        true,
        false,
    )
}

/// Sandbox setup for ozone+ entry - minimal config for ozone+ direct handoff.
pub fn sandbox_setup_ozone_plus_entry() -> Value {
    sandbox_setup(&[], None, false, false)
}

// =============================================================================
// Helper Functions for Journey Building
// =============================================================================

use anyhow::{bail, Result};
use std::path::Path;

use super::{MockUserJourneySpec, MockUserJourneyStep};

/// Helper function to append arguments to a command vector.
#[allow(dead_code)]
pub fn append_args(command: &[String], args: &[&str]) -> Vec<String> {
    let mut full = command.to_vec();
    full.extend(args.iter().map(|value| (*value).to_owned()));
    full
}

/// Resolve binary command, preferring debug build if available, falling back to cargo run.
#[allow(dead_code)]
pub fn front_door_binary_command(repo_root: &Path, binary: &str, args: &[&str]) -> Vec<String> {
    if matches!(
        std::env::var("OZONE_MCP_FRONT_DOOR_PROFILE").as_deref(),
        Ok("release")
    ) {
        let binary_path = repo_root.join("target/release").join(binary);
        let mut command = vec![binary_path.display().to_string()];
        command.extend(args.iter().map(|value| (*value).to_owned()));
        return command;
    }

    let binary_path = repo_root.join("target/debug").join(binary);
    if binary_path.exists() {
        let mut command = vec![binary_path.display().to_string()];
        command.extend(args.iter().map(|value| (*value).to_owned()));
        command
    } else {
        let mut command = vec!["cargo".to_owned(), "run".to_owned(), "--quiet".to_owned()];
        if binary != "ozone" {
            command.push("-p".to_owned());
            command.push(binary.to_owned());
        }
        command.push("--".to_owned());
        command.extend(args.iter().map(|value| (*value).to_owned()));
        command
    }
}

// =============================================================================
// Journey Builders (extracted from lib.rs)
// =============================================================================

pub fn build_mock_user_journey(
    server: &crate::OzoneMcpServer,
    journey_name: &str,
    args: &Value,
) -> Result<MockUserJourneySpec> {
    match journey_name {
        "launcher_monitor_roundtrip" => {
            let mut journey =
                build_capturable_screen_journey(server, "base_monitor", args, journey_name)?;
            journey.steps.push(MockUserJourneyStep::text(
                "return to launcher",
                "r",
                1200,
                ["Launch", "Open ozone+", "Settings"],
            ));
            Ok(journey)
        }
        "launcher_to_ozone_plus" => {
            build_capturable_screen_journey(server, "base_ozone_plus_shell", args, journey_name)
        }
        "ozone_plus_chat_journey" => {
            let prompt = crate::optional_string(args, "prompt")
                .unwrap_or_else(|| "Check the observatory key".to_owned());
            let mut journey = build_capturable_screen_journey(
                server,
                "base_ozone_plus_shell",
                args,
                journey_name,
            )?;
            if let Some(step) = journey.steps.last_mut() {
                step.settle_ms = 2500;
            }
            journey.steps.extend([
                MockUserJourneyStep::key(
                    "open new chat",
                    "enter",
                    800,
                    ["Composer", "insert mode", "NOR"],
                ),
                MockUserJourneyStep::text("type prompt", &prompt, 400, []),
                MockUserJourneyStep::key(
                    "send prompt",
                    "enter",
                    8000,
                    ["You", "User", "assistant", "AI", "ozone+"],
                ),
            ]);
            Ok(journey)
        }
        other => bail!("unsupported mock-user journey `{other}`"),
    }
}

pub fn build_capturable_screen_journey(
    server: &crate::OzoneMcpServer,
    target_screen: &str,
    args: &Value,
    journey_name: &str,
) -> Result<MockUserJourneySpec> {
    let builder = capturable_screen_definition(target_screen)?.builder;
    builder(&server.repo_root, journey_name, args)
}

pub fn capturable_screen_definition(
    target_screen: &str,
) -> Result<&'static crate::testing::CapturableScreenJourneyDefinition> {
    crate::capturable_screen_journey_builders()
        .iter()
        .find(|entry| entry.target_screen == target_screen)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "unknown screen navigation target `{target_screen}`; use `screen_nav_targets` to list valid targets"
            )
        })
}

pub fn screen_nav_target_data(
    server: &crate::OzoneMcpServer,
    definition: &crate::testing::CapturableScreenJourneyDefinition,
) -> Result<Value> {
    let journey = build_capturable_screen_journey(
        server,
        definition.target_screen,
        &json!({}),
        definition.target_screen,
    )?;
    Ok(json!({
        "name": definition.target_screen,
        "description": definition.description,
        "command": journey.command,
        "toolArguments": {
            "target": definition.target_screen
        },
        "sandboxSetup": (definition.sandbox_setup)(),
    }))
}

pub fn build_base_splash_screen_journey(
    repo_root: &Path,
    journey_name: &str,
    _args: &Value,
) -> Result<MockUserJourneySpec> {
    Ok(MockUserJourneySpec {
        name: journey_name.to_owned(),
        cwd: repo_root.to_string_lossy().into_owned(),
        command: append_args(
            &front_door_binary_command(repo_root, "ozone", &["--mode", "base"]),
            &["--no-browser"],
        ),
        steps: vec![MockUserJourneyStep::wait_for(
            "render splash",
            5500,
            ["Continue", "local-first AI tooling"],
        )],
    })
}

pub fn build_base_tier_picker_screen_journey(
    repo_root: &Path,
    journey_name: &str,
    _args: &Value,
) -> Result<MockUserJourneySpec> {
    Ok(MockUserJourneySpec {
        name: journey_name.to_owned(),
        cwd: repo_root.to_string_lossy().into_owned(),
        command: front_door_binary_command(repo_root, "ozone", &["--pick", "--no-browser"]),
        steps: vec![
            MockUserJourneyStep::wait("splash settle", 5500),
            MockUserJourneyStep::key(
                "open tier picker",
                "enter",
                1000,
                ["Choose Your Tier", "ozone+", "ozonelite"],
            ),
        ],
    })
}

pub fn build_base_launcher_screen_journey(
    repo_root: &Path,
    journey_name: &str,
    _args: &Value,
) -> Result<MockUserJourneySpec> {
    Ok(MockUserJourneySpec {
        name: journey_name.to_owned(),
        cwd: repo_root.to_string_lossy().into_owned(),
        command: append_args(
            &front_door_binary_command(repo_root, "ozone", &["--mode", "base"]),
            &["--no-browser"],
        ),
        steps: vec![
            MockUserJourneyStep::wait("splash settle", 5500),
            MockUserJourneyStep::key(
                "reach launcher",
                "enter",
                1000,
                ["Launch", "Open ozone+", "Settings"],
            ),
        ],
    })
}

pub fn build_base_exit_confirm_screen_journey(
    repo_root: &Path,
    journey_name: &str,
    _args: &Value,
) -> Result<MockUserJourneySpec> {
    let mut journey = build_base_launcher_screen_journey(repo_root, journey_name, &json!({}))?;
    journey.steps.push(MockUserJourneyStep::key(
        "open exit confirm",
        "esc",
        600,
        ["Confirm Exit", "Leave Ozone?", "Stay"],
    ));
    Ok(journey)
}

pub fn build_base_settings_screen_journey(
    repo_root: &Path,
    journey_name: &str,
    _args: &Value,
) -> Result<MockUserJourneySpec> {
    let mut journey = build_base_launcher_screen_journey(repo_root, journey_name, &json!({}))?;
    journey.steps.extend([
        MockUserJourneyStep::text("open quick command", "/", 150, ["Quick Command", "Matches"]),
        MockUserJourneyStep::text(
            "filter settings command",
            "settings",
            250,
            ["Settings", "/settings"],
        ),
        MockUserJourneyStep::key(
            "open settings",
            "enter",
            800,
            ["Settings", "Active Defaults", "Navigation"],
        ),
    ]);
    Ok(journey)
}

pub fn build_base_model_picker_launch_screen_journey(
    repo_root: &Path,
    journey_name: &str,
    _args: &Value,
) -> Result<MockUserJourneySpec> {
    let mut journey = build_base_launcher_screen_journey(repo_root, journey_name, &json!({}))?;
    journey.steps.push(MockUserJourneyStep::key(
        "open launch model picker",
        "enter",
        1200,
        ["Model Picker · Launch", "mock-model.gguf", "type to filter"],
    ));
    Ok(journey)
}

pub fn build_base_confirm_launch_screen_journey(
    repo_root: &Path,
    journey_name: &str,
    _args: &Value,
) -> Result<MockUserJourneySpec> {
    let mut journey =
        build_base_model_picker_launch_screen_journey(repo_root, journey_name, &json!({}))?;
    journey.steps.push(MockUserJourneyStep::key(
        "build launch plan",
        "enter",
        1200,
        ["Confirm Launch", "Context:", "QuantKV:"],
    ));
    Ok(journey)
}

pub fn build_base_frontend_choice_screen_journey(
    repo_root: &Path,
    journey_name: &str,
    _args: &Value,
) -> Result<MockUserJourneySpec> {
    let mut journey =
        build_base_confirm_launch_screen_journey(repo_root, journey_name, &json!({}))?;
    journey.steps.push(MockUserJourneyStep::key(
        "open frontend choice",
        "enter",
        800,
        ["Choose Frontend", "SillyTavern", "ozone+"],
    ));
    Ok(journey)
}

pub fn build_base_launching_screen_journey(
    repo_root: &Path,
    journey_name: &str,
    _args: &Value,
) -> Result<MockUserJourneySpec> {
    let mut journey =
        build_base_frontend_choice_screen_journey(repo_root, journey_name, &json!({}))?;
    journey.steps.push(MockUserJourneyStep::key(
        "start launch",
        "enter",
        600,
        [
            "Launching KoboldCpp",
            "Preparing ozone+ handoff",
            "Please wait",
        ],
    ));
    Ok(journey)
}

pub fn build_base_monitor_screen_journey(
    repo_root: &Path,
    journey_name: &str,
    _args: &Value,
) -> Result<MockUserJourneySpec> {
    let mut journey = build_base_launcher_screen_journey(repo_root, journey_name, &json!({}))?;
    journey.command = append_args(
        &front_door_binary_command(repo_root, "ozone", &["--mode", "base"]),
        &["--frontend", "silly-tavern", "--no-browser"],
    );
    journey.steps.extend([
        MockUserJourneyStep::key(
            "pick launch model",
            "enter",
            1500,
            ["Confirm Launch", "Context:", "QuantKV:"],
        ),
        MockUserJourneyStep::key(
            "launch into monitor",
            "enter",
            3000,
            ["Ozone Monitor", "Services", "SillyTavern"],
        ),
    ]);
    Ok(journey)
}

pub fn build_base_model_picker_profile_screen_journey(
    repo_root: &Path,
    journey_name: &str,
    _args: &Value,
) -> Result<MockUserJourneySpec> {
    let mut journey = build_base_launcher_screen_journey(repo_root, journey_name, &json!({}))?;
    journey.steps.extend([
        MockUserJourneyStep::key("move to profile", "down", 150, []),
        MockUserJourneyStep::key(
            "open profile model picker",
            "enter",
            1200,
            [
                "Model Picker · Profile",
                "mock-model.gguf",
                "type to filter",
            ],
        ),
    ]);
    Ok(journey)
}

pub fn build_base_profile_advisory_screen_journey(
    repo_root: &Path,
    journey_name: &str,
    _args: &Value,
) -> Result<MockUserJourneySpec> {
    let mut journey =
        build_base_model_picker_profile_screen_journey(repo_root, journey_name, &json!({}))?;
    journey.steps.push(MockUserJourneyStep::key(
        "build profiling advisory",
        "enter",
        1200,
        ["Profiling Advisor", "Next Actions", "Recommendation:"],
    ));
    Ok(journey)
}

pub fn build_base_profile_confirm_screen_journey(
    repo_root: &Path,
    journey_name: &str,
    _args: &Value,
) -> Result<MockUserJourneySpec> {
    let mut journey =
        build_base_profile_advisory_screen_journey(repo_root, journey_name, &json!({}))?;
    journey.steps.push(MockUserJourneyStep::key(
        "open profiling confirm",
        "enter",
        800,
        ["Confirm Profiling Step", "Press Enter to start", "Action:"],
    ));
    Ok(journey)
}

pub fn build_base_profile_running_screen_journey(
    repo_root: &Path,
    journey_name: &str,
    _args: &Value,
) -> Result<MockUserJourneySpec> {
    let mut journey =
        build_base_profile_confirm_screen_journey(repo_root, journey_name, &json!({}))?;
    journey.steps.push(MockUserJourneyStep::key(
        "start profiling",
        "enter",
        800,
        ["Profiling In Progress", "Stage:", "Preparing"],
    ));
    Ok(journey)
}

pub fn build_base_profile_failure_screen_journey(
    repo_root: &Path,
    journey_name: &str,
    _args: &Value,
) -> Result<MockUserJourneySpec> {
    let mut journey =
        build_base_profile_advisory_screen_journey(repo_root, journey_name, &json!({}))?;
    journey.steps.push(MockUserJourneyStep::key(
        "open profiling failure",
        "enter",
        800,
        ["Profiling Failed", "Suggestions", "Recovery Actions"],
    ));
    Ok(journey)
}

pub fn build_base_ozone_plus_shell_journey(
    repo_root: &Path,
    journey_name: &str,
    _args: &Value,
) -> Result<MockUserJourneySpec> {
    Ok(MockUserJourneySpec {
        name: journey_name.to_owned(),
        cwd: repo_root.to_string_lossy().into_owned(),
        command: front_door_binary_command(repo_root, "ozone", &["--no-browser"]),
        steps: vec![
            MockUserJourneyStep::wait_for(
                "reach launcher or ready splash",
                35000,
                ["Continue", "Open ozone+", "Launch ozone+"],
            ),
            MockUserJourneyStep::key(
                "enter launcher",
                "enter",
                1000,
                ["Open ozone+", "Launch ozone+"],
            ),
            MockUserJourneyStep::key("select ozone+", "down", 1000, []),
            MockUserJourneyStep::key(
                "open ozone+ shell",
                "enter",
                4500,
                ["New Chat", "Sessions", "Characters", "Settings"],
            ),
        ],
    })
}

pub fn build_ozone_plus_main_menu_screen_journey(
    repo_root: &Path,
    journey_name: &str,
    _args: &Value,
) -> Result<MockUserJourneySpec> {
    Ok(MockUserJourneySpec {
        name: journey_name.to_owned(),
        cwd: repo_root.to_string_lossy().into_owned(),
        command: front_door_binary_command(
            repo_root,
            "ozone-plus",
            &["handoff", "--launcher-session"],
        ),
        steps: vec![MockUserJourneyStep::wait_for(
            "settle main menu",
            1200,
            ["New Chat", "Sessions", "Characters", "Settings"],
        )],
    })
}

pub fn build_ozone_plus_sessions_screen_journey(
    repo_root: &Path,
    journey_name: &str,
    _args: &Value,
) -> Result<MockUserJourneySpec> {
    let mut journey =
        build_ozone_plus_main_menu_screen_journey(repo_root, journey_name, &json!({}))?;
    journey.steps.push(MockUserJourneyStep::text(
        "open sessions",
        "2",
        500,
        ["Sessions", "0 total"],
    ));
    Ok(journey)
}

pub fn build_ozone_plus_characters_screen_journey(
    repo_root: &Path,
    journey_name: &str,
    _args: &Value,
) -> Result<MockUserJourneySpec> {
    let mut journey =
        build_ozone_plus_main_menu_screen_journey(repo_root, journey_name, &json!({}))?;
    journey.steps.push(MockUserJourneyStep::text(
        "open characters",
        "3",
        500,
        ["Characters", "session(s)"],
    ));
    Ok(journey)
}

pub fn build_ozone_plus_settings_screen_journey(
    repo_root: &Path,
    journey_name: &str,
    _args: &Value,
) -> Result<MockUserJourneySpec> {
    let mut journey =
        build_ozone_plus_main_menu_screen_journey(repo_root, journey_name, &json!({}))?;
    journey.steps.push(MockUserJourneyStep::text(
        "open settings",
        "4",
        500,
        ["Settings", "config.toml", "next session open"],
    ));
    Ok(journey)
}

pub fn build_ozone_plus_character_create_screen_journey(
    repo_root: &Path,
    journey_name: &str,
    _args: &Value,
) -> Result<MockUserJourneySpec> {
    let mut journey =
        build_ozone_plus_characters_screen_journey(repo_root, journey_name, &json!({}))?;
    journey.steps.push(MockUserJourneyStep::text(
        "open character create",
        "n",
        600,
        ["New Character", "System Prompt", "Save"],
    ));
    Ok(journey)
}

pub fn build_ozone_plus_character_import_screen_journey(
    repo_root: &Path,
    journey_name: &str,
    _args: &Value,
) -> Result<MockUserJourneySpec> {
    let mut journey =
        build_ozone_plus_characters_screen_journey(repo_root, journey_name, &json!({}))?;
    journey.steps.push(MockUserJourneyStep::text(
        "open character import",
        "i",
        600,
        ["Import Character Card", "File Path", "Supports:"],
    ));
    Ok(journey)
}

pub fn build_ozone_plus_conversation_screen_journey(
    repo_root: &Path,
    journey_name: &str,
    _args: &Value,
) -> Result<MockUserJourneySpec> {
    let mut journey =
        build_ozone_plus_main_menu_screen_journey(repo_root, journey_name, &json!({}))?;
    journey.steps.push(MockUserJourneyStep::key(
        "open conversation",
        "enter",
        800,
        ["Conversation", "Composer", "Status"],
    ));
    Ok(journey)
}

pub fn build_ozone_plus_help_screen_journey(
    repo_root: &Path,
    journey_name: &str,
    _args: &Value,
) -> Result<MockUserJourneySpec> {
    let mut journey =
        build_ozone_plus_conversation_screen_journey(repo_root, journey_name, &json!({}))?;
    journey.steps.push(MockUserJourneyStep::text(
        "open help",
        "?",
        600,
        ["Help", "Slash Commands", "Ctrl+K"],
    ));
    Ok(journey)
}
