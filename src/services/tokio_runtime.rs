use std::sync::Arc;

use gpui::Global;
use tokio::runtime::Runtime;

/// App-level Tokio runtime for network I/O and CPU-heavy async work.
/// GPUI's own async facilities should not be used for blocking or long-running operations.
///
/// Usage: `cx.global::<TokioRuntimeGlobal>().0.spawn(async { ... })`
#[derive(Clone)]
pub struct TokioRuntime {
    runtime: Arc<Runtime>,
}

impl TokioRuntime {
    pub fn new() -> anyhow::Result<Self> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .thread_name("torii-io")
            .build()
            .map_err(|e| anyhow::anyhow!("failed to build I/O tokio runtime: {e}"))?;
        Ok(Self {
            runtime: Arc::new(runtime),
        })
    }

    /// Spawn a future on the I/O runtime. Returns a JoinHandle.
    pub fn spawn<F>(&self, future: F) -> tokio::task::JoinHandle<F::Output>
    where
        F: std::future::Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.runtime.spawn(future)
    }

    /// Get a handle to the runtime for spawning from async contexts.
    pub fn handle(&self) -> &tokio::runtime::Handle {
        self.runtime.handle()
    }
}

#[derive(Clone)]
pub struct TokioRuntimeGlobal(pub Arc<TokioRuntime>);

impl Global for TokioRuntimeGlobal {}
