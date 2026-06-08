use ozone_core::product::{ProductTier, OZONE_PLUS_DESIGN_DOC_PATH, OZONE_PLUS_DOC_PATH};
use ozone_core::paths::{benchmarks_db_path, data_dir, kobold_log_path, preferences_path};
use ozone_persist::PersistencePaths;

pub fn print_bootstrap_summary() {
    println!(
        "{} ({}) — {}",
        ProductTier::OzonePlus.display_name(),
        ProductTier::OzonePlus.slug(),
        ProductTier::OzonePlus.status_label()
    );
    println!("⬡ Local-LLM chat shell with persistent memory and sessions.");
    println!(
        "Create sessions, chat with streaming inference, pin memories, search across sessions,"
    );
    println!("branch transcripts, import characters, and export your data.");
    println!();
    println!("Try one of:");
    println!("  ozone-plus create \"First Session\"");
    println!("  ozone-plus send <session-id> \"Hello there\"");
    println!("  ozone-plus transcript <session-id>");
    println!("  ozone-plus branch list <session-id>");
    println!("  ozone-plus swipe list <session-id>");
    println!("  ozone-plus import card ./aster.json");
    println!("  ozone-plus export session <session-id> --output ./session.json");
}

pub fn print_identity() {
    println!("Current target");
    println!("  name:   {}", ProductTier::OzonePlus.display_name());
    println!("  slug:   {}", ProductTier::OzonePlus.slug());
    println!("  status: {}", ProductTier::OzonePlus.status_label());
    println!();
    println!("Ozone family");
    for (name, slug, status) in [
        (
            ProductTier::Ozonelite.display_name(),
            ProductTier::Ozonelite.slug(),
            ProductTier::Ozonelite.status_label(),
        ),
        (
            ProductTier::Ozone.display_name(),
            ProductTier::Ozone.slug(),
            ProductTier::Ozone.status_label(),
        ),
        (
            ProductTier::OzonePlus.display_name(),
            ProductTier::OzonePlus.slug(),
            ProductTier::OzonePlus.status_label(),
        ),
    ] {
        println!("  - {:<10} ({}) [{}]", name, slug, status);
    }
}

pub fn print_docs() {
    println!("ozone+ documentation entry points");
    println!("  family guide:    {OZONE_PLUS_DOC_PATH}");
    println!("  baseline design: {OZONE_PLUS_DESIGN_DOC_PATH}");
    println!();
    println!("These docs describe the current shipped ozone+ scope.");
    println!("Run `ozone-plus --help` for the full command reference.");
}

pub fn print_paths() {
    println!("Shared ozone+ filesystem paths");
    print_optional_path("data dir", data_dir());
    print_optional_path("preferences", preferences_path());
    print_optional_path("benchmarks db", benchmarks_db_path());
    print_optional_path("kobold log", kobold_log_path());
    println!();
    println!("Persistence layout");
    match PersistencePaths::from_xdg() {
        Ok(paths) => {
            print_resolved_path("global db", paths.global_db_path());
            print_resolved_path("sessions dir", paths.sessions_dir());
        }
        Err(error) => println!("  unavailable   {error}"),
    }
    println!();
    println!("Run `ozone-plus open <session-id>` to launch the chat TUI.");
}

fn print_optional_path(label: &str, path: Option<std::path::PathBuf>) {
    match path {
        Some(path) => println!("  {label:<13} {}", path.display()),
        None => println!("  {label:<13} unavailable on this machine"),
    }
}

fn print_resolved_path(label: &str, path: impl AsRef<std::path::Path>) {
    println!("  {label:<13} {}", path.as_ref().display());
}