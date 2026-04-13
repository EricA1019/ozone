use std::{fmt, path::PathBuf};

use ozone_inference::{
    detect_template, ConfigLoader, InferenceGateway, InferenceRequest, OzoneConfig,
    TemplateMessage, TemplateRegistry,
};

#[derive(Debug, Clone, Default)]
pub struct InferenceAdapterInit {
    pub global_config_path: Option<PathBuf>,
    pub session_config_path: Option<PathBuf>,
    pub custom_template_dir: Option<PathBuf>,
    pub template_override: Option<String>,
    pub model_hint: Option<String>,
    pub extra_config_toml: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TranscriptRole {
    System,
    User,
    Assistant,
}

impl TranscriptRole {
    pub fn parse(author_kind: &str) -> Option<Self> {
        match author_kind.trim().to_ascii_lowercase().as_str() {
            "system" => Some(Self::System),
            "user" => Some(Self::User),
            "assistant" => Some(Self::Assistant),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscriptTurn {
    pub role: TranscriptRole,
    pub content: String,
}

impl TranscriptTurn {
    pub fn new(role: TranscriptRole, content: impl Into<String>) -> Self {
        Self {
            role,
            content: content.into(),
        }
    }

    pub fn from_author_kind(
        author_kind: &str,
        content: impl Into<String>,
    ) -> Result<Self, InferenceAdapterError> {
        let role = TranscriptRole::parse(author_kind)
            .ok_or_else(|| InferenceAdapterError::InvalidRole(author_kind.to_string()))?;
        Ok(Self::new(role, content))
    }

    fn as_template_message(&self) -> TemplateMessage {
        match self.role {
            TranscriptRole::System => TemplateMessage::system(self.content.clone()),
            TranscriptRole::User => TemplateMessage::user(self.content.clone()),
            TranscriptRole::Assistant => TemplateMessage::assistant(self.content.clone()),
        }
    }
}

#[derive(Debug)]
pub enum InferenceAdapterError {
    Config(String),
    Template(String),
    Gateway(String),
    InvalidRole(String),
}

impl fmt::Display for InferenceAdapterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Config(msg) => write!(f, "config error: {msg}"),
            Self::Template(msg) => write!(f, "template error: {msg}"),
            Self::Gateway(msg) => write!(f, "gateway error: {msg}"),
            Self::InvalidRole(role) => write!(f, "unsupported transcript role: {role}"),
        }
    }
}

impl std::error::Error for InferenceAdapterError {}

pub struct InferenceAdapter {
    config: OzoneConfig,
    template_registry: TemplateRegistry,
    selected_template: String,
    gateway: InferenceGateway,
}

impl InferenceAdapter {
    pub fn load(init: InferenceAdapterInit) -> Result<Self, InferenceAdapterError> {
        let mut loader = ConfigLoader::new();
        if let Some(path) = init.global_config_path.as_ref() {
            loader = loader.global_config_path(path);
        }
        if let Some(path) = init.session_config_path.as_ref() {
            loader = loader.session_config_path(path);
        }
        if let Some(extra) = init.extra_config_toml.as_ref() {
            loader = loader.extra_toml_override(extra.clone());
        }

        let config = loader
            .build()
            .map_err(|err| InferenceAdapterError::Config(err.to_string()))?;

        let mut registry_builder = TemplateRegistry::builder();
        if let Some(path) = init.custom_template_dir.as_ref() {
            registry_builder = registry_builder.custom_template_dir(path);
        }
        let template_registry = registry_builder
            .build()
            .map_err(|err| InferenceAdapterError::Template(err.to_string()))?;

        let selected_template = select_template(&template_registry, &config, &init);
        if !template_exists(&template_registry, &selected_template) {
            return Err(InferenceAdapterError::Template(format!(
                "template '{selected_template}' is not available"
            )));
        }

        let gateway = InferenceGateway::new(&config.backend)
            .map_err(|err| InferenceAdapterError::Gateway(err.to_string()))?;

        Ok(Self {
            config,
            template_registry,
            selected_template,
            gateway,
        })
    }

    pub fn config(&self) -> &OzoneConfig {
        &self.config
    }

    pub fn selected_template(&self) -> &str {
        &self.selected_template
    }

    pub fn gateway(&self) -> &InferenceGateway {
        &self.gateway
    }

    pub fn render_prompt(&self, turns: &[TranscriptTurn]) -> Result<String, InferenceAdapterError> {
        let messages: Vec<_> = turns
            .iter()
            .map(TranscriptTurn::as_template_message)
            .collect();
        self.template_registry
            .render(&self.selected_template, &messages)
            .map_err(|err| InferenceAdapterError::Template(err.to_string()))
    }

    pub fn build_request(&self, prompt: impl Into<String>) -> InferenceRequest {
        InferenceRequest::new(prompt, self.config.context.max_tokens)
    }
}

fn select_template(
    template_registry: &TemplateRegistry,
    config: &OzoneConfig,
    init: &InferenceAdapterInit,
) -> String {
    if let Some(name) = init.template_override.as_deref() {
        if template_exists(template_registry, name) {
            return name.to_string();
        }
    }

    if template_exists(template_registry, &config.backend.prompt_template) {
        return config.backend.prompt_template.clone();
    }

    if let Some(model_hint) = init.model_hint.as_deref() {
        let detected = detect_template(model_hint);
        if template_exists(template_registry, detected) {
            return detected.to_string();
        }
    }

    "chatml".to_string()
}

fn template_exists(registry: &TemplateRegistry, template_name: &str) -> bool {
    registry.render(template_name, &[]).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_defaults_and_uses_config_template() {
        let adapter = InferenceAdapter::load(InferenceAdapterInit {
            global_config_path: Some(PathBuf::from("/nonexistent/ozone-plus-config.toml")),
            ..Default::default()
        })
        .expect("adapter should load defaults");

        assert_eq!(adapter.selected_template(), "chatml");
        assert_eq!(adapter.config().backend.r#type, "koboldcpp");
    }

    #[test]
    fn model_hint_detects_template_when_config_template_is_invalid() {
        let adapter = InferenceAdapter::load(InferenceAdapterInit {
            global_config_path: Some(PathBuf::from("/nonexistent/ozone-plus-config.toml")),
            extra_config_toml: Some(
                "[backend]\nprompt_template = \"unknown-template\"".to_string(),
            ),
            model_hint: Some("Meta-Llama-3.1-8B-Instruct".to_string()),
            ..Default::default()
        })
        .expect("adapter should fall back to model-detected template");

        assert_eq!(adapter.selected_template(), "llama3-instruct");
    }

    #[test]
    fn prompt_rendering_maps_transcript_turns() {
        let adapter = InferenceAdapter::load(InferenceAdapterInit {
            global_config_path: Some(PathBuf::from("/nonexistent/ozone-plus-config.toml")),
            template_override: Some("alpaca".to_string()),
            ..Default::default()
        })
        .expect("adapter should load");

        let prompt = adapter
            .render_prompt(&[
                TranscriptTurn::new(TranscriptRole::System, "You are concise."),
                TranscriptTurn::new(TranscriptRole::User, "Hello"),
                TranscriptTurn::new(TranscriptRole::Assistant, "Hi"),
            ])
            .expect("prompt should render");

        assert!(prompt.contains("### Instruction:"));
        assert!(prompt.contains("Hello"));
    }

    #[test]
    fn rejects_unknown_author_kind() {
        let err = TranscriptTurn::from_author_kind("developer", "content")
            .expect_err("unknown role should fail");
        assert!(
            err.to_string().contains("unsupported transcript role"),
            "unexpected error: {err}"
        );
    }
}
