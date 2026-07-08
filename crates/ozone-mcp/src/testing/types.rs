use serde::{Deserialize, Serialize};
use serde_json::Value;

// =============================================================================
// PTY VTE Capture Types
// =============================================================================

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PtyVteCaptureConfig {
    pub rows: u16,
    pub columns: u16,
    #[serde(rename = "tailChars")]
    pub tail_chars: usize,
    #[serde(rename = "fontSize")]
    pub font_size: u16,
    #[serde(rename = "pngPath", skip_serializing_if = "Option::is_none")]
    pub png_path: Option<String>,
    #[serde(rename = "jsonPath", skip_serializing_if = "Option::is_none")]
    pub json_path: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
pub struct PtyVteCaptureArtifacts {
    #[serde(rename = "pngPath")]
    pub png_path: String,
    #[serde(rename = "jsonPath")]
    pub json_path: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct MockUserCaptureSettings {
    pub capture: PtyVteCaptureConfig,
    #[serde(rename = "captureScreenshots")]
    pub capture_screenshots: bool,
    #[serde(rename = "outputDir", skip_serializing_if = "Option::is_none")]
    pub output_dir: Option<String>,
    #[serde(rename = "stepCaptures", skip_serializing_if = "Vec::is_empty")]
    pub step_captures: Vec<PtyVteCaptureArtifacts>,
}

// Serde deserialization-only struct — never directly constructed.
#[allow(dead_code)]
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PtyVteCaptureResult {
    pub screen_rows: u16,
    pub screen_columns: u16,
    pub line_count: u16,
    pub column_count: u16,
    pub cursor: PtyVteCursor,
    pub cursor_row: u16,
    pub cursor_col: u16,
    #[serde(default)]
    pub display: Vec<String>,
    pub text: String,
    #[serde(rename = "tailText")]
    pub tail_text: String,
    #[serde(default)]
    pub rows: Vec<PtyVteCaptureRow>,
    #[serde(default)]
    pub grid: Vec<PtyVteCaptureRow>,
    #[serde(rename = "pngPath", default, skip_serializing_if = "Option::is_none")]
    pub png_path: Option<String>,
    #[serde(rename = "jsonPath", default, skip_serializing_if = "Option::is_none")]
    pub json_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font: Option<PtyVteCaptureFont>,
}

// Serde deserialization-only struct — never directly constructed.
#[allow(dead_code)]
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PtyVteCursor {
    pub row: u16,
    pub column: u16,
}

// Serde deserialization-only struct — never directly constructed.
#[allow(dead_code)]
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PtyVteCaptureRow {
    #[serde(default)]
    pub index: u16,
    #[serde(default)]
    pub row: Option<u16>,
    pub text: String,
    pub cells: Vec<PtyVteCaptureCell>,
}

// Serde deserialization-only struct — never directly constructed.
#[allow(dead_code)]
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PtyVteCaptureCell {
    pub column: u16,
    pub text: String,
    pub fg: String,
    pub bg: String,
    pub bold: bool,
    pub italics: bool,
    pub underscore: bool,
    pub strikethrough: bool,
    pub blink: bool,
    pub reverse: bool,
    #[serde(default)]
    pub resolved_fg: Vec<u8>,
    #[serde(default)]
    pub resolved_bg: Vec<u8>,
}

// Serde deserialization-only struct — never directly constructed.
#[allow(dead_code)]
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PtyVteCaptureFont {
    pub family: String,
    pub size: u16,
    pub cell_width: u32,
    pub cell_height: u32,
}

// =============================================================================
// Screen Check Types
// =============================================================================

#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
pub struct ScreenRegion {
    pub top: u16,
    pub left: u16,
    pub bottom: u16,
    pub right: u16,
}

#[derive(Debug, Serialize)]
pub struct ScreenCheckOutcome {
    pub index: usize,
    #[serde(rename = "type")]
    pub check_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub passed: bool,
    pub summary: String,
    pub detail: Value,
}

#[derive(Debug, Clone, Copy)]
pub struct ScreenColorMatch<'a> {
    pub raw: &'a str,
    pub resolved: &'a [u8],
}

#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ComparableScreenCell {
    pub text: String,
    pub fg: String,
    pub bg: String,
    pub bold: bool,
    pub italics: bool,
    pub underscore: bool,
    pub strikethrough: bool,
    pub blink: bool,
    pub reverse: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BaselineCompareDiff {
    pub row: u16,
    pub column: u16,
    pub kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub baseline: Option<ComparableScreenCell>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actual: Option<ComparableScreenCell>,
}

// =============================================================================
// Mock User Journey Types
// =============================================================================

#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
pub struct MockUserJourneySpec {
    pub name: String,
    pub cwd: String,
    pub command: Vec<String>,
    pub steps: Vec<MockUserJourneyStep>,
}

#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
pub struct MockUserJourneyStep {
    pub name: String,
    pub action: MockUserAction,
    #[serde(rename = "settleMs")]
    pub settle_ms: u64,
    #[serde(rename = "expectAny")]
    pub expect_any: Vec<String>,
}

impl MockUserJourneyStep {
    pub fn wait(name: &str, ms: u64) -> Self {
        Self::wait_for(name, ms, [])
    }

    pub fn wait_for(
        name: &str,
        ms: u64,
        expect_any: impl IntoIterator<Item = &'static str>,
    ) -> Self {
        Self {
            name: name.to_owned(),
            action: MockUserAction::Wait { ms },
            settle_ms: 0,
            expect_any: expect_any.into_iter().map(str::to_owned).collect(),
        }
    }

    pub fn key(
        name: &str,
        key: &str,
        settle_ms: u64,
        expect_any: impl IntoIterator<Item = &'static str>,
    ) -> Self {
        Self {
            name: name.to_owned(),
            action: MockUserAction::Key {
                key: key.to_owned(),
            },
            settle_ms,
            expect_any: expect_any.into_iter().map(str::to_owned).collect(),
        }
    }

    pub fn text(
        name: &str,
        text: &str,
        settle_ms: u64,
        expect_any: impl IntoIterator<Item = &'static str>,
    ) -> Self {
        Self {
            name: name.to_owned(),
            action: MockUserAction::Text {
                text: text.to_owned(),
            },
            settle_ms,
            expect_any: expect_any.into_iter().map(str::to_owned).collect(),
        }
    }
}

#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MockUserAction {
    Wait { ms: u64 },
    Key { key: String },
    Text { text: String },
}

#[derive(Debug, Serialize)]
#[cfg_attr(not(feature = "legacy-tools"), allow(dead_code))]
pub struct LauncherSmokeRunnerSpec {
    #[serde(rename = "repoRoot")]
    pub repo_root: String,
    #[serde(rename = "liveRefreshPath", skip_serializing_if = "Option::is_none")]
    pub live_refresh_path: Option<String>,
    #[serde(rename = "enterCount")]
    pub enter_count: u64,
    pub capture: PtyVteCaptureConfig,
}

#[derive(Debug, Serialize)]
pub struct MockUserRunnerSpec {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    pub cwd: String,
    pub command: Vec<String>,
    pub steps: Vec<MockUserJourneyStep>,
    #[serde(flatten)]
    pub capture_settings: MockUserCaptureSettings,
}

// =============================================================================
// Journey Definition Types
// =============================================================================

pub type CapturableScreenJourneyBuilder =
    fn(&std::path::Path, &str, &Value) -> anyhow::Result<MockUserJourneySpec>;
pub type CapturableScreenSandboxSetup = fn() -> Value;

pub struct CapturableScreenJourneyDefinition {
    pub target_screen: &'static str,
    pub description: &'static str,
    pub(crate) builder: CapturableScreenJourneyBuilder,
    pub sandbox_setup: CapturableScreenSandboxSetup,
}

// =============================================================================
// Sandbox Types
// =============================================================================

#[derive(Clone, Debug)]
pub struct PreparedSandbox {
    pub sandbox_id: String,
    pub auto_created: bool,
    pub auto_started_mock_backend: bool,
    pub setup_applied: Option<Value>,
}
