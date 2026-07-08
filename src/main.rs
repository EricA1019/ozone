//! Ozone binary entry point.
//!
//! This is intentionally minimal — all logic lives in `lib.rs` so that
//! integration tests can import internal modules.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    ozone::run()?;
    Ok(())
}
