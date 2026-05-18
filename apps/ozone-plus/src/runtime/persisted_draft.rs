use std::{fs, io::ErrorKind};

use ozone_persist::SessionId;
use ozone_tui::DraftState as TuiDraftState;

use super::Phase1dRuntime;

impl Phase1dRuntime {
    pub(super) fn load_persisted_draft(
        &self,
        session_id: &SessionId,
    ) -> Result<Option<TuiDraftState>, String> {
        let draft_path = self.repo.paths().session_draft_path(session_id);
        match fs::read_to_string(&draft_path) {
            Ok(text) if text.is_empty() => Ok(None),
            Ok(text) => Ok(Some(TuiDraftState::restore(
                ozone_tui::app::DraftCheckpoint::new(text.clone(), text.chars().count()),
            ))),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
            Err(error) => Err(format!(
                "failed to read persisted draft at {}: {error}",
                draft_path.display()
            )),
        }
    }

    pub(super) fn save_persisted_draft(
        &self,
        session_id: &SessionId,
        draft: Option<&str>,
    ) -> Result<(), String> {
        let draft_path = self.repo.paths().session_draft_path(session_id);
        let parent = draft_path.parent().ok_or_else(|| {
            format!(
                "draft path {} has no parent directory",
                draft_path.display()
            )
        })?;
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create draft directory {}: {error}",
                parent.display()
            )
        })?;

        match draft.filter(|text| !text.is_empty()) {
            Some(text) => fs::write(&draft_path, text.as_bytes()).map_err(|error| {
                format!(
                    "failed to write persisted draft {}: {error}",
                    draft_path.display()
                )
            })?,
            None => match fs::remove_file(&draft_path) {
                Ok(()) => {}
                Err(error) if error.kind() == ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(format!(
                        "failed to remove persisted draft {}: {error}",
                        draft_path.display()
                    ))
                }
            },
        }

        Ok(())
    }
}
