//! Output abstraction for the worker pool.
//!
//! The pool itself knows nothing about Tauri or terminals: it records events
//! into [`RunState`](crate::state::run_state::RunState) and hands each one to
//! an `EventSink`. The GUI sink forwards them to the webview; the CLI sink
//! prints them.

use crate::pipeline::stages::PipelineEvent;
use crate::state::run_state::FileEvent;

pub trait EventSink: Send + Sync {
    /// Deliver an event. Called after run state has been updated for it.
    fn emit(&self, event: &PipelineEvent);

    /// Hook invoked for every completed file *before* it is recorded and
    /// emitted. Lets a platform adjust the outcome (e.g. Android write-back of
    /// the cached output to the original content URI).
    fn finalize_file_event(&self, _event: &mut FileEvent) {}

    /// Invoked once after all workers have exited, before `RunComplete`.
    fn run_finished(&self) {}
}

/// Sink that discards everything. Handy for tests.
pub struct NullSink;

impl EventSink for NullSink {
    fn emit(&self, _event: &PipelineEvent) {}
}
