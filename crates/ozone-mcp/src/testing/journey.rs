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
            "preferredBackend": "KoboldCpp"
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
            "preferredBackend": "KoboldCpp"
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
            "preferredBackend": "KoboldCpp",
            "preferredFrontend": "OzonePlus"
        })),
        true,
        false,
    )
}

/// Sandbox setup for ozone+ entry - minimal config for ozone+ direct handoff.
pub fn sandbox_setup_ozone_plus_entry() -> Value {
    sandbox_setup(&[], None, false, false)
}