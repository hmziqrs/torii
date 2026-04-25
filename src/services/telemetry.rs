use metrics::{counter, histogram};
use std::sync::atomic::{AtomicU64, Ordering};

static REQUESTS_COMPLETED_TOTAL: AtomicU64 = AtomicU64::new(0);
static REQUESTS_CANCELLED_TOTAL: AtomicU64 = AtomicU64::new(0);
static REQUESTS_FAILED_TOTAL: AtomicU64 = AtomicU64::new(0);
static RESPONSES_TRUNCATED_TOTAL: AtomicU64 = AtomicU64::new(0);
static ASYNC_UPDATE_FAILURES_TOTAL: AtomicU64 = AtomicU64::new(0);
static TREE_ILLEGAL_DROPS_TOTAL: AtomicU64 = AtomicU64::new(0);
static VARIABLE_RESOLUTION_MISSING_TOTAL: AtomicU64 = AtomicU64::new(0);
static TREE_CATALOG_RELOAD_TOTAL: AtomicU64 = AtomicU64::new(0);

pub fn inc_requests_completed() {
    let value = REQUESTS_COMPLETED_TOTAL.fetch_add(1, Ordering::Relaxed) + 1;
    counter!("requests_completed_total").increment(1);
    tracing::info!(requests_completed_total = value, "counter updated");
}

pub fn inc_requests_cancelled() {
    let value = REQUESTS_CANCELLED_TOTAL.fetch_add(1, Ordering::Relaxed) + 1;
    counter!("requests_cancelled_total").increment(1);
    tracing::info!(requests_cancelled_total = value, "counter updated");
}

pub fn inc_requests_failed() {
    let value = REQUESTS_FAILED_TOTAL.fetch_add(1, Ordering::Relaxed) + 1;
    counter!("requests_failed_total").increment(1);
    tracing::info!(requests_failed_total = value, "counter updated");
}

pub fn inc_responses_truncated() {
    let value = RESPONSES_TRUNCATED_TOTAL.fetch_add(1, Ordering::Relaxed) + 1;
    counter!("responses_truncated_total").increment(1);
    tracing::info!(responses_truncated_total = value, "counter updated");
}

pub fn observe_preview_bytes(preview_len: usize) {
    histogram!("preview_bytes_histogram").record(preview_len as f64);
    tracing::info!(
        preview_bytes_histogram = preview_len,
        "preview bytes observation"
    );
}

pub fn inc_async_update_failures(category: &'static str) {
    let value = ASYNC_UPDATE_FAILURES_TOTAL.fetch_add(1, Ordering::Relaxed) + 1;
    counter!("async_update_failures_total", "category" => category).increment(1);
    tracing::warn!(
        async_update_failures_total = value,
        category,
        "async update failure observed"
    );
}

pub fn inc_tree_illegal_drops() {
    let value = TREE_ILLEGAL_DROPS_TOTAL.fetch_add(1, Ordering::Relaxed) + 1;
    counter!("tree_illegal_drops_total").increment(1);
    tracing::warn!(tree_illegal_drops_total = value, "counter updated");
}

pub fn inc_variable_resolution_missing_failures() {
    let value = VARIABLE_RESOLUTION_MISSING_TOTAL.fetch_add(1, Ordering::Relaxed) + 1;
    counter!("variable_resolution_missing_failures_total").increment(1);
    tracing::warn!(
        variable_resolution_missing_failures_total = value,
        "counter updated"
    );
}

pub fn inc_tree_catalog_reload() {
    let value = TREE_CATALOG_RELOAD_TOTAL.fetch_add(1, Ordering::Relaxed) + 1;
    counter!("tree_catalog_reload_total").increment(1);
    tracing::info!(tree_catalog_reload_total = value, "counter updated");
}
