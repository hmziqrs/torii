use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use anyhow::Result;
use notify_debouncer_full::{
    DebounceEventResult, DebouncedEvent, new_debouncer,
    notify::{EventKind, RecursiveMode, Watcher},
};

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

pub struct LinkedCollectionMonitor {
    pending_events: Arc<Mutex<Vec<LinkedCollectionEvent>>>,
    stop: Arc<AtomicBool>,
    _worker: std::thread::JoinHandle<()>,
}

impl LinkedCollectionMonitor {
    pub fn start_for_roots(roots: Vec<(CollectionId, PathBuf)>) -> Result<Self> {
        let pending_events = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let worker_pending = pending_events.clone();
        let worker_stop = stop.clone();

        let worker = std::thread::spawn(move || {
            let root_map = roots;
            let mut callback_root_map = HashMap::new();
            for (collection_id, root) in &root_map {
                callback_root_map.insert(root.clone(), *collection_id);
            }

            let callback_pending = worker_pending.clone();
            let callback = move |res: DebounceEventResult| {
                let Ok(events) = res else {
                    return;
                };
                let mapped = map_debounced_events(events, &callback_root_map, 50);
                if mapped.is_empty() {
                    return;
                }
                if let Ok(mut guard) = callback_pending.lock() {
                    guard.extend(mapped);
                }
            };

            let mut debouncer =
                match new_debouncer(Duration::from_millis(75), None, callback)
            {
                Ok(debouncer) => debouncer,
                Err(err) => {
                    tracing::error!("failed to initialize linked collection debouncer: {err}");
                    return;
                }
            };

            for (_, root) in &root_map {
                if let Err(err) = debouncer.watcher().watch(root, RecursiveMode::Recursive) {
                    tracing::warn!(root = %root.display(), "failed to watch linked root: {err}");
                }
            }

            while !worker_stop.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(50));
            }
        });

        Ok(Self {
            pending_events,
            stop,
            _worker: worker,
        })
    }

    pub fn drain_events(&self) -> Vec<LinkedCollectionEvent> {
        if let Ok(mut guard) = self.pending_events.lock() {
            std::mem::take(&mut *guard)
        } else {
            Vec::new()
        }
    }
}

impl Drop for LinkedCollectionMonitor {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

fn map_debounced_events(
    events: Vec<DebouncedEvent>,
    roots: &HashMap<PathBuf, CollectionId>,
    full_rescan_threshold: usize,
) -> Vec<LinkedCollectionEvent> {
    if events.is_empty() {
        return Vec::new();
    }

    let mut by_collection: HashMap<CollectionId, Vec<LinkedCollectionEvent>> = HashMap::new();
    for event in events {
        let Some(path) = event.event.paths.first().cloned() else {
            continue;
        };
        let Some(collection_id) = find_collection_for_path(&path, roots) else {
            continue;
        };
        let Some(kind) = map_event_kind(&event.event.kind) else {
            continue;
        };
        by_collection
            .entry(collection_id)
            .or_default()
            .push(LinkedCollectionEvent {
                collection_id,
                kind,
                path,
            });
    }

    let mut out = Vec::new();
    for (collection_id, mut collection_events) in by_collection {
        if collection_events.len() > full_rescan_threshold {
            let path = collection_events
                .first()
                .map(|event| event.path.clone())
                .unwrap_or_default();
            out.push(LinkedCollectionEvent {
                collection_id,
                kind: LinkedCollectionEventKind::FullRescanRequested,
                path,
            });
            continue;
        }
        // Deduplicate same-path events within the debounce window: last one wins.
        let mut by_path: HashMap<PathBuf, LinkedCollectionEventKind> = HashMap::new();
        for event in collection_events.drain(..) {
            by_path.insert(event.path, event.kind);
        }
        out.extend(by_path.into_iter().map(|(path, kind)| LinkedCollectionEvent {
            collection_id,
            kind,
            path,
        }));
    }
    out
}

fn find_collection_for_path(path: &Path, roots: &HashMap<PathBuf, CollectionId>) -> Option<CollectionId> {
    let mut best: Option<(usize, CollectionId)> = None;
    for (root, collection_id) in roots {
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

fn map_event_kind(kind: &EventKind) -> Option<LinkedCollectionEventKind> {
    match kind {
        EventKind::Create(_) => Some(LinkedCollectionEventKind::FileAdded),
        EventKind::Modify(_) => Some(LinkedCollectionEventKind::FileChanged),
        EventKind::Remove(remove_kind) => match remove_kind {
            notify_debouncer_full::notify::event::RemoveKind::Folder => {
                Some(LinkedCollectionEventKind::DirectoryRemoved)
            }
            _ => Some(LinkedCollectionEventKind::FileRemoved),
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use notify_debouncer_full::notify::event::{CreateKind, ModifyKind, RemoveKind};

    fn ev(path: &str, kind: EventKind) -> DebouncedEvent {
        DebouncedEvent {
            event: notify_debouncer_full::notify::Event {
                kind,
                paths: vec![PathBuf::from(path)],
                attrs: Default::default(),
            },
            time: std::time::Instant::now(),
        }
    }

    #[test]
    fn map_events_collapse_to_full_rescan_for_burst() {
        let collection_id = CollectionId::new();
        let root = PathBuf::from("/tmp/root");
        let roots = HashMap::from([(root.clone(), collection_id)]);
        let events = (0..60)
            .map(|i| ev(&format!("/tmp/root/f{i}.json"), EventKind::Modify(ModifyKind::Any)))
            .collect::<Vec<_>>();

        let mapped = map_debounced_events(events, &roots, 50);
        assert_eq!(mapped.len(), 1);
        assert_eq!(mapped[0].kind, LinkedCollectionEventKind::FullRescanRequested);
        assert_eq!(mapped[0].collection_id, collection_id);
    }

    #[test]
    fn map_events_dedupe_same_path_last_wins() {
        let collection_id = CollectionId::new();
        let root = PathBuf::from("/tmp/root");
        let roots = HashMap::from([(root.clone(), collection_id)]);
        let path = "/tmp/root/a.json";
        let events = vec![
            ev(path, EventKind::Create(CreateKind::File)),
            ev(path, EventKind::Modify(ModifyKind::Any)),
            ev(path, EventKind::Remove(RemoveKind::File)),
        ];

        let mapped = map_debounced_events(events, &roots, 50);
        assert_eq!(mapped.len(), 1);
        assert_eq!(mapped[0].kind, LinkedCollectionEventKind::FileRemoved);
    }
}
