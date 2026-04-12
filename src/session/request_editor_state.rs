use std::sync::Arc;
use tokio_util::sync::CancellationToken;

use crate::domain::{
    ids::{HistoryEntryId, RequestDraftId, RequestId},
    request::RequestItem,
    response::ResponseSummary,
};
use crate::services::error_classifier::ClassifiedError;

/// OperationId is a type alias for HistoryEntryId.
pub type OperationId = HistoryEntryId;

// ---------------------------------------------------------------------------
// Save status (axis A — about persistence)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum SaveStatus {
    /// Draft equals persisted baseline.
    Pristine,
    /// Draft diverges from baseline.
    Dirty,
    /// Save operation in flight.
    Saving,
    /// Last save attempt failed; draft is still in memory.
    SaveFailed { error: String },
}

// ---------------------------------------------------------------------------
// Execution status (axis B — about the network operation)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum ExecStatus {
    /// No operation has run yet, or previous result was cleared.
    Idle,
    /// Request handed to execution service, awaiting response.
    Sending,
    /// Response headers received, body streaming.
    Streaming,
    /// Terminal: response fully received.
    Completed { response: Arc<ResponseSummary> },
    /// Terminal: request failed.
    Failed {
        summary: String,
        classified: Option<ClassifiedError>,
    },
    /// Terminal: request was cancelled.
    Cancelled { partial_size: Option<u64> },
}

impl ExecStatus {
    /// Returns true if this is a terminal state (Completed, Failed, or Cancelled).
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Completed { .. } | Self::Failed { .. } | Self::Cancelled { .. }
        )
    }

    /// Returns true if an operation is actively in flight (Sending or Streaming).
    pub fn is_in_flight(&self) -> bool {
        matches!(self, Self::Sending | Self::Streaming)
    }
}

// ---------------------------------------------------------------------------
// Preflight error — distinct from Failed exec terminal state
// ---------------------------------------------------------------------------

/// Errors that occur *before* the request is sent (URL parse, secret resolution, etc.).
/// These do NOT transition ExecStatus to Failed because nothing was actually sent.
#[derive(Debug, Clone)]
pub struct PreflightError {
    pub message: String,
}

// ---------------------------------------------------------------------------
// Editor identity — draft tab vs persisted tab
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum EditorIdentity {
    /// An unsaved new request — transitions to Persisted on first save.
    Draft(RequestDraftId),
    /// A persisted request — always has a RequestId.
    Persisted(RequestId),
}

impl EditorIdentity {
    pub fn as_request_id(&self) -> Option<RequestId> {
        match self {
            Self::Persisted(id) => Some(*id),
            Self::Draft(_) => None,
        }
    }
}

// ---------------------------------------------------------------------------
// RequestEditorState — hot per-tab entity
// ---------------------------------------------------------------------------

/// Hot per-tab request editor state holding draft values, persistence status,
/// execution lifecycle, and the latest response snapshot.
pub struct RequestEditorState {
    /// Tab identity: draft or persisted.
    identity: EditorIdentity,
    /// Current draft request value (the editor's working copy).
    draft: RequestItem,
    /// Persisted baseline for dirty diff checks.
    baseline: Option<RequestItem>,
    /// Save status axis.
    save_status: SaveStatus,
    /// Execution status axis.
    exec_status: ExecStatus,
    /// Active operation ID (aliases HistoryEntryId).
    active_operation_id: Option<HistoryEntryId>,
    /// Cancellation token for the active operation.
    cancellation_token: Option<CancellationToken>,
    /// Latest history entry ID for reopen/refresh.
    latest_history_id: Option<HistoryEntryId>,
    /// Preflight error, if any — distinct from exec Failed.
    preflight_error: Option<PreflightError>,
}

impl RequestEditorState {
    // -----------------------------------------------------------------------
    // Construction
    // -----------------------------------------------------------------------

    /// Create a new draft editor state for an unsaved request.
    pub fn new_draft(collection_id: crate::domain::ids::CollectionId) -> Self {
        let draft = RequestItem::new(collection_id, None, "Untitled Request", "GET", "", 0);
        Self {
            identity: EditorIdentity::Draft(RequestDraftId::new()),
            draft,
            baseline: None,
            save_status: SaveStatus::Dirty,
            exec_status: ExecStatus::Idle,
            active_operation_id: None,
            cancellation_token: None,
            latest_history_id: None,
            preflight_error: None,
        }
    }

    /// Create an editor state for an existing persisted request.
    pub fn from_persisted(request: RequestItem) -> Self {
        let baseline = request.clone();
        Self {
            identity: EditorIdentity::Persisted(request.id),
            draft: request,
            baseline: Some(baseline),
            save_status: SaveStatus::Pristine,
            exec_status: ExecStatus::Idle,
            active_operation_id: None,
            cancellation_token: None,
            latest_history_id: None,
            preflight_error: None,
        }
    }

    // -----------------------------------------------------------------------
    // Identity
    // -----------------------------------------------------------------------

    pub fn identity(&self) -> &EditorIdentity {
        &self.identity
    }

    pub fn request_id(&self) -> Option<RequestId> {
        self.identity.as_request_id()
    }

    /// Transition from draft identity to persisted identity after first save.
    pub fn transition_to_persisted(&mut self, id: RequestId, saved: &RequestItem) {
        self.identity = EditorIdentity::Persisted(id);
        self.draft.id = id;
        self.draft.collection_id = saved.collection_id;
        self.draft.parent_folder_id = saved.parent_folder_id;
        self.draft.sort_order = saved.sort_order;
        self.baseline = Some(self.draft.clone());
        self.save_status = SaveStatus::Pristine;
    }

    // -----------------------------------------------------------------------
    // Draft access
    // -----------------------------------------------------------------------

    pub fn draft(&self) -> &RequestItem {
        &self.draft
    }

    pub fn draft_mut(&mut self) -> &mut RequestItem {
        self.preflight_error = None;
        &mut self.draft
    }

    // -----------------------------------------------------------------------
    // Baseline and dirty detection
    // -----------------------------------------------------------------------

    pub fn baseline(&self) -> Option<&RequestItem> {
        self.baseline.as_ref()
    }

    /// Detect whether the draft diverges from the baseline.
    pub fn detect_dirty(&self) -> bool {
        match &self.baseline {
            None => true, // No baseline means draft-only, always dirty.
            Some(baseline) => {
                let draft = &self.draft;
                draft.name != baseline.name
                    || draft.method != baseline.method
                    || draft.url != baseline.url
                    || draft.params != baseline.params
                    || draft.headers != baseline.headers
                    || draft.auth != baseline.auth
                    || draft.body != baseline.body
                    || draft.scripts != baseline.scripts
                    || draft.settings != baseline.settings
            }
        }
    }

    /// Refresh save status based on current dirty detection.
    pub fn refresh_save_status(&mut self) {
        if self.detect_dirty() {
            if self.save_status == SaveStatus::Pristine {
                self.save_status = SaveStatus::Dirty;
            }
        } else {
            self.save_status = SaveStatus::Pristine;
        }
    }

    // -----------------------------------------------------------------------
    // Save status transitions
    // -----------------------------------------------------------------------

    pub fn save_status(&self) -> &SaveStatus {
        &self.save_status
    }

    pub fn begin_save(&mut self) {
        self.save_status = SaveStatus::Saving;
    }

    pub fn complete_save(&mut self, saved: &RequestItem) {
        self.draft.meta = saved.meta.clone();
        self.baseline = Some(self.draft.clone());
        self.save_status = SaveStatus::Pristine;
    }

    pub fn fail_save(&mut self, error: String) {
        self.save_status = SaveStatus::SaveFailed { error };
    }

    /// Reload baseline from persisted data without discarding the draft.
    pub fn reload_baseline(&mut self, persisted: RequestItem) {
        self.baseline = Some(persisted);
        self.refresh_save_status();
    }

    // -----------------------------------------------------------------------
    // Execution status transitions
    // -----------------------------------------------------------------------

    pub fn exec_status(&self) -> &ExecStatus {
        &self.exec_status
    }

    /// Begin a new send operation. Auto-cancels any in-flight operation.
    /// Returns the previous operation's cancellation token (already cancelled).
    pub fn begin_send(&mut self, operation_id: HistoryEntryId) -> Option<CancellationToken> {
        let old_token = self.cancellation_token.take();
        if let Some(ref token) = old_token {
            token.cancel();
        }
        self.active_operation_id = Some(operation_id);
        self.cancellation_token = Some(CancellationToken::new());
        self.exec_status = ExecStatus::Sending;
        self.preflight_error = None;
        old_token
    }

    pub fn transition_to_streaming(&mut self) {
        if matches!(self.exec_status, ExecStatus::Sending) {
            self.exec_status = ExecStatus::Streaming;
        }
    }

    pub fn complete_exec(
        &mut self,
        response: ResponseSummary,
        operation_id: HistoryEntryId,
    ) -> bool {
        if self.active_operation_id != Some(operation_id) {
            return false; // Late response — ignore.
        }
        self.exec_status = ExecStatus::Completed {
            response: Arc::new(response),
        };
        self.active_operation_id = None;
        self.cancellation_token = None;
        true
    }

    pub fn fail_exec(
        &mut self,
        summary: String,
        classified: Option<ClassifiedError>,
        operation_id: HistoryEntryId,
    ) -> bool {
        if self.active_operation_id != Some(operation_id) {
            return false; // Late failure — ignore.
        }
        self.exec_status = ExecStatus::Failed {
            summary,
            classified,
        };
        self.active_operation_id = None;
        self.cancellation_token = None;
        true
    }

    pub fn cancel_exec(&mut self, partial_size: Option<u64>, operation_id: HistoryEntryId) -> bool {
        if self.active_operation_id != Some(operation_id) {
            return false;
        }
        self.exec_status = ExecStatus::Cancelled { partial_size };
        self.active_operation_id = None;
        self.cancellation_token = None;
        true
    }

    /// Restore a completed response snapshot (e.g. from history on reopen).
    pub fn restore_completed_response(&mut self, response: ResponseSummary) {
        self.exec_status = ExecStatus::Completed {
            response: Arc::new(response),
        };
        self.active_operation_id = None;
        self.cancellation_token = None;
    }

    /// Restore a failed execution snapshot (e.g. from history on reopen).
    pub fn restore_failed_response(&mut self, summary: String) {
        self.exec_status = ExecStatus::Failed {
            summary,
            classified: None,
        };
        self.active_operation_id = None;
        self.cancellation_token = None;
    }

    /// Restore a cancelled execution snapshot (e.g. from history on reopen).
    pub fn restore_cancelled_response(&mut self, partial_size: Option<u64>) {
        self.exec_status = ExecStatus::Cancelled { partial_size };
        self.active_operation_id = None;
        self.cancellation_token = None;
    }

    // -----------------------------------------------------------------------
    // Operation identity and late-response guards
    // -----------------------------------------------------------------------

    pub fn active_operation_id(&self) -> Option<HistoryEntryId> {
        self.active_operation_id
    }

    pub fn cancellation_token(&self) -> Option<&CancellationToken> {
        self.cancellation_token.as_ref()
    }

    /// Returns true if the given operation ID matches the active operation.
    pub fn is_active_operation(&self, operation_id: HistoryEntryId) -> bool {
        self.active_operation_id == Some(operation_id)
    }

    // -----------------------------------------------------------------------
    // Latest history entry
    // -----------------------------------------------------------------------

    pub fn latest_history_id(&self) -> Option<HistoryEntryId> {
        self.latest_history_id
    }

    pub fn set_latest_history_id(&mut self, id: Option<HistoryEntryId>) {
        self.latest_history_id = id;
    }

    // -----------------------------------------------------------------------
    // Preflight errors
    // -----------------------------------------------------------------------

    pub fn preflight_error(&self) -> Option<&PreflightError> {
        self.preflight_error.as_ref()
    }

    pub fn set_preflight_error(&mut self, message: String) {
        self.preflight_error = Some(PreflightError { message });
    }

    pub fn clear_preflight_error(&mut self) {
        self.preflight_error = None;
    }

    /// Reset exec state after a preflight failure.
    /// Clears the exec status back to Idle and drops the active operation
    /// so the editor is not stuck in Sending.
    pub fn reset_preflight(&mut self) {
        self.exec_status = ExecStatus::Idle;
        self.active_operation_id = None;
        self.cancellation_token = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ids::CollectionId;

    fn test_collection_id() -> CollectionId {
        CollectionId::new()
    }

    #[test]
    fn draft_editor_starts_dirty() {
        let editor = RequestEditorState::new_draft(test_collection_id());
        assert_eq!(editor.save_status(), &SaveStatus::Dirty);
        assert!(editor.baseline().is_none());
        assert!(editor.detect_dirty());
    }

    #[test]
    fn persisted_editor_starts_pristine() {
        let request = RequestItem::new(test_collection_id(), None, "Test", "GET", "/api", 0);
        let editor = RequestEditorState::from_persisted(request);
        assert_eq!(editor.save_status(), &SaveStatus::Pristine);
        assert!(editor.baseline().is_some());
        assert!(!editor.detect_dirty());
    }

    #[test]
    fn editing_draft_makes_dirty() {
        let request = RequestItem::new(test_collection_id(), None, "Test", "GET", "/api", 0);
        let mut editor = RequestEditorState::from_persisted(request);
        editor.draft_mut().url = "/changed".to_string();
        editor.refresh_save_status();
        assert_eq!(editor.save_status(), &SaveStatus::Dirty);
    }

    #[test]
    fn save_roundtrip_resets_to_pristine() {
        let request = RequestItem::new(test_collection_id(), None, "Test", "GET", "/api", 0);
        let mut editor = RequestEditorState::from_persisted(request);
        editor.draft_mut().url = "/changed".to_string();
        editor.begin_save();
        assert_eq!(editor.save_status(), &SaveStatus::Saving);

        let saved = editor.draft().clone();
        editor.complete_save(&saved);
        assert_eq!(editor.save_status(), &SaveStatus::Pristine);
        assert!(!editor.detect_dirty());
    }

    #[test]
    fn save_failed_stays_dirty() {
        let request = RequestItem::new(test_collection_id(), None, "Test", "GET", "/api", 0);
        let mut editor = RequestEditorState::from_persisted(request);
        editor.draft_mut().url = "/changed".to_string();
        editor.begin_save();
        editor.fail_save("conflict".to_string());
        assert!(matches!(
            editor.save_status(),
            SaveStatus::SaveFailed { .. }
        ));
        assert!(editor.detect_dirty());
    }

    #[test]
    fn exec_transitions_idle_to_sending_to_completed() {
        let request = RequestItem::new(test_collection_id(), None, "Test", "GET", "/api", 0);
        let mut editor = RequestEditorState::from_persisted(request);
        assert!(matches!(editor.exec_status(), ExecStatus::Idle));

        let op_id = HistoryEntryId::new();
        let old = editor.begin_send(op_id);
        assert!(old.is_none());
        assert!(editor.exec_status().is_in_flight());

        let summary = ResponseSummary {
            status_code: 200,
            status_text: "OK".to_string(),
            headers_json: None,
            media_type: Some("text/plain".to_string()),
            body_ref: crate::domain::response::BodyRef::Empty,
            total_ms: Some(150),
            ttfb_ms: Some(50),
            dispatched_at_unix_ms: None,
            first_byte_at_unix_ms: None,
            completed_at_unix_ms: None,
        };
        assert!(editor.complete_exec(summary, op_id));
        assert!(editor.exec_status().is_terminal());
    }

    #[test]
    fn late_response_is_ignored() {
        let request = RequestItem::new(test_collection_id(), None, "Test", "GET", "/api", 0);
        let mut editor = RequestEditorState::from_persisted(request);

        let op1 = HistoryEntryId::new();
        let op2 = HistoryEntryId::new();
        let old = editor.begin_send(op1);

        // Auto-cancel: start new operation
        if let Some(token) = old {
            token.cancel();
        }
        let _old2 = editor.begin_send(op2);

        // Late completion for op1 should be ignored
        let summary = ResponseSummary {
            status_code: 200,
            status_text: "OK".to_string(),
            headers_json: None,
            media_type: None,
            body_ref: crate::domain::response::BodyRef::Empty,
            total_ms: None,
            ttfb_ms: None,
            dispatched_at_unix_ms: None,
            first_byte_at_unix_ms: None,
            completed_at_unix_ms: None,
        };
        assert!(!editor.complete_exec(summary, op1));
        assert!(editor.exec_status().is_in_flight()); // Still sending for op2
    }

    #[test]
    fn auto_cancel_on_resend() {
        let request = RequestItem::new(test_collection_id(), None, "Test", "GET", "/api", 0);
        let mut editor = RequestEditorState::from_persisted(request);

        let op1 = HistoryEntryId::new();
        let old = editor.begin_send(op1);
        assert!(old.is_none());
        let token1 = editor.cancellation_token().unwrap().clone();

        let op2 = HistoryEntryId::new();
        let old = editor.begin_send(op2);
        let old_token = old.unwrap();
        assert!(old_token.is_cancelled());
        assert!(token1.is_cancelled()); // Same token, cancelled via begin_send
        assert_eq!(editor.active_operation_id(), Some(op2));
    }

    #[test]
    fn cancel_exec_transitions_to_cancelled() {
        let request = RequestItem::new(test_collection_id(), None, "Test", "GET", "/api", 0);
        let mut editor = RequestEditorState::from_persisted(request);

        let op_id = HistoryEntryId::new();
        editor.begin_send(op_id);
        assert!(editor.cancel_exec(Some(1024), op_id));
        assert!(matches!(
            editor.exec_status(),
            ExecStatus::Cancelled {
                partial_size: Some(1024)
            }
        ));
    }

    #[test]
    fn orthogonal_save_and_exec_status() {
        let request = RequestItem::new(test_collection_id(), None, "Test", "GET", "/api", 0);
        let mut editor = RequestEditorState::from_persisted(request);

        // Edit + save in flight
        editor.draft_mut().url = "/changed".to_string();
        editor.begin_save();

        // Send while saving
        let op_id = HistoryEntryId::new();
        editor.begin_send(op_id);

        assert_eq!(editor.save_status(), &SaveStatus::Saving);
        assert!(editor.exec_status().is_in_flight());

        // Complete exec while still saving
        let summary = ResponseSummary {
            status_code: 200,
            status_text: "OK".to_string(),
            headers_json: None,
            media_type: None,
            body_ref: crate::domain::response::BodyRef::Empty,
            total_ms: None,
            ttfb_ms: None,
            dispatched_at_unix_ms: None,
            first_byte_at_unix_ms: None,
            completed_at_unix_ms: None,
        };
        assert!(editor.complete_exec(summary, op_id));

        // Now complete save
        let saved = editor.draft().clone();
        editor.complete_save(&saved);
        assert_eq!(*editor.save_status(), SaveStatus::Pristine);
    }

    #[test]
    fn draft_transitions_to_persisted_on_first_save() {
        let mut editor = RequestEditorState::new_draft(test_collection_id());
        assert!(matches!(editor.identity(), EditorIdentity::Draft(_)));
        assert!(editor.request_id().is_none());

        let id = crate::domain::ids::RequestId::new();
        let saved = editor.draft().clone();
        editor.transition_to_persisted(id, &saved);
        assert!(matches!(editor.identity(), EditorIdentity::Persisted(_)));
        assert_eq!(editor.request_id(), Some(id));
    }

    #[test]
    fn preflight_error_does_not_change_exec_status() {
        let request = RequestItem::new(test_collection_id(), None, "Test", "GET", "/api", 0);
        let mut editor = RequestEditorState::from_persisted(request);
        assert!(matches!(editor.exec_status(), ExecStatus::Idle));

        editor.set_preflight_error("bad URL".to_string());
        assert!(editor.preflight_error().is_some());
        // Exec status should still be Idle — preflight errors don't move FSM
        assert!(matches!(editor.exec_status(), ExecStatus::Idle));
    }

    #[test]
    fn reload_baseline_keeps_draft() {
        let request = RequestItem::new(test_collection_id(), None, "Test", "GET", "/api", 0);
        let mut editor = RequestEditorState::from_persisted(request);
        editor.draft_mut().url = "/changed".to_string();
        assert!(editor.detect_dirty());

        // Simulate reloading baseline from DB (someone else saved)
        let mut reloaded = editor.draft().clone();
        reloaded.url = "/remote-change".to_string();
        reloaded.meta.revision += 1;
        editor.reload_baseline(reloaded);

        // Draft still has our local changes
        assert_eq!(editor.draft().url, "/changed");
        // Baseline is now the remote version
        assert_eq!(editor.baseline().unwrap().url, "/remote-change");
        // Still dirty because draft diverges from new baseline
        assert!(editor.detect_dirty());
    }

    #[test]
    fn reset_preflight_clears_sending_state() {
        let request = RequestItem::new(test_collection_id(), None, "Test", "GET", "/api", 0);
        let mut editor = RequestEditorState::from_persisted(request);
        assert!(matches!(editor.exec_status(), ExecStatus::Idle));

        // Begin send — transitions to Sending
        let op_id = HistoryEntryId::new();
        editor.begin_send(op_id);
        assert!(editor.exec_status().is_in_flight());
        assert_eq!(editor.active_operation_id(), Some(op_id));
        assert!(editor.cancellation_token().is_some());

        // Reset after preflight failure
        editor.reset_preflight();
        assert!(matches!(editor.exec_status(), ExecStatus::Idle));
        assert!(editor.active_operation_id().is_none());
        assert!(editor.cancellation_token().is_none());

        // A subsequent send should work cleanly
        let op_id2 = HistoryEntryId::new();
        editor.begin_send(op_id2);
        assert!(editor.exec_status().is_in_flight());
        assert_eq!(editor.active_operation_id(), Some(op_id2));
    }
}
