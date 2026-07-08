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

use super::{MockUserJourneySpec, MockUserJourneyStep, CapturableScreenJourneyDefinition };

/// Helper function to append arguments to a command vector.
// Used through the MCP testing journey builder; not directly invoked from production paths.
#[allow(dead_code)]
pub fn append_args(command: &[String], args: &[&str]) -> Vec<String> {
    let mut full = command.to_vec();
    full.extend(args.iter().map(|value| (*value).to_owned()));
    full
}

/// Resolve binary command, preferring debug build if available, falling back to cargo run.
// Used through the MCP testing journey builder; not directly invoked from production paths.
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
) -> Result<&'static crate::CapturableScreenJourneyDefinition> {
    capturable_screen_journey_builders()
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
    definition: &crate::CapturableScreenJourneyDefinition,
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


// ── Capturable journey builders (extracted from lib.rs) ──

pub fn capturable_screen_journey_builders() -> &'static [CapturableScreenJourneyDefinition]
{
    &[
        CapturableScreenJourneyDefinition {
            target_screen: "base_splash",
            description: "Cold-start Ozone splash screen.",
            builder: build_base_splash_screen_journey,
            sandbox_setup: sandbox_setup_base_splash,
        },
        CapturableScreenJourneyDefinition {
            target_screen: "base_tier_picker",
            description: "First-run tier picker between splash and launcher.",
            builder: build_base_tier_picker_screen_journey,
            sandbox_setup: sandbox_setup_base_tier_picker,
        },
        CapturableScreenJourneyDefinition {
            target_screen: "base_launcher",
            description: "Base Ozone launcher dashboard.",
            builder: build_base_launcher_screen_journey,
            sandbox_setup: sandbox_setup_base_launcher,
        },
        CapturableScreenJourneyDefinition {
            target_screen: "base_exit_confirm",
            description: "Launcher exit confirmation dialog.",
            builder: build_base_exit_confirm_screen_journey,
            sandbox_setup: sandbox_setup_base_launcher,
        },
        CapturableScreenJourneyDefinition {
            target_screen: "base_settings",
            description: "Base Ozone settings screen.",
            builder: build_base_settings_screen_journey,
            sandbox_setup: sandbox_setup_base_launcher,
        },
        CapturableScreenJourneyDefinition {
            target_screen: "base_model_picker_launch",
            description: "Launch-mode model picker.",
            builder: build_base_model_picker_launch_screen_journey,
            sandbox_setup: sandbox_setup_base_launch_path,
        },
        CapturableScreenJourneyDefinition {
            target_screen: "base_confirm_launch",
            description: "Launch confirmation dialog before backend start.",
            builder: build_base_confirm_launch_screen_journey,
            sandbox_setup: sandbox_setup_base_launch_path,
        },
        CapturableScreenJourneyDefinition {
            target_screen: "base_frontend_choice",
            description: "Frontend choice screen shown when no frontend is preselected.",
            builder: build_base_frontend_choice_screen_journey,
            sandbox_setup: sandbox_setup_base_launch_path,
        },
        CapturableScreenJourneyDefinition {
            target_screen: "base_launching",
            description: "Transient launch-progress screen after confirming frontend.",
            builder: build_base_launching_screen_journey,
            sandbox_setup: sandbox_setup_base_launch_path,
        },
        CapturableScreenJourneyDefinition {
            target_screen: "base_monitor",
            description: "Live Ozone monitor screen.",
            builder: build_base_monitor_screen_journey,
            sandbox_setup: sandbox_setup_base_launch_path,
        },
        CapturableScreenJourneyDefinition {
            target_screen: "base_model_picker_profile",
            description: "Profile-mode model picker.",
            builder: build_base_model_picker_profile_screen_journey,
            sandbox_setup: sandbox_setup_base_profile_review,
        },
        CapturableScreenJourneyDefinition {
            target_screen: "base_profile_advisory",
            description: "Profiling advisor overview.",
            builder: build_base_profile_advisory_screen_journey,
            sandbox_setup: sandbox_setup_base_profile_review,
        },
        CapturableScreenJourneyDefinition {
            target_screen: "base_profile_confirm",
            description: "Profiling action confirmation dialog.",
            builder: build_base_profile_confirm_screen_journey,
            sandbox_setup: sandbox_setup_base_profile_run,
        },
        CapturableScreenJourneyDefinition {
            target_screen: "base_profile_running",
            description: "Profiling in-progress screen.",
            builder: build_base_profile_running_screen_journey,
            sandbox_setup: sandbox_setup_base_profile_run,
        },
        CapturableScreenJourneyDefinition {
            target_screen: "base_profile_failure",
            description: "Profiling failure / issue-report screen.",
            builder: build_base_profile_failure_screen_journey,
            sandbox_setup: sandbox_setup_base_profile_review,
        },
        CapturableScreenJourneyDefinition {
            target_screen: "base_ozone_plus_shell",
            description: "ozone+ conversation shell reached through the base launcher handoff.",
            builder: build_base_ozone_plus_shell_journey,
            sandbox_setup: sandbox_setup_base_ozone_plus_shell,
        },
        CapturableScreenJourneyDefinition {
            target_screen: "ozone_plus_main_menu",
            description: "ozone+ main menu from direct handoff.",
            builder: build_ozone_plus_main_menu_screen_journey,
            sandbox_setup: sandbox_setup_ozone_plus_entry,
        },
        CapturableScreenJourneyDefinition {
            target_screen: "ozone_plus_sessions",
            description: "ozone+ session list screen.",
            builder: build_ozone_plus_sessions_screen_journey,
            sandbox_setup: sandbox_setup_ozone_plus_entry,
        },
        CapturableScreenJourneyDefinition {
            target_screen: "ozone_plus_characters",
            description: "ozone+ character manager screen.",
            builder: build_ozone_plus_characters_screen_journey,
            sandbox_setup: sandbox_setup_ozone_plus_entry,
        },
        CapturableScreenJourneyDefinition {
            target_screen: "ozone_plus_character_create",
            description: "ozone+ new-character form.",
            builder: build_ozone_plus_character_create_screen_journey,
            sandbox_setup: sandbox_setup_ozone_plus_entry,
        },
        CapturableScreenJourneyDefinition {
            target_screen: "ozone_plus_character_import",
            description: "ozone+ import-character form.",
            builder: build_ozone_plus_character_import_screen_journey,
            sandbox_setup: sandbox_setup_ozone_plus_entry,
        },
        CapturableScreenJourneyDefinition {
            target_screen: "ozone_plus_settings",
            description: "ozone+ settings/config screen.",
            builder: build_ozone_plus_settings_screen_journey,
            sandbox_setup: sandbox_setup_ozone_plus_entry,
        },
        CapturableScreenJourneyDefinition {
            target_screen: "ozone_plus_conversation",
            description: "ozone+ conversation shell from the main menu.",
            builder: build_ozone_plus_conversation_screen_journey,
            sandbox_setup: sandbox_setup_ozone_plus_entry,
        },
        CapturableScreenJourneyDefinition {
            target_screen: "ozone_plus_help",
            description: "ozone+ help overlay from conversation mode.",
            builder: build_ozone_plus_help_screen_journey,
            sandbox_setup: sandbox_setup_ozone_plus_entry,
        },
    ]
}

pub(crate) const PYTHON_PTY_VTE_HELPER: &str = r###"import json
import os
import pty
import select
import signal
import subprocess
import time
import fcntl
import struct
import termios
from pathlib import Path

SPEC = json.loads(__SPEC_JSON__)
CAPTURE = SPEC.get("capture") or {}
ROWS = int(CAPTURE.get("rows") or 40)
COLUMNS = int(CAPTURE.get("columns") or 120)
TAIL_CHARS = int(CAPTURE.get("tailChars") or 1600)
FONT_SIZE = int(CAPTURE.get("fontSize") or 16)
DEFAULT_FG = (229, 229, 229)
DEFAULT_BG = (12, 12, 12)
ANSI_RGB = {
    "default": DEFAULT_FG,
    "black": (12, 12, 12),
    "red": (205, 49, 49),
    "green": (13, 188, 121),
    "brown": (229, 229, 16),
    "yellow": (229, 229, 16),
    "blue": (36, 114, 200),
    "magenta": (188, 63, 188),
    "cyan": (17, 168, 205),
    "white": (229, 229, 229),
    "brightblack": (102, 102, 102),
    "brightred": (241, 76, 76),
    "brightgreen": (35, 209, 139),
    "brightyellow": (245, 245, 67),
    "brightblue": (59, 142, 234),
    "brightmagenta": (214, 112, 214),
    "brightcyan": (41, 184, 219),
    "brightwhite": (255, 255, 255),
}
KEY_BYTES = {
    "enter": b"\r",
    "esc": b"\x1b",
    "up": b"\x1b[A",
    "down": b"\x1b[B",
    "right": b"\x1b[C",
    "left": b"\x1b[D",
    "tab": b"\t",
}

try:
    import pyte
except ModuleNotFoundError:
    pyte = None

try:
    from PIL import Image, ImageDraw, ImageFont
except ModuleNotFoundError:
    Image = ImageDraw = ImageFont = None

def fail(message, *, missing_dependencies=None, **extra):
    payload = {"ok": False, "error": message}
    if missing_dependencies:
        payload["missingModules"] = list(missing_dependencies)
    payload.update(extra)
    print(json.dumps(payload, ensure_ascii=False))
    raise SystemExit(1)

missing_dependencies = []
if pyte is None:
    missing_dependencies.append("pyte")
if Image is None:
    missing_dependencies.append("Pillow")
if missing_dependencies:
    fail(
        "missing python dependencies for VTE capture helper: "
        + ", ".join(missing_dependencies)
        + ". Install with `python3 -m pip install pyte Pillow`.",
        missing_dependencies=missing_dependencies,
    )

screen = pyte.Screen(COLUMNS, ROWS)
stream = pyte.ByteStream(screen)
buffer = bytearray()

def ensure_parent(path):
    if path:
        Path(path).parent.mkdir(parents=True, exist_ok=True)

def child_env():
    env = os.environ.copy()
    env.setdefault("TERM", "xterm-color")
    env["LINES"] = str(ROWS)
    env["COLUMNS"] = str(COLUMNS)
    return env

def open_pty_process(command, cwd):
    master, slave = pty.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", ROWS, COLUMNS, 0, 0))
    proc = subprocess.Popen(
        command,
        cwd=cwd,
        stdin=slave,
        stdout=slave,
        stderr=slave,
        start_new_session=True,
        env=child_env(),
    )
    os.close(slave)
    return master, proc

def append_chunk(chunk):
    if not chunk:
        return False
    buffer.extend(chunk)
    stream.feed(chunk)
    return True

def pump(master, proc, seconds):
    end = time.time() + seconds
    while time.time() < end:
        timeout = min(0.2, max(0.0, end - time.time()))
        ready, _, _ = select.select([master], [], [], timeout)
        if master in ready:
            try:
                chunk = os.read(master, 65536)
            except OSError:
                break
            if not chunk:
                break
            append_chunk(chunk)
        if proc.poll() is not None and not ready:
            break

def send_key(master, key):
    payload = KEY_BYTES.get(key)
    if payload is None:
        fail(f"unsupported PTY key `{key}`")
    os.write(master, payload)

def send_text(master, text):
    os.write(master, text.encode("utf-8"))

def stop_process(proc):
    process_exited = proc.poll() is not None
    exit_code = proc.returncode if process_exited else None
    if not process_exited:
        try:
            os.killpg(proc.pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
        try:
            proc.wait(timeout=5)
        except Exception:
            try:
                os.killpg(proc.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
    return {
        "processExitedBeforeStop": process_exited,
        "exitCode": exit_code,
    }

def screen_text():
    return "\n".join(screen.display)

def screen_tail():
    return screen_text()[-TAIL_CHARS:]

def screen_contains(marker):
    return bool(marker) and marker in screen_text()

def font_candidates():
    return [
        os.environ.get("OZONE_MCP_VTE_FONT"),
        "DejaVuSansMono.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
        "/usr/share/fonts/dejavu/DejaVuSansMono.ttf",
        "LiberationMono-Regular.ttf",
        "/usr/share/fonts/truetype/liberation2/LiberationMono-Regular.ttf",
        "NotoSansMono-Regular.ttf",
        "/usr/share/fonts/truetype/noto/NotoSansMono-Regular.ttf",
    ]

def load_font():
    for candidate in font_candidates():
        if not candidate:
            continue
        try:
            return ImageFont.truetype(candidate, FONT_SIZE), candidate
        except OSError:
            continue
    return ImageFont.load_default(), "PillowDefault"

def resolve_color(name, fallback):
    if not isinstance(name, str):
        return fallback
    lower = name.lower()
    if lower == "default":
        return fallback
    return ANSI_RGB.get(lower, fallback)

def cell_style(char):
    fg = resolve_color(getattr(char, "fg", "default"), DEFAULT_FG)
    bg = resolve_color(getattr(char, "bg", "default"), DEFAULT_BG)
    reverse = bool(getattr(char, "reverse", False))
    if reverse:
        fg, bg = bg, fg
    return {
        "fg": getattr(char, "fg", "default"),
        "bg": getattr(char, "bg", "default"),
        "bold": bool(getattr(char, "bold", False)),
        "italics": bool(getattr(char, "italics", False)),
        "underscore": bool(getattr(char, "underscore", False)),
        "strikethrough": bool(getattr(char, "strikethrough", False)),
        "blink": bool(getattr(char, "blink", False)),
        "reverse": reverse,
        "resolvedFg": list(fg),
        "resolvedBg": list(bg),
    }

def serialize_screen():
    display = list(screen.display)
    rows = []
    for row_index in range(screen.lines):
        line = screen.buffer.get(row_index) or {}
        row_cells = []
        for column_index in range(screen.columns):
            char = line.get(column_index) or screen.default_char
            style = cell_style(char)
            row_cells.append(
                {
                    "column": column_index,
                    "text": getattr(char, "data", " "),
                    **style,
                }
            )
        rows.append(
            {
                "index": row_index,
                "row": row_index,
                "text": display[row_index],
                "cells": row_cells,
            }
        )
    text = "\n".join(display)
    return {
        "screenRows": screen.lines,
        "screenColumns": screen.columns,
        "lineCount": screen.lines,
        "columnCount": screen.columns,
        "cursor": {
            "row": screen.cursor.y,
            "column": screen.cursor.x,
        },
        "cursorRow": screen.cursor.y,
        "cursorCol": screen.cursor.x,
        "display": display,
        "text": text,
        "tailText": text[-TAIL_CHARS:],
        "rows": rows,
        "grid": rows,
    }

def render_screen_png(capture, png_path):
    ensure_parent(png_path)
    font, font_name = load_font()
    bbox = font.getbbox("M")
    cell_width = max(1, bbox[2] - bbox[0])
    cell_height = max(1, bbox[3] - bbox[1] + 2)
    baseline_y = -bbox[1]
    image = Image.new(
        "RGB",
        (capture["screenColumns"] * cell_width, capture["screenRows"] * cell_height),
        DEFAULT_BG,
    )
    draw = ImageDraw.Draw(image)
    for row in capture["grid"]:
        top = row.get("index", row["row"]) * cell_height
        for cell in row["cells"]:
            left = cell["column"] * cell_width
            fg = tuple(cell["resolvedFg"])
            bg = tuple(cell["resolvedBg"])
            if bg != DEFAULT_BG:
                draw.rectangle((left, top, left + cell_width, top + cell_height), fill=bg)
            text = cell["text"] or " "
            if text != " ":
                draw.text((left, top + baseline_y), text, font=font, fill=fg)
            if cell["underscore"]:
                underline_y = top + cell_height - 2
                draw.line((left, underline_y, left + cell_width - 1, underline_y), fill=fg, width=1)
            if cell["strikethrough"]:
                strike_y = top + (cell_height // 2)
                draw.line((left, strike_y, left + cell_width - 1, strike_y), fill=fg, width=1)
    image.save(png_path, format="PNG")
    capture["font"] = {
        "family": font_name,
        "size": FONT_SIZE,
        "cellWidth": cell_width,
        "cellHeight": cell_height,
    }

def capture_screen():
    capture = serialize_screen()
    png_path = CAPTURE.get("pngPath")
    json_path = CAPTURE.get("jsonPath")
    if png_path and not json_path:
        json_path = str(Path(png_path).with_suffix(".json"))
    if png_path:
        render_screen_png(capture, png_path)
        capture["pngPath"] = png_path
    if json_path:
        ensure_parent(json_path)
        with open(json_path, "w", encoding="utf-8") as handle:
            json.dump(capture, handle, ensure_ascii=False, indent=2)
        capture["jsonPath"] = json_path
    return capture

def summarize_capture(capture):
    return {
        "screenRows": capture["screenRows"],
        "screenColumns": capture["screenColumns"],
        "cursor": capture["cursor"],
        "tailText": capture["tailText"],
        "display": capture["display"],
        "pngPath": capture.get("pngPath"),
        "jsonPath": capture.get("jsonPath"),
        "font": capture.get("font"),
    }
"###;

pub(crate) const PYTHON_PTY_VTE_HELPER_TRAILER: &str = r###"if __name__ == "__main__":
    try:
        print(json.dumps(run(), ensure_ascii=False))
    except SystemExit:
        raise
    except Exception as exc:
        print(
            json.dumps(
                {
                    "ok": False,
                    "error": str(exc),
                    "errorType": type(exc).__name__,
                },
                ensure_ascii=False,
            )
        )
        raise SystemExit(1)
"###;

