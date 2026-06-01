use crate::planner::LaunchPlan;

#[cfg(test)]
pub(super) fn build_kc_args(plan: &LaunchPlan) -> Vec<String> {
    let mut args = vec![
        "--gpulayers".to_string(),
        plan.gpu_layers.to_string(),
        "--contextsize".to_string(),
        plan.context_size.to_string(),
        "--quantkv".to_string(),
        plan.quant_kv.to_string(),
    ];
    if let Some(t) = plan.threads {
        args.push("--threads".to_string());
        args.push(t.to_string());
    }
    if let Some(bt) = plan.blas_threads {
        args.push("--blasthreads".to_string());
        args.push(bt.to_string());
    }
    args
}

pub(super) fn build_llama_args(plan: &LaunchPlan) -> Vec<String> {
    const LLAMACPP_MANAGED_HOST: &str = "127.0.0.1";
    const LLAMACPP_MANAGED_PORT: &str = "8989";

    let gpu_layers = if plan.gpu_layers < 0 {
        "all".to_string()
    } else {
        plan.gpu_layers.to_string()
    };
    let mut args = vec![
        "--host".to_string(),
        LLAMACPP_MANAGED_HOST.to_string(),
        "--port".to_string(),
        LLAMACPP_MANAGED_PORT.to_string(),
        "--ctx-size".to_string(),
        plan.context_size.to_string(),
        "--gpu-layers".to_string(),
        gpu_layers,
        "--no-webui".to_string(),
    ];
    if let Some(t) = plan.threads {
        args.push("--threads".to_string());
        args.push(t.to_string());
    }
    args
}

#[cfg(test)]
mod tests {
    use super::{build_kc_args, build_llama_args};
    use crate::planner::{LaunchPlan, RecommendationMode};

    fn sample_plan() -> LaunchPlan {
        LaunchPlan {
            model_name: "sample.gguf".to_string(),
            context_size: 8192,
            gpu_layers: 42,
            total_layers: 56,
            cpu_layers: 14,
            quant_kv: 2,
            threads: Some(8),
            blas_threads: Some(4),
            mode: RecommendationMode::MixedMemory,
            rationale: "test".to_string(),
            estimated: false,
            estimated_vram_mb: 10240,
            estimated_ram_mb: 4096,
            source: "test".to_string(),
            layer_source_label: "test".to_string(),
            layer_source_note: None,
        }
    }

    #[test]
    fn build_kc_args_includes_thread_overrides() {
        let args = build_kc_args(&sample_plan());

        assert_eq!(
            args,
            vec![
                "--gpulayers",
                "42",
                "--contextsize",
                "8192",
                "--quantkv",
                "2",
                "--threads",
                "8",
                "--blasthreads",
                "4",
            ]
        );
    }

    #[test]
    fn build_llama_args_uses_managed_port_and_all_for_negative_gpu_layers() {
        let mut plan = sample_plan();
        plan.gpu_layers = -1;
        plan.threads = None;

        let args = build_llama_args(&plan);

        assert_eq!(
            args,
            vec![
                "--host",
                "127.0.0.1",
                "--port",
                "8989",
                "--ctx-size",
                "8192",
                "--gpu-layers",
                "all",
                "--no-webui",
            ]
        );
    }
}