// Imports in this block are used by child modules via `use super::*;`.
// The compiler cannot detect this cross-module usage, so we suppress
// the false-positive `unused_imports` warnings.
#[allow(unused_imports)]
use std::time::Instant;

#[allow(unused_imports)]
use crate::catalog::CatalogRecord;
#[allow(unused_imports)]
use crate::hardware::HardwareProfile;
#[allow(unused_imports)]
use crate::launch_config::LaunchPlan;
#[allow(unused_imports)]
use crate::llamacpp::ServiceStatus;
#[allow(unused_imports)]
use crate::prefs::{Preferences, SavedLaunchProfile};
#[cfg(feature = "profiling-ui")]
#[allow(unused_imports)]
use crate::profiling::{
    ProfilingAction, ProfilingAdvisory, ProfilingFailureReport, ProfilingSuccessReport,
    WorkflowEvent,
};
use anyhow::Result;
use serde::{de, Deserialize, Deserializer, Serialize};
#[cfg(feature = "profiling-ui")]
#[allow(unused_imports)]
use tokio::sync::mpsc::UnboundedReceiver;
#[cfg(feature = "profiling-ui")]
#[allow(unused_imports)]
use tokio_util::sync::CancellationToken;

mod backend_args;
#[cfg(feature = "eval")]
mod bench_eval;
#[cfg(feature = "eval")]
mod bench_eval_flow;
#[cfg(feature = "eval")]
pub mod bench_eval_workflow;
mod bench_launcher;
mod catalog_flow;
mod command_overlay_flow;
mod configure_hub_flow;
mod configure_plan_flow;
mod configure_profile_flow;
mod confirm_flow;
#[cfg(feature = "eval")]
mod eval_launcher;
#[cfg(feature = "eval")]
pub mod eval_run_workflow;
mod exit_confirm_flow;
mod launch_execution_flow;
pub mod launcher;
mod launcher_loop;
pub use self::launcher_loop::run_launcher;
mod launcher_screens;
mod launcher_settings;
#[cfg(feature = "profiling-ui")]
mod launcher_profile_views;
mod launcher_screen_flow;
mod model_picker_flow;
pub mod monitor;
mod monitor_flow;
#[cfg(feature = "profiling-ui")]
mod profiling_entry_flow;
#[cfg(feature = "profiling-ui")]
mod profiling_result_flow;
mod settings_flow;
mod settings_screen_flow;
pub mod splash;
mod splash_flow;
pub mod tier_install;
pub mod tier_picker;
mod tier_picker_flow;
pub mod results;

#[cfg(feature = "eval")]
use self::bench_eval_flow::{handle_bench_eval_key, BenchEvalOutcome};
#[cfg(feature = "eval")]
use self::bench_eval_workflow::apply_bench_eval_event;
#[cfg(feature = "eval")]
use self::bench_launcher::{handle_key as handle_bench_launcher_key, BenchLauncherOutcome};
use self::catalog_flow::apply_catalog_report;
#[cfg(test)]
use self::catalog_flow::{apply_catalog_refresh, selected_catalog_name};
#[cfg(test)]
use self::command_overlay_flow::normalize_command_overlay;
use self::command_overlay_flow::{
    handle_command_overlay_key, open_command_overlay, overlay_supported,
};
use self::configure_hub_flow::handle_configure_hub_key;
#[cfg(test)]
use self::configure_plan_flow::{adjust_configure_plan, reset_configure_plan};
#[cfg(test)]
use self::configure_profile_flow::build_effective_plan;
use self::confirm_flow::handle_confirm_key;
#[cfg(feature = "eval")]
use self::eval_launcher::{handle_key as handle_eval_launcher_key, EvalLauncherOutcome};
#[cfg(feature = "eval")]
use self::eval_run_workflow::apply_eval_run_event;
use self::exit_confirm_flow::{handle_exit_confirm_key, ExitConfirmOutcome};
use self::launch_execution_flow::{
    handle_pending_frontend_launch, run_launcher_action, PendingFrontendLaunchOutcome,
};
use self::launcher_screen_flow::handle_launcher_screen_key;
use self::model_picker_flow::handle_model_picker_key;
use self::monitor_flow::{handle_monitor_key, MonitorOutcome};
#[cfg(feature = "profiling-ui")]
use self::profiling_entry_flow::{handle_profile_advisory_key, handle_profile_confirm_key};
#[cfg(feature = "profiling-ui")]
use self::profiling_result_flow::{
    apply_workflow_event, handle_profile_failure_key, handle_profile_running_key,
    handle_profile_success_key, ProfilingResultOutcome,
};
#[cfg(test)]
use self::settings_flow::back_from_confirm;
use self::settings_flow::{open_exit_confirm, open_settings, sync_settings_from_prefs};
use self::settings_screen_flow::handle_settings_key;
use self::splash_flow::handle_splash_key;
use self::tier_picker_flow::{handle_tier_picker_key, TierPickerOutcome};

pub mod screen;
pub use self::screen::Screen;

pub mod terminal;
use self::terminal::*;

mod app;
pub use self::app::{App, BenchEvalState};

#[derive(Debug, Clone, PartialEq)]
pub enum ModelPickerMode {
    Launch,
    Configure,
    #[cfg(feature = "profiling-ui")]
    Profile,
    BenchEval,
    EvalLauncher,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LauncherActionId {
    Launch,
    ConfigureModel,
    #[cfg(feature = "profiling-ui")]
    ProfileModel,
    BenchLauncher,
    EvalLauncher,
    Results,
    Settings,
    ClearGpu,
    Monitor,
    Exit,
}

#[derive(Debug, Clone)]
pub struct LauncherAction {
    pub id: LauncherActionId,
    pub label: String,
    pub description: String,
    pub command: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum BackendMode {
    LlamaCpp,
}

impl<'de> Deserialize<'de> for BackendMode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        const LLAMACPP_BACKEND_NAME: &str = "llama-cpp";
        const LEGACY_KOBOLD_BACKEND_NAME: &str = "kobold-cpp";
        const LEGACY_OLLAMA_BACKEND_NAME: &str = "ollama";

        let raw = String::deserialize(deserializer)?;
        match raw.as_str() {
            LLAMACPP_BACKEND_NAME | LEGACY_KOBOLD_BACKEND_NAME | LEGACY_OLLAMA_BACKEND_NAME => {
                Ok(Self::LlamaCpp)
            }
            _ => Err(de::Error::unknown_variant(
                &raw,
                &[
                    LLAMACPP_BACKEND_NAME,
                    LEGACY_KOBOLD_BACKEND_NAME,
                    LEGACY_OLLAMA_BACKEND_NAME,
                ],
            )),
        }
    }
}

mod plan_builder;
use plan_builder::{next_screen_after_splash, queue_launch, LauncherActionOutcome, selected_record};

pub use monitor_flow::run_monitor;

#[cfg(test)]
mod tests;
