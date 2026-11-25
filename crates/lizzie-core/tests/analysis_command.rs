//! The analysis engine's launch command (Java `AnalysisEngineLaunchCommand`,
//! `KataGoAutoSetupHelper.experimentalBackendOverrides`): the batch size of backends that pad
//! every batch, and the backend settings a command made for the same KataGo carries. Expectations
//! follow the Java tests and KataGo's own rule, `max(2, (analysis threads × search threads + 1) / 2)`.

use std::path::{Path, PathBuf};

use lizzie_core::engine::analysis_command::{backend_overrides, size_padded_batches};
use lizzie_core::resources::{build_gtp_command, ExperimentalBackend};

/// KataGo's `analysis_example.cfg`, as bundled: 2 analysis threads × 16 search threads, 64 rows.
const SHIPPED: &str = "numAnalysisThreads = 2  # positions in parallel\n\
                       numSearchThreadsPerAnalysisThread = 16\n\
                       nnMaxBatchSize = 64\n";

struct Configs {
    dir: tempfile::TempDir,
    count: u32,
}

impl Configs {
    fn new() -> Configs {
        Configs { dir: tempfile::tempdir().unwrap(), count: 0 }
    }

    /// An analysis command for `program` reading a config of `text`, with `overrides`.
    fn command_of(&mut self, program: &str, text: &str, overrides: &str) -> String {
        self.count += 1;
        let path = self.dir.path().join(format!("analysis config {}.cfg", self.count));
        std::fs::write(&path, text).unwrap();
        let overrides = if overrides.is_empty() { String::new() } else { format!(" -override-config {overrides}") };
        format!("{program} analysis -model model.bin.gz -config \"{}\"{overrides} -quit-without-waiting", path.display())
    }

    fn command(&mut self, text: &str, overrides: &str) -> String {
        self.command_of("katago", text, overrides)
    }
}

fn sized(command: &str) -> String {
    size_padded_batches(command, &[])
}

#[test]
fn padding_backends_get_a_batch_their_threads_can_fill() {
    let mut c = Configs::new();
    // A quick request on DirectML: 2 analysis threads × 1 search thread.
    let quick = c.command(
        SHIPPED,
        "onnxProvider=directml,analysisPVLen=15,numSearchThreads=,numSearchThreadsPerAnalysisThread=1",
    );
    assert_eq!(
        sized(&quick),
        quick.replace("numSearchThreadsPerAnalysisThread=1", "numSearchThreadsPerAnalysisThread=1,nnMaxBatchSize=2")
    );
    // (padding settings, analysis config, the batch size) — threads from the overrides, else
    // from the config; `numSearchThreads` counts as the per-analysis-thread value.
    let table = [
        ("onnxProvider=directml,numAnalysisThreads=6,numSearchThreads=,numSearchThreadsPerAnalysisThread=2", SHIPPED, 6),
        ("onnxProvider=directml", SHIPPED, 16),
        ("onnxProvider=directml", "numAnalysisThreads = 4\nnumSearchThreads = 3\nnnMaxBatchSize = 64\n", 6),
        ("onnxProvider=directml,numAnalysisThreads=1", "numSearchThreadsPerAnalysisThread = 1\nnnMaxBatchSize = 64\n", 2),
        ("onnxProvider=directml,numAnalysisThreads=3", "numSearchThreadsPerAnalysisThread = 3\nnnMaxBatchSize = 64\n", 5),
        ("onnxProvider=openvino,onnxOpenVINODeviceType=NPU", SHIPPED, 16),
        ("onnxProvider=openvino,onnxOpenVINODeviceTypeThread1=BATCH:NPU", SHIPPED, 16),
        ("onnxProvider=cpu,onnxPadBatch=true", SHIPPED, 16),
        ("metalPadBatch=true", SHIPPED, 16),
    ];
    for (overrides, config, batch) in table {
        let command = c.command(config, overrides);
        assert_eq!(
            sized(&command),
            command.replace(overrides, &format!("{overrides},nnMaxBatchSize={batch}")),
            "{overrides} / {config:?}"
        );
    }
}

#[test]
fn the_managed_directml_and_openvino_npu_engines_get_the_batch_size() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("analysis.cfg");
    std::fs::write(&config, SHIPPED).unwrap();
    for backend in [ExperimentalBackend::DirectMl, ExperimentalBackend::OpenVinoNpu] {
        let gtp = build_gtp_command(Path::new("/engines/katago"), Path::new("/w.bin.gz"), &config, Some(backend), Path::new("/data"))
            .unwrap();
        let analysis = gtp.replacen(" gtp ", " analysis ", 1);
        assert!(sized(&analysis).contains(",nnMaxBatchSize=16"), "{backend:?}: {}", sized(&analysis));
    }
    let gpu = build_gtp_command(
        Path::new("/engines/katago"),
        Path::new("/w.bin.gz"),
        &config,
        Some(ExperimentalBackend::OpenVinoGpu),
        Path::new("/data"),
    )
    .unwrap()
    .replacen(" gtp ", " analysis ", 1);
    assert_eq!(sized(&gpu), gpu);
}

#[test]
fn dynamic_backends_and_chosen_sizes_keep_the_command() {
    let mut c = Configs::new();
    let unchanged = [
        // CUDA, TensorRT, OpenCL, Eigen and the bundled Metal engine batch dynamically: their
        // commands carry no padding settings.
        c.command_of("/engines/cuda/katago", SHIPPED, "analysisPVLen=15,numSearchThreads=,numSearchThreadsPerAnalysisThread=1"),
        c.command_of("/engines/tensorrt/katago", SHIPPED, "analysisPVLen=15"),
        c.command_of("/engines/opencl/katago", SHIPPED, ""),
        c.command_of("/engines/eigen/katago", SHIPPED, "numAnalysisThreads=8"),
        c.command_of("/engines/metal/katago", SHIPPED, "analysisPVLen=15"),
        c.command(SHIPPED, "onnxProvider=cuda"),
        c.command(SHIPPED, "onnxProvider=openvino,onnxOpenVINODeviceType=GPU"),
        c.command(SHIPPED, "onnxProvider=directml,onnxPadBatch=false"),
        // A size the user wrote in the command is theirs.
        c.command(SHIPPED, "onnxProvider=directml,nnMaxBatchSize=8"),
        // A configured size already small enough.
        c.command("numAnalysisThreads = 2\nnumSearchThreads = 16\nnnMaxBatchSize = 4\n", "onnxProvider=directml"),
        // Threads that cannot be read.
        c.command("nnMaxBatchSize = 64\n", "onnxProvider=directml"),
        "katago analysis -config missing.cfg -override-config onnxProvider=directml".into(),
        "katago analysis -override-config".into(),
        String::new(),
    ];
    for command in unchanged {
        assert_eq!(sized(&command), command);
    }
}

#[test]
fn a_relative_config_is_read_where_the_launch_finds_it() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("analysis.cfg"), SHIPPED).unwrap();
    let command = "katago analysis -config analysis.cfg -override-config onnxProvider=directml";
    let roots: Vec<PathBuf> = vec![root.path().to_path_buf()];
    assert_eq!(
        size_padded_batches(command, &roots),
        "katago analysis -config analysis.cfg -override-config onnxProvider=directml,nnMaxBatchSize=16"
    );
}

#[test]
fn backend_settings_follow_the_engine_command() {
    let data = Path::new("/data");
    let command = |backend| {
        build_gtp_command(Path::new("/engines/katago"), Path::new("/w.bin.gz"), Path::new("/c.cfg"), backend, data).unwrap()
    };
    let cache = data.join("runtime/katago-openvino-cache");
    assert_eq!(backend_overrides(&command(Some(ExperimentalBackend::DirectMl))).as_deref(), Some("onnxProvider=directml"));
    assert_eq!(
        backend_overrides(&command(Some(ExperimentalBackend::OpenVinoGpu))),
        Some(format!("onnxProvider=openvino,onnxOpenVINOCacheDir={}", cache.display()))
    );
    assert_eq!(
        backend_overrides(&command(Some(ExperimentalBackend::OpenVinoNpu))),
        Some(format!("onnxProvider=openvino,onnxOpenVINOCacheDir={},onnxOpenVINODeviceType=NPU", cache.display()))
    );
    assert_eq!(backend_overrides(&command(None)), None);
    assert_eq!(backend_overrides(&command(Some(ExperimentalBackend::RocmGfx110x))), None);
    // Other settings of the engine are its own.
    assert_eq!(
        backend_overrides("katago gtp -override-config numSearchThreads=4,onnxProvider=directml,maxVisits=9"),
        Some("onnxProvider=directml".into())
    );
}
