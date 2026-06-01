use crate::catalog::{CatalogLoadIssue, CatalogLoadIssueLevel, CatalogLoadReport, CatalogRecord};

use super::{App, Screen};

pub(super) fn selected_catalog_name(app: &App) -> Option<String> {
    app.filtered_catalog_get(app.selected_model)
        .map(|record| record.model_name)
}

fn select_catalog_index(app: &App, preferred_name: Option<&str>) -> usize {
    preferred_name
        .and_then(|name| {
            app.filtered_catalog()
                .iter()
                .position(|record| record.model_name == name)
        })
        .unwrap_or(0)
}

pub(super) fn apply_catalog_refresh(app: &mut App, catalog: Vec<CatalogRecord>) {
    let preferred_name = selected_catalog_name(app)
        .or_else(|| app.current_plan.as_ref().map(|plan| plan.model_name.clone()))
        .or_else(|| (!app.prefs.last_model_name.is_empty()).then(|| app.prefs.last_model_name.clone()));

    app.catalog = catalog;
    app.selected_model = select_catalog_index(app, preferred_name.as_deref());

    let plan_missing = app
        .current_plan
        .as_ref()
        .is_some_and(|plan| !app.catalog.iter().any(|record| record.model_name == plan.model_name));
    if plan_missing {
        app.current_plan = None;
        app.configure_recommended_plan = None;
        if matches!(app.screen, Screen::ConfigureHub | Screen::Confirm) {
            app.screen = Screen::ModelPicker;
            app.set_error("Selected model is no longer available.".into());
        }
    }
}

fn summarize_catalog_issues(issues: &[CatalogLoadIssue]) -> Option<(CatalogLoadIssueLevel, String)> {
    if issues.is_empty() {
        return None;
    }

    let level = if issues
        .iter()
        .any(|issue| issue.level == CatalogLoadIssueLevel::Error)
    {
        CatalogLoadIssueLevel::Error
    } else {
        CatalogLoadIssueLevel::Warning
    };
    let message = issues
        .iter()
        .map(|issue| issue.message.as_str())
        .collect::<Vec<_>>()
        .join(" | ");
    Some((level, message))
}

pub(super) fn apply_catalog_report(app: &mut App, report: CatalogLoadReport) {
    let issue_summary = summarize_catalog_issues(&report.issues);
    apply_catalog_refresh(app, report.records);

    if app.error_msg.is_some() {
        return;
    }

    if let Some((level, message)) = issue_summary {
        match level {
            CatalogLoadIssueLevel::Warning => app.set_status(message),
            CatalogLoadIssueLevel::Error => app.set_error(message),
        }
    }
}
