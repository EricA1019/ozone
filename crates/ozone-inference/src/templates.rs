//! Prompt-template loading and rendering via minijinja.
//!
//! Templates are loaded from (in priority order):
//!   1. A custom user template directory (e.g. `$XDG_CONFIG_HOME/ozone/templates/`)
//!   2. Built-in templates embedded in the binary
//!
//! Templates are addressed by name (e.g. `"chatml"`, `"alpaca"`).

use std::collections::HashMap;
use std::path::Path;

use minijinja::{context, Environment, Value};
use serde::{Deserialize, Serialize};

use crate::error::InferenceError;

// ---------------------------------------------------------------------------
// Embedded built-in templates
// ---------------------------------------------------------------------------

const BUILTIN_CHATML: &str = include_str!("../../../templates/chatml.jinja");
const BUILTIN_ALPACA: &str = include_str!("../../../templates/alpaca.jinja");
const BUILTIN_LLAMA3: &str = include_str!("../../../templates/llama3-instruct.jinja");

/// A message in the prompt-building context.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TemplateMessage {
    pub role: String,
    pub content: String,
}

impl TemplateMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".into(),
            content: content.into(),
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".into(),
            content: content.into(),
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: "assistant".into(),
            content: content.into(),
        }
    }
}

/// Registry of available prompt-format templates.
///
/// Build with [`TemplateRegistry::builder()`] to control which directories
/// are searched before falling back to built-ins.
pub struct TemplateRegistry {
    env: Environment<'static>,
}

impl TemplateRegistry {
    /// Create a registry pre-loaded with only the built-in templates.
    pub fn with_builtins() -> anyhow::Result<Self> {
        Self::builder().build()
    }

    pub fn builder() -> TemplateRegistryBuilder {
        TemplateRegistryBuilder::default()
    }

    /// Render a named template with the given messages.
    ///
    /// Returns the fully formatted prompt string.
    pub fn render(
        &self,
        template_name: &str,
        messages: &[TemplateMessage],
    ) -> anyhow::Result<String> {
        let tmpl =
            self.env
                .get_template(template_name)
                .map_err(|e| InferenceError::PromptTemplate {
                    template: template_name.to_string(),
                    reason: e.to_string(),
                })?;

        let ctx = context! { messages => Value::from_serialize(messages) };
        tmpl.render(ctx)
            .map_err(|e| InferenceError::PromptTemplate {
                template: template_name.to_string(),
                reason: e.to_string(),
            })
            .map_err(Into::into)
    }

    /// List all registered template names.
    pub fn available_templates(&self) -> Vec<String> {
        BUILTIN_NAMES.iter().map(|s| s.to_string()).collect()
    }
}

const BUILTIN_NAMES: &[&str] = &["chatml", "alpaca", "llama3-instruct"];

/// Builder for `TemplateRegistry`.
#[derive(Debug, Default)]
pub struct TemplateRegistryBuilder {
    extra_dirs: Vec<std::path::PathBuf>,
    /// Name → template source overrides, for tests.
    overrides: HashMap<String, String>,
}

impl TemplateRegistryBuilder {
    /// Add a filesystem directory to search for `.jinja` template files.
    /// Files in this directory take priority over built-ins with the same name.
    pub fn custom_template_dir(mut self, dir: impl AsRef<Path>) -> Self {
        self.extra_dirs.push(dir.as_ref().to_path_buf());
        self
    }

    /// Inject a template source directly (primarily for testing).
    pub fn add_template(mut self, name: impl Into<String>, source: impl Into<String>) -> Self {
        self.overrides.insert(name.into(), source.into());
        self
    }

    pub fn build(self) -> anyhow::Result<TemplateRegistry> {
        let mut env = Environment::new();

        // Load built-ins first (lowest priority — overridden below).
        env.add_template("chatml", BUILTIN_CHATML)
            .map_err(|e| InferenceError::PromptTemplate {
                template: "chatml".into(),
                reason: e.to_string(),
            })?;
        env.add_template("alpaca", BUILTIN_ALPACA)
            .map_err(|e| InferenceError::PromptTemplate {
                template: "alpaca".into(),
                reason: e.to_string(),
            })?;
        env.add_template("llama3-instruct", BUILTIN_LLAMA3)
            .map_err(|e| InferenceError::PromptTemplate {
                template: "llama3-instruct".into(),
                reason: e.to_string(),
            })?;

        // Load from custom directories (override built-ins with the same stem).
        for dir in &self.extra_dirs {
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|e| e.to_str()) == Some("jinja") {
                        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                            match std::fs::read_to_string(&path) {
                                Ok(src) => {
                                    // env.add_template leaks the string for 'static;
                                    // we store owned copies via add_template_owned.
                                    env.add_template_owned(stem.to_string(), src).map_err(|e| {
                                        InferenceError::PromptTemplate {
                                            template: stem.to_string(),
                                            reason: e.to_string(),
                                        }
                                    })?;
                                }
                                Err(e) => {
                                    tracing::warn!("failed to read template {:?}: {e}", path);
                                }
                            }
                        }
                    }
                }
            }
        }

        // Programmatic overrides (highest priority — used in tests).
        for (name, src) in self.overrides {
            env.add_template_owned(name, src)
                .map_err(|e| InferenceError::PromptTemplate {
                    template: "override".into(),
                    reason: e.to_string(),
                })?;
        }

        Ok(TemplateRegistry { env })
    }
}

// ---------------------------------------------------------------------------
// Template auto-detection heuristic
// ---------------------------------------------------------------------------

/// Guess the best template name based on a model name string.
///
/// Falls back to `"chatml"` if no pattern matches.
pub fn detect_template(model_name: &str) -> &'static str {
    let lower = model_name.to_lowercase();
    if lower.contains("llama-3") || lower.contains("llama3") || lower.contains("meta-llama") {
        "llama3-instruct"
    } else if lower.contains("alpaca") || lower.contains("vicuna") || lower.contains("orca") {
        "alpaca"
    } else {
        "chatml"
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_messages() -> Vec<TemplateMessage> {
        vec![
            TemplateMessage::system("You are a helpful assistant."),
            TemplateMessage::user("Hello!"),
            TemplateMessage::assistant("Hi there!"),
            TemplateMessage::user("What is 2+2?"),
        ]
    }

    #[test]
    fn chatml_contains_control_tokens() {
        let reg = TemplateRegistry::with_builtins().unwrap();
        let out = reg.render("chatml", &make_messages()).unwrap();
        assert!(
            out.contains("<|im_start|>system"),
            "has system header: {out}"
        );
        assert!(out.contains("<|im_end|>"), "has end token: {out}");
        assert!(
            out.contains("<|im_start|>assistant"),
            "ends with assistant turn: {out}"
        );
        assert!(
            out.contains("You are a helpful assistant."),
            "has system content: {out}"
        );
        assert!(out.contains("Hello!"), "has user content: {out}");
    }

    #[test]
    fn alpaca_contains_instruction_markers() {
        let reg = TemplateRegistry::with_builtins().unwrap();
        let out = reg.render("alpaca", &make_messages()).unwrap();
        assert!(
            out.contains("### Instruction:"),
            "has instruction marker: {out}"
        );
        assert!(out.contains("### Response:"), "has response marker: {out}");
        assert!(out.contains("Hello!"), "has user content: {out}");
    }

    #[test]
    fn llama3_contains_special_tokens() {
        let reg = TemplateRegistry::with_builtins().unwrap();
        let out = reg.render("llama3-instruct", &make_messages()).unwrap();
        assert!(out.contains("<|begin_of_text|>"), "has BOS: {out}");
        assert!(
            out.contains("<|start_header_id|>"),
            "has header start: {out}"
        );
        assert!(out.contains("<|eot_id|>"), "has EOT: {out}");
    }

    #[test]
    fn unknown_template_returns_error() {
        let reg = TemplateRegistry::with_builtins().unwrap();
        let err = reg.render("nonexistent", &make_messages()).unwrap_err();
        assert!(err.to_string().contains("nonexistent") || err.to_string().contains("template"));
    }

    #[test]
    fn programmatic_override_wins_over_builtin() {
        let custom_src = "CUSTOM:{% for m in messages %}{{m.content}}|{% endfor %}";
        let reg = TemplateRegistry::builder()
            .add_template("chatml", custom_src)
            .build()
            .unwrap();
        let out = reg
            .render("chatml", &[TemplateMessage::user("hi")])
            .unwrap();
        assert!(out.starts_with("CUSTOM:"), "custom template used: {out}");
    }

    #[test]
    fn template_auto_detect() {
        assert_eq!(
            detect_template("Meta-Llama-3.1-8B-Instruct"),
            "llama3-instruct"
        );
        assert_eq!(detect_template("llama3-70b"), "llama3-instruct");
        assert_eq!(detect_template("alpaca-7b"), "alpaca");
        assert_eq!(detect_template("mistral-7b-v0.1"), "chatml");
        assert_eq!(detect_template("unknown-model"), "chatml");
    }

    #[test]
    fn available_templates_lists_builtins() {
        let reg = TemplateRegistry::with_builtins().unwrap();
        let names = reg.available_templates();
        assert!(names.contains(&"chatml".to_string()));
        assert!(names.contains(&"alpaca".to_string()));
        assert!(names.contains(&"llama3-instruct".to_string()));
    }
}
