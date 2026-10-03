#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// WHY: Single executable with 2 modes (GUI vs headless tick) avoids Gatekeeper re-evaluation
//      for a secondary worker binary in ad-hoc signed distribution without Apple Developer ID.
// WHAT BREAKS: Separating a distinct worker binary re-triggers macOS Gatekeeper rejection
//              ("codex-scheduler-cli is not open") and silently breaks scheduled background execution.
// EVIDENCE: docs/adr/0003-macos-single-executable-headless-scheduler.md, OS-SCHED-001, DELIVERY-BUNDLE-002
fn main() {
    let mode = codex_scheduler_gui_lib::parse_execution_mode(std::env::args());
    match mode {
        codex_scheduler_gui_lib::AppExecutionMode::HeadlessSchedulerTick => {
            codex_scheduler_gui_lib::run_headless_tick();
        }
        codex_scheduler_gui_lib::AppExecutionMode::Gui => {
            codex_scheduler_gui_lib::run();
        }
    }
}
