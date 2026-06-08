use std::fs;

const MAIN_RS_PATH: &str = "src/main.rs";
const ANALYZE_RS_PATH: &str = "src/analyze.rs";
const PROFILING_RS_PATH: &str = "src/profiling.rs";

#[test]
fn active_export_surface_drops_stale_kobold_labels() {
    let main_rs = fs::read_to_string(MAIN_RS_PATH).expect("read main.rs");
    let analyze_rs = fs::read_to_string(ANALYZE_RS_PATH).expect("read analyze.rs");
    let profiling_rs = fs::read_to_string(PROFILING_RS_PATH).expect("read profiling.rs");

    for stale_marker in [
        "Export profiles to koboldcpp-presets.conf",
        "Write the best profile into koboldcpp-presets.conf.",
        "Exporting KoboldCpp presets",
        "export it to koboldcpp-presets.conf",
        "KoboldCpp install is broken",
        "KoboldCpp never became ready",
    ] {
        assert!(
            !main_rs.contains(stale_marker),
            "main.rs still contains {stale_marker}"
        );
        assert!(
            !analyze_rs.contains(stale_marker),
            "analyze.rs still contains {stale_marker}"
        );
        assert!(
            !profiling_rs.contains(stale_marker),
            "profiling.rs still contains {stale_marker}"
        );
    }
}