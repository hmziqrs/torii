use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::mpsc::{Receiver, Sender},
    time::{Duration, Instant},
};

use anyhow::Result;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

use crate::domain::ids::CollectionId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkedCollectionEvent {
    pub collection_id: CollectionId,
    pub kind: LinkedCollectionEventKind,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkedCollectionEventKind {
    FileAdded,
    FileChanged,
    FileRemoved,
    DirectoryRemoved,
    FullRescanRequested,
}

#[derive(Debug, Clone)]
struct PendingCollectionEvents {
    first_seen: Instant,
    events: Vec<LinkedCollectionEvent>,
}

#[derive(Debug, Clone)]
pub struct LinkedCollectionReconcileQueue {
    debounce_window: Duration,
    full_rescan_threshold: usize,
    pending: HashMap<CollectionId, PendingCollectionEvents>,
}

impl LinkedCollectionReconcileQueue {
    pub fn new(debounce_window: Duration, full_rescan_threshold: usize) -> Self {
        Self {
            debounce_window,
            full_rescan_threshold,
            pending: HashMap::new(),
        }
    }

    pub fn push(&mut self, event: LinkedCollectionEvent, now: Instant) {
        let entry = self
            .pending
            .entry(event.collection_id)
            .or_insert_with(|| PendingCollectionEvents {
                first_seen: now,
                events: Vec::new(),
            });
        if entry.events.is_empty() {
            entry.first_seen = now;
        }
        entry.events.push(event);
    }

    pub fn drain_ready(&mut self, now: Instant) -> Vec<LinkedCollectionEvent> {
        let mut ready_ids = Vec::new();
        for (collection_id, pending) in &self.pending {
            if now.saturating_duration_since(pending.first_seen) >= self.debounce_window {
                ready_ids.push(*collection_id);
            }
        }

        let mut drained = Vec::new();
        for collection_id in ready_ids {
            if let Some(pending) = self.pending.remove(&collection_id) {
                drained.extend(coalesce_collection_events(
                    collection_id,
                    pending.events,
                    self.full_rescan_threshold,
                ));
            }
        }
        drained
    }
}

fn coalesce_collection_events(
    collection_id: CollectionId,
    events: Vec<LinkedCollectionEvent>,
    full_rescan_threshold: usize,
) -> Vec<LinkedCollectionEvent> {
    if events.is_empty() {
        return Vec::new();
    }
    if events.len() > full_rescan_threshold {
        let path = events
            .first()
            .map(|event| event.path.clone())
            .unwrap_or_default();
        return vec![LinkedCollectionEvent {
            collection_id,
            kind: LinkedCollectionEventKind::FullRescanRequested,
            path,
        }];
    }

    // Deduplicate by path and keep the last event kind for deterministic replay.
    let mut by_path: HashMap<PathBuf, LinkedCollectionEventKind> = HashMap::new();
    for event in events {
        by_path.insert(event.path, event.kind);
    }

    by_path
        .into_iter()
        .map(|(path, kind)| LinkedCollectionEvent {
            collection_id,
            kind,
            path,
        })
        .collect()
}

pub struct LinkedCollectionWatcher {
    watcher: RecommendedWatcher,
    tx: Sender<notify::Result<Event>>,
    rx: Receiver<notify::Result<Event>>,
    watched_roots: HashMap<PathBuf, CollectionId>,
}

impl LinkedCollectionWatcher {
    pub fn new() -> Result<Self> {
        let (tx, rx) = std::sync::mpsc::channel();
        let tx_clone = tx.clone();
        let watcher = notify::recommended_watcher(move |res| {
            // Drop send failures quietly; consumer may have been dropped.
            let _ = tx_clone.send(res);
        })?;

        Ok(Self {
            watcher,
            tx,
            rx,
            watched_roots: HashMap::new(),
        })
    }

    pub fn watch_collection(&mut self, collection_id: CollectionId, root: PathBuf) -> Result<()> {
        self.watcher.watch(&root, RecursiveMode::Recursive)?;
        self.watched_roots.insert(root, collection_id);
        Ok(())
    }

    pub fn unwatch_collection(&mut self, collection_id: CollectionId) -> Result<()> {
        let root = self
            .watched_roots
            .iter()
            .find_map(|(path, id)| (*id == collection_id).then_some(path.clone()));
        if let Some(root) = root {
            self.watcher.unwatch(&root)?;
            self.watched_roots.remove(&root);
        }
        Ok(())
    }

    pub fn poll_events(&self) -> Vec<LinkedCollectionEvent> {
        let mut out = Vec::new();
        while let Ok(event_res) = self.rx.try_recv() {
            let Ok(event) = event_res else {
                continue;
            };
            if let Some((collection_id, path, kind)) = self.map_notify_event(&event) {
                out.push(LinkedCollectionEvent {
                    collection_id,
                    kind,
                    path,
                });
            }
        }
        out
    }

    fn map_notify_event(
        &self,
        event: &Event,
    ) -> Option<(CollectionId, PathBuf, LinkedCollectionEventKind)> {
        let kind = map_event_kind(&event.kind)?;
        let path = event.paths.first()?.clone();
        let collection_id = self.find_collection_for_path(&path)?;
        Some((collection_id, path, kind))
    }

    fn find_collection_for_path(&self, path: &Path) -> Option<CollectionId> {
        // Longest-prefix match handles nested roots deterministically.
        let mut best: Option<(usize, CollectionId)> = None;
        for (root, collection_id) in &self.watched_roots {
            if path.starts_with(root) {
                let score = root.as_os_str().len();
                match best {
                    Some((best_score, _)) if best_score >= score => {}
                    _ => best = Some((score, *collection_id)),
                }
            }
        }
        best.map(|(_, collection_id)| collection_id)
    }

    #[allow(dead_code)]
    pub fn event_sender(&self) -> Sender<notify::Result<Event>> {
        self.tx.clone()
    }
}

fn map_event_kind(kind: &EventKind) -> Option<LinkedCollectionEventKind> {
    match kind {
        EventKind::Create(_) => Some(LinkedCollectionEventKind::FileAdded),
        EventKind::Modify(_) => Some(LinkedCollectionEventKind::FileChanged),
        EventKind::Remove(remove_kind) => match remove_kind {
            notify::event::RemoveKind::Folder => Some(LinkedCollectionEventKind::DirectoryRemoved),
            _ => Some(LinkedCollectionEventKind::FileRemoved),
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_event(collection_id: CollectionId, kind: LinkedCollectionEventKind, path: &str) -> LinkedCollectionEvent {
        LinkedCollectionEvent {
            collection_id,
            kind,
            path: PathBuf::from(path),
        }
    }

    #[test]
    fn queue_debounces_per_collection() {
        let collection_id = CollectionId::new();
        let mut queue = LinkedCollectionReconcileQueue::new(Duration::from_millis(50), 50);
        let start = Instant::now();

        queue.push(
            mk_event(collection_id, LinkedCollectionEventKind::FileChanged, "/tmp/a"),
            start,
        );
        queue.push(
            mk_event(collection_id, LinkedCollectionEventKind::FileChanged, "/tmp/b"),
            start + Duration::from_millis(10),
        );

        let early = queue.drain_ready(start + Duration::from_millis(30));
        assert!(early.is_empty());

        let ready = queue.drain_ready(start + Duration::from_millis(60));
        assert_eq!(ready.len(), 2);
    }

    #[test]
    fn queue_collapses_burst_to_full_rescan() {
        let collection_id = CollectionId::new();
        let mut queue = LinkedCollectionReconcileQueue::new(Duration::from_millis(10), 3);
        let start = Instant::now();
        for i in 0..4 {
            queue.push(
                mk_event(
                    collection_id,
                    LinkedCollectionEventKind::FileChanged,
                    &format!("/tmp/file-{i}.json"),
                ),
                start,
            );
        }

        let ready = queue.drain_ready(start + Duration::from_millis(20));
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].kind, LinkedCollectionEventKind::FullRescanRequested);
    }

    #[test]
    fn queue_deduplicates_by_path_last_event_wins() {
        let collection_id = CollectionId::new();
        let mut queue = LinkedCollectionReconcileQueue::new(Duration::from_millis(5), 50);
        let start = Instant::now();

        queue.push(
            mk_event(collection_id, LinkedCollectionEventKind::FileAdded, "/tmp/same"),
            start,
        );
        queue.push(
            mk_event(collection_id, LinkedCollectionEventKind::FileRemoved, "/tmp/same"),
            start,
        );

        let ready = queue.drain_ready(start + Duration::from_millis(10));
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].kind, LinkedCollectionEventKind::FileRemoved);
    }
}
