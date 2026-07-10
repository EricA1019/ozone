use ozone_core::{
    engine::{BranchId, ConversationCommand, MessageId},
    session::UnixTimestamp,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditMessageCommand {
    pub message_id: MessageId,
    pub new_content: String,
    pub edited_at: UnixTimestamp,
}

impl EditMessageCommand {
    pub fn new(
        message_id: MessageId,
        new_content: impl Into<String>,
        edited_at: UnixTimestamp,
    ) -> Self {
        Self {
            message_id,
            new_content: new_content.into(),
            edited_at,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivateBranchCommand {
    pub branch_id: BranchId,
}

impl ActivateBranchCommand {
    pub fn new(branch_id: BranchId) -> Self {
        Self { branch_id }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineCommand {
    Conversation(ConversationCommand),
    EditMessage(EditMessageCommand),
    ActivateBranch(ActivateBranchCommand),
}

impl EngineCommand {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Conversation(command) => command.kind(),
            Self::EditMessage(_) => "edit_message",
            Self::ActivateBranch(_) => "activate_branch",
        }
    }
}

impl From<ConversationCommand> for EngineCommand {
    fn from(command: ConversationCommand) -> Self {
        Self::Conversation(command)
    }
}
