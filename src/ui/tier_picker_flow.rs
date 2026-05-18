use crossterm::event::{KeyCode, KeyEvent};

use super::{tier_install, tier_picker, App, Screen};

pub(super) enum TierPickerOutcome {
    Continue,
    ExitLauncher,
}

pub(super) fn handle_tier_picker_key(app: &mut App, key: KeyEvent) -> TierPickerOutcome {
    let phase = app.tier_picker.phase.clone();
    match phase {
        tier_picker::TierPickerPhase::Picking => match key.code {
            KeyCode::Char('q') | KeyCode::Esc => return TierPickerOutcome::ExitLauncher,
            KeyCode::Up => app.tier_picker.up(),
            KeyCode::Down => app.tier_picker.down(),
            KeyCode::Enter => {
                let tier = app.tier_picker.selected_tier();
                let binary = tier_install::binary_name_for_tier(tier).to_string();
                match tier {
                    crate::prefs::Tier::Lite => {
                        app.prefs.preferred_tier = Some(tier);
                        let prefs_clone = app.prefs.clone();
                        tokio::spawn(async move {
                            let _ = crate::prefs::save_prefs(&prefs_clone).await;
                        });
                        app.screen = Screen::Launcher;
                    }
                    crate::prefs::Tier::Base | crate::prefs::Tier::Plus => {
                        if tier_install::is_tier_installed(&binary) {
                            app.prefs.preferred_tier = Some(tier);
                            let prefs_clone = app.prefs.clone();
                            tokio::spawn(async move {
                                let _ = crate::prefs::save_prefs(&prefs_clone).await;
                            });
                            app.screen = Screen::Launcher;
                        } else {
                            app.tier_picker.phase = tier_picker::TierPickerPhase::ConfirmingDownload {
                                tier,
                                binary,
                            };
                        }
                    }
                }
            }
            _ => {}
        },
        tier_picker::TierPickerPhase::ConfirmingDownload { tier, binary } => match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                let bin = binary.clone();
                let (tx, rx) = std::sync::mpsc::channel();
                std::thread::spawn(move || {
                    let result = tier_install::install_tier_from_github(&bin);
                    let _ = tx.send(result);
                });
                app.tier_picker.install_rx = Some(rx);
                app.tier_picker.phase = tier_picker::TierPickerPhase::Installing { tier, binary };
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Char('q') | KeyCode::Esc => {
                app.prefs.preferred_tier = Some(crate::prefs::Tier::Lite);
                let prefs_clone = app.prefs.clone();
                tokio::spawn(async move {
                    let _ = crate::prefs::save_prefs(&prefs_clone).await;
                });
                app.tier_picker.phase = tier_picker::TierPickerPhase::Picking;
                app.screen = Screen::Launcher;
            }
            _ => {}
        },
        tier_picker::TierPickerPhase::Installing { .. } => {
            // No input during install.
        }
        tier_picker::TierPickerPhase::InstallDone { tier, .. } => match key.code {
            KeyCode::Enter | KeyCode::Char(' ') => {
                app.prefs.preferred_tier = Some(tier);
                let prefs_clone = app.prefs.clone();
                tokio::spawn(async move {
                    let _ = crate::prefs::save_prefs(&prefs_clone).await;
                });
                app.tier_picker.phase = tier_picker::TierPickerPhase::Picking;
                app.screen = Screen::Launcher;
            }
            _ => {}
        },
        tier_picker::TierPickerPhase::InstallError { .. } => match key.code {
            KeyCode::Enter | KeyCode::Esc | KeyCode::Char(' ') => {
                app.prefs.preferred_tier = Some(crate::prefs::Tier::Lite);
                let prefs_clone = app.prefs.clone();
                tokio::spawn(async move {
                    let _ = crate::prefs::save_prefs(&prefs_clone).await;
                });
                app.tier_picker.phase = tier_picker::TierPickerPhase::Picking;
                app.screen = Screen::Launcher;
            }
            _ => {}
        },
    }

    TierPickerOutcome::Continue
}
