use std::fs;

const BENCH_RS_PATH: &str = "src/bench.rs";
const MAIN_RS_PATH: &str = "src/main.rs";
const PROFILING_RS_PATH: &str = "src/profiling.rs";

#[test]
fn native_benchmark_surface_is_managed_llamacpp_only() {
    let bench_rs = fs::read_to_string(BENCH_RS_PATH).expect("read bench.rs");
    let main_rs = fs::read_to_string(MAIN_RS_PATH).expect("read main.rs");
    let profiling_rs = fs::read_to_string(PROFILING_RS_PATH).expect("read profiling.rs");

    for legacy_marker in [
        "BenchBackend::KoboldCpp",
        "resolved_kobold_launcher_path",
        "run_kobold_generation",
        "koboldcpp_generate_url",
        "KoboldCpp",
        "8080",
    ] {
        assert!(
            !bench_rs.contains(legacy_marker),
            "bench.rs still contains {legacy_marker}"
        );
    }

    assert!(!main_rs.contains("BenchBackend::KoboldCpp"));
    assert!(!main_rs.contains("resolved_kobold_launcher_path"));
    assert!(!profiling_rs.contains("BenchBackend::KoboldCpp"));
    assert!(bench_rs.contains("8989"));
}
