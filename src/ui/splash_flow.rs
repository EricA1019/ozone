use super::App;

pub(super) fn handle_splash_key(app: &mut App) {
    if app.splash_ready {
        app.screen = super::next_screen_after_splash(app);
    }
}
