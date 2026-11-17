//! 诊断与日志 through the workbench commands the dialog sends (`SetLogging`, `SetFullTrace`).
use std::sync::{Arc, Mutex};

use lizzie_core::diagnostics::{DiagnosticModule, EndpointCategory, LogLevel, LoggingSettings, TraceScope};
use lizzie_core::engine::gtp::LaunchSpec;
use lizzie_core::engine::runtime::{EngineIo, InstanceId, Launcher};
use lizzie_core::settings::Storage;
use lizzie_core::workbench::Workbench;

struct NoEngine;
impl Launcher for NoEngine {
    fn launch(&self, _: InstanceId, _: &LaunchSpec) -> std::io::Result<Box<dyn EngineIo>> {
        Err(std::io::Error::other("not started"))
    }
}

fn storage(root: &std::path::Path) -> Storage {
    Storage::in_dirs(&root.join("config"), &root.join("data"))
}

fn workbench(root: &std::path::Path) -> Workbench {
    Workbench::new(storage(root), Some("zh-CN"), Box::new(NoEngine), Vec::new())
}

fn trace_files(root: &std::path::Path) -> Vec<&'static str> {
    let logs = storage(root).logs_dir;
    TraceScope::ALL.into_iter().map(TraceScope::file_name).filter(|f| logs.join(f).exists()).collect()
}

#[test]
fn switches_take_effect_at_once_and_survive_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    assert_eq!(wb.view(false).settings.logging, LoggingSettings::default());
    let lines = Arc::new(Mutex::new(Vec::new()));
    let out = lines.clone();
    wb.diagnostics().set_sink(Arc::new(move |_, text: &str| out.lock().unwrap().push(text.to_string())));
    let only_gtp = LoggingSettings { diagnostic_modules: vec![DiagnosticModule::GtpSummary], ..LoggingSettings::default() };
    assert!(wb.set_logging(only_gtp.clone()).unwrap());
    wb.diagnostics().debug(DiagnosticModule::ReadBoardYike, || "readboard".into());
    wb.diagnostics().debug(DiagnosticModule::GtpSummary, || "gtp".into());
    assert!(wb.set_logging(LoggingSettings { diagnostics_enabled: false, ..only_gtp.clone() }).unwrap());
    wb.diagnostics().debug(DiagnosticModule::GtpSummary, || "gtp off".into());
    assert_eq!(*lines.lock().unwrap(), ["gtp"]);
    drop(wb);
    let mut reopened = workbench(dir.path());
    assert_eq!(reopened.view(false).settings.logging, LoggingSettings { diagnostics_enabled: false, ..only_gtp });
    assert!(!reopened.diagnostics().records(DiagnosticModule::GtpSummary));
}

#[test]
fn choosing_scopes_records_nothing_until_a_session_starts_and_a_restart_does_not_resume_it() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    let scopes = vec![TraceScope::ReadBoardYike, TraceScope::NetworkWebSocket];
    wb.set_logging(LoggingSettings { preferred_full_trace_scopes: scopes.clone(), ..LoggingSettings::default() }).unwrap();
    wb.diagnostics().trace_payload(EndpointCategory::Protocol, "send", "not recorded");
    assert!(trace_files(dir.path()).is_empty());
    assert!(wb.view(false).full_trace.is_empty());

    assert!(wb.set_full_trace(true, Some(scopes.clone())).unwrap());
    assert_eq!(wb.view(false).full_trace, scopes);
    assert_eq!(trace_files(dir.path()), ["readboard-trace.log", "network-trace.log"]);
    // The running session keeps its scopes; only the preference changes.
    assert!(!wb.set_full_trace(true, Some(vec![TraceScope::EngineGtp])).unwrap());
    wb.set_logging(LoggingSettings { preferred_full_trace_scopes: vec![TraceScope::EngineGtp], ..LoggingSettings::default() }).unwrap();
    assert_eq!(wb.view(false).full_trace, scopes);
    wb.diagnostics().trace_payload(EndpointCategory::Protocol, "send", "kata-analyze password=hunter2 secret-marker");
    assert!(wb.set_full_trace(false, None).unwrap());
    let network = std::fs::read_to_string(storage(dir.path()).logs_dir.join("network-trace.log")).unwrap();
    assert!(network.contains("network raw direction=send"), "{network}");
    assert!(!network.contains("hunter2"), "{network}");
    wb.diagnostics().trace_payload(EndpointCategory::Protocol, "send", "after the stop");
    assert!(!std::fs::read_to_string(storage(dir.path()).logs_dir.join("network-trace.log")).unwrap().contains("after the stop"));

    drop(wb);
    let mut reopened = workbench(dir.path());
    let view = reopened.view(false);
    assert!(view.full_trace.is_empty());
    assert_eq!(view.settings.logging.preferred_full_trace_scopes, [TraceScope::EngineGtp]);
}

#[test]
fn starting_without_a_choice_uses_the_saved_preference_and_none_chosen_means_all() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    wb.set_logging(LoggingSettings { preferred_full_trace_scopes: vec![TraceScope::EngineGtp], ..LoggingSettings::default() }).unwrap();
    wb.set_full_trace(true, None).unwrap();
    assert_eq!(wb.view(false).full_trace, [TraceScope::EngineGtp]);
    wb.set_full_trace(false, None).unwrap();
    wb.set_full_trace(true, Some(Vec::new())).unwrap();
    assert_eq!(wb.view(false).full_trace, TraceScope::ALL);
    assert_eq!(wb.view(false).settings.logging.preferred_full_trace_scopes, TraceScope::ALL);
}

#[test]
fn network_failures_are_written_even_with_diagnostics_off() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    wb.set_logging(LoggingSettings { diagnostics_enabled: false, ..LoggingSettings::default() }).unwrap();
    let lines = Arc::new(Mutex::new(Vec::new()));
    let out = lines.clone();
    wb.diagnostics().set_sink(Arc::new(move |level, text: &str| out.lock().unwrap().push((level, text.to_string()))));
    let mut exchange = lizzie_core::diagnostics::NetworkExchange {
        remote: true, method: "POST", host: "https://api.example.org/login?password=hunter2",
        category: EndpointCategory::Authentication, status: Some(200), latency_ms: 5, outcome: "ok",
    };
    wb.diagnostics().network(&exchange);
    exchange.outcome = "failed";
    wb.diagnostics().network(&exchange);
    let lines = lines.lock().unwrap();
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].0, LogLevel::Warn);
    assert_eq!(lines[0].1, "remote event=http method=POST host=api.example.org category=authentication status=200 latencyMs=5 outcome=failed");
}

#[test]
fn a_full_log_rolls_over_at_10_mib_and_keeps_7_days_and_100_mib_of_archives() {
    // Java `LoggingRuntime.rollingAppender` with `LoggingLimits`: archive/<stem>.<day>.<i>.log.gz.
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    let logs = storage(dir.path()).logs_dir;
    let archive = logs.join("archive");
    std::fs::create_dir_all(&archive).unwrap();
    let day = |days_ago: u64| (chrono::Local::now().date_naive() - chrono::Days::new(days_ago)).format("%Y-%m-%d").to_string();
    let sized = |path: std::path::PathBuf, len: u64| std::fs::File::create(path).unwrap().set_len(len).unwrap();
    sized(archive.join(format!("network-trace.{}.0.log.gz", day(30))), 10);
    sized(archive.join(format!("network-trace.{}.0.log.gz", day(3))), 60 << 20);
    sized(archive.join(format!("network-trace.{}.0.log.gz", day(2))), 50 << 20);
    sized(archive.join(format!("readboard-trace.{}.0.log.gz", day(30))), 10);
    // An earlier session left the active file at the 10 MiB threshold.
    sized(logs.join("network-trace.log"), 10 << 20);

    wb.set_full_trace(true, Some(vec![TraceScope::NetworkWebSocket])).unwrap();
    wb.diagnostics().trace_payload(EndpointCategory::Protocol, "send", "after the rollover");
    wb.set_full_trace(false, None).unwrap();

    let active = std::fs::read_to_string(logs.join("network-trace.log")).unwrap();
    assert!(active.contains("full trace session started") && active.contains("after the rollover"), "{active}");
    assert!(active.len() < 1024);
    let mut names: Vec<String> = std::fs::read_dir(&archive).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
    names.sort();
    let today = format!("network-trace.{}.0.log.gz", day(0));
    // Older than 7 days, then the oldest beyond 100 MiB, go; another log's archives stay.
    assert_eq!(names, [format!("network-trace.{}.0.log.gz", day(2)), today.clone(), format!("readboard-trace.{}.0.log.gz", day(30))]);
    let mut rolled = Vec::new();
    std::io::Read::read_to_end(&mut flate2::read::GzDecoder::new(std::fs::File::open(archive.join(today)).unwrap()), &mut rolled).unwrap();
    assert_eq!(rolled.len(), 10 << 20);
}
