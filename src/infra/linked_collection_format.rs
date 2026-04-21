use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context as _, Result, anyhow};
use serde::{Deserialize, Serialize};

use crate::domain::{
    collection::{Collection, CollectionStorageKind},
    environment::Environment,
    folder::Folder,
    ids::FolderId,
    request::RequestItem,
};

pub const LINKED_CONTROL_DIR: &str = ".torii";
pub const LINKED_ENV_DIR: &str = "environments";
pub const COLLECTION_META_FILE: &str = "collection.json";
pub const FOLDER_META_FILE: &str = ".torii-folder.json";
pub const REQUEST_FILE_EXT: &str = ".torii-request.json";
pub const ENV_FILE_EXT: &str = ".torii-env.json";
pub const FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LinkedSiblingId {
    Folder { id: String },
    Request { id: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkedCollectionState {
    pub collection: Collection,
    pub folders: Vec<Folder>,
    pub requests: Vec<RequestItem>,
    pub environments: Vec<Environment>,
    pub root_child_order: Vec<LinkedSiblingId>,
    pub folder_child_orders: HashMap<FolderId, Vec<LinkedSiblingId>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct CollectionMetaFile {
    format_version: u32,
    collection: Collection,
    ordered_root_child_ids: Vec<LinkedSiblingId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct FolderMetaFile {
    folder: Folder,
    ordered_child_ids: Vec<LinkedSiblingId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct RequestFile {
    request: RequestItem,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct EnvironmentFile {
    environment: Environment,
}

pub fn write_linked_collection(root: &Path, state: &LinkedCollectionState) -> Result<()> {
    if state.collection.storage_kind != CollectionStorageKind::Linked {
        return Err(anyhow!("linked writer requires linked collection storage kind"));
    }
    fs::create_dir_all(root)
        .with_context(|| format!("failed to create linked root {}", root.display()))?;

    let folder_paths = build_folder_paths(root, &state.folders)?;
    for folder in &state.folders {
        ensure_not_reserved_name(&folder.name)?;
        let path = folder_paths
            .get(&folder.id)
            .ok_or_else(|| anyhow!("missing computed path for folder {}", folder.id))?;
        fs::create_dir_all(path)
            .with_context(|| format!("failed to create folder path {}", path.display()))?;
    }

    let control_dir = root.join(LINKED_CONTROL_DIR);
    let env_dir = control_dir.join(LINKED_ENV_DIR);
    fs::create_dir_all(&env_dir)
        .with_context(|| format!("failed to create linked env dir {}", env_dir.display()))?;

    let collection_meta = CollectionMetaFile {
        format_version: FORMAT_VERSION,
        collection: state.collection.clone(),
        ordered_root_child_ids: if state.root_child_order.is_empty() {
            derive_root_order(state)
        } else {
            state.root_child_order.clone()
        },
    };
    write_json_file(&control_dir.join(COLLECTION_META_FILE), &collection_meta)?;

    let folder_orders = if state.folder_child_orders.is_empty() {
        derive_folder_orders(state)
    } else {
        state.folder_child_orders.clone()
    };
    for folder in &state.folders {
        let path = folder_paths
            .get(&folder.id)
            .ok_or_else(|| anyhow!("missing computed path for folder {}", folder.id))?;
        let meta = FolderMetaFile {
            folder: folder.clone(),
            ordered_child_ids: folder_orders.get(&folder.id).cloned().unwrap_or_default(),
        };
        write_json_file(&path.join(FOLDER_META_FILE), &meta)?;
    }

    for request in &state.requests {
        ensure_not_reserved_name(&request.name)?;
        let parent_dir = if let Some(parent_id) = request.parent_folder_id {
            folder_paths
                .get(&parent_id)
                .cloned()
                .ok_or_else(|| anyhow!("request references missing parent folder {}", parent_id))?
        } else {
            root.to_path_buf()
        };
        let file_name = format!("{}--{}{}", sanitize_name(&request.name), request.id, REQUEST_FILE_EXT);
        write_json_file(
            &parent_dir.join(file_name),
            &RequestFile {
                request: request.clone(),
            },
        )?;
    }

    for environment in &state.environments {
        let file_name = format!(
            "{}--{}{}",
            sanitize_name(&environment.name),
            environment.id,
            ENV_FILE_EXT
        );
        write_json_file(
            &env_dir.join(file_name),
            &EnvironmentFile {
                environment: environment.clone(),
            },
        )?;
    }

    Ok(())
}

pub fn read_linked_collection(root: &Path) -> Result<LinkedCollectionState> {
    let control_path = root.join(LINKED_CONTROL_DIR).join(COLLECTION_META_FILE);
    let collection_meta: CollectionMetaFile = read_json_file(&control_path)?;
    if collection_meta.format_version != FORMAT_VERSION {
        return Err(anyhow!(
            "unsupported linked format version {}",
            collection_meta.format_version
        ));
    }

    let mut folders = Vec::new();
    let mut requests = Vec::new();
    let mut folder_child_orders: HashMap<FolderId, Vec<LinkedSiblingId>> = HashMap::new();
    read_dir_recursive(
        root,
        &collection_meta.collection,
        None,
        &mut folders,
        &mut requests,
        &mut folder_child_orders,
    )?;

    let mut environments = Vec::new();
    let env_dir = root.join(LINKED_CONTROL_DIR).join(LINKED_ENV_DIR);
    if env_dir.exists() {
        for entry in fs::read_dir(&env_dir)
            .with_context(|| format!("failed to read env dir {}", env_dir.display()))?
        {
            let entry = entry?;
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            if !path
                .file_name()
                .and_then(|it| it.to_str())
                .is_some_and(|name| name.ends_with(ENV_FILE_EXT))
            {
                continue;
            }
            let mut file: EnvironmentFile = read_json_file(&path)?;
            file.environment.collection_id = collection_meta.collection.id;
            environments.push(file.environment);
        }
    }

    apply_sibling_order(
        &collection_meta.ordered_root_child_ids,
        &mut folders,
        &mut requests,
        None,
    );
    for (folder_id, order) in &folder_child_orders {
        apply_sibling_order(order, &mut folders, &mut requests, Some(*folder_id));
    }

    Ok(LinkedCollectionState {
        collection: collection_meta.collection,
        folders,
        requests,
        environments,
        root_child_order: collection_meta.ordered_root_child_ids,
        folder_child_orders,
    })
}

pub fn ensure_not_reserved_name(name: &str) -> Result<()> {
    if name.trim() == LINKED_CONTROL_DIR {
        return Err(anyhow!(
            "reserved name '{}' cannot be used in linked collections",
            LINKED_CONTROL_DIR
        ));
    }
    Ok(())
}

fn read_dir_recursive(
    path: &Path,
    collection: &Collection,
    parent_folder_id: Option<FolderId>,
    folders: &mut Vec<Folder>,
    requests: &mut Vec<RequestItem>,
    folder_child_orders: &mut HashMap<FolderId, Vec<LinkedSiblingId>>,
) -> Result<()> {
    for entry in fs::read_dir(path).with_context(|| format!("failed to read {}", path.display()))? {
        let entry = entry?;
        let child_path = entry.path();
        let Some(name) = child_path.file_name().and_then(|it| it.to_str()) else {
            continue;
        };
        if child_path.is_dir() {
            if name == LINKED_CONTROL_DIR {
                continue;
            }
            let folder_meta_path = child_path.join(FOLDER_META_FILE);
            if !folder_meta_path.exists() {
                continue;
            }
            let mut folder_meta: FolderMetaFile = read_json_file(&folder_meta_path)?;
            folder_meta.folder.collection_id = collection.id;
            folder_meta.folder.parent_folder_id = parent_folder_id;
            let folder_id = folder_meta.folder.id;
            folder_child_orders.insert(folder_id, folder_meta.ordered_child_ids);
            folders.push(folder_meta.folder.clone());
            read_dir_recursive(
                &child_path,
                collection,
                Some(folder_id),
                folders,
                requests,
                folder_child_orders,
            )?;
            continue;
        }
        if !name.ends_with(REQUEST_FILE_EXT) {
            continue;
        }
        let mut request_file: RequestFile = read_json_file(&child_path)?;
        request_file.request.collection_id = collection.id;
        request_file.request.parent_folder_id = parent_folder_id;
        requests.push(request_file.request);
    }
    Ok(())
}

fn apply_sibling_order(
    ordered: &[LinkedSiblingId],
    folders: &mut [Folder],
    requests: &mut [RequestItem],
    parent_folder_id: Option<FolderId>,
) {
    let mut order_map: BTreeMap<String, i64> = BTreeMap::new();
    for (index, sibling) in ordered.iter().enumerate() {
        match sibling {
            LinkedSiblingId::Folder { id } | LinkedSiblingId::Request { id } => {
                order_map.insert(id.clone(), index as i64);
            }
        }
    }

    let mut next_index = order_map.len() as i64;
    for folder in folders
        .iter()
        .filter(|f| f.parent_folder_id == parent_folder_id)
    {
        order_map.entry(folder.id.to_string()).or_insert_with(|| {
            let value = next_index;
            next_index += 1;
            value
        });
    }
    for request in requests
        .iter()
        .filter(|r| r.parent_folder_id == parent_folder_id)
    {
        order_map.entry(request.id.to_string()).or_insert_with(|| {
            let value = next_index;
            next_index += 1;
            value
        });
    }

    for folder in folders
        .iter_mut()
        .filter(|f| f.parent_folder_id == parent_folder_id)
    {
        if let Some(order) = order_map.get(&folder.id.to_string()) {
            folder.sort_order = *order;
        }
    }
    for request in requests
        .iter_mut()
        .filter(|r| r.parent_folder_id == parent_folder_id)
    {
        if let Some(order) = order_map.get(&request.id.to_string()) {
            request.sort_order = *order;
        }
    }
}

fn derive_root_order(state: &LinkedCollectionState) -> Vec<LinkedSiblingId> {
    let mut rows = Vec::new();
    for folder in state.folders.iter().filter(|f| f.parent_folder_id.is_none()) {
        rows.push((folder.sort_order, folder.id.to_string(), true));
    }
    for request in state.requests.iter().filter(|r| r.parent_folder_id.is_none()) {
        rows.push((request.sort_order, request.id.to_string(), false));
    }
    rows.sort_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));
    rows.into_iter()
        .map(|(_, id, is_folder)| {
            if is_folder {
                LinkedSiblingId::Folder { id }
            } else {
                LinkedSiblingId::Request { id }
            }
        })
        .collect()
}

fn derive_folder_orders(state: &LinkedCollectionState) -> HashMap<FolderId, Vec<LinkedSiblingId>> {
    let mut by_folder = HashMap::new();
    for folder in &state.folders {
        let mut rows = Vec::new();
        for child in state
            .folders
            .iter()
            .filter(|f| f.parent_folder_id == Some(folder.id))
        {
            rows.push((child.sort_order, child.id.to_string(), true));
        }
        for request in state
            .requests
            .iter()
            .filter(|r| r.parent_folder_id == Some(folder.id))
        {
            rows.push((request.sort_order, request.id.to_string(), false));
        }
        rows.sort_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));
        by_folder.insert(
            folder.id,
            rows.into_iter()
                .map(|(_, id, is_folder)| {
                    if is_folder {
                        LinkedSiblingId::Folder { id }
                    } else {
                        LinkedSiblingId::Request { id }
                    }
                })
                .collect(),
        );
    }
    by_folder
}

fn build_folder_paths(root: &Path, folders: &[Folder]) -> Result<HashMap<FolderId, PathBuf>> {
    let mut map = HashMap::new();
    let by_id: HashMap<FolderId, &Folder> = folders.iter().map(|f| (f.id, f)).collect();
    let mut visiting = HashSet::new();

    fn resolve_path(
        folder: &Folder,
        root: &Path,
        by_id: &HashMap<FolderId, &Folder>,
        map: &mut HashMap<FolderId, PathBuf>,
        visiting: &mut HashSet<FolderId>,
    ) -> Result<PathBuf> {
        if let Some(existing) = map.get(&folder.id) {
            return Ok(existing.clone());
        }
        if !visiting.insert(folder.id) {
            return Err(anyhow!("cycle detected while resolving folder paths"));
        }
        let base = if let Some(parent_id) = folder.parent_folder_id {
            let parent = by_id
                .get(&parent_id)
                .copied()
                .ok_or_else(|| anyhow!("missing parent folder {}", parent_id))?;
            resolve_path(parent, root, by_id, map, visiting)?
        } else {
            root.to_path_buf()
        };
        let path = base.join(format!("{}--{}", sanitize_name(&folder.name), folder.id));
        map.insert(folder.id, path.clone());
        visiting.remove(&folder.id);
        Ok(path)
    }

    for folder in folders {
        resolve_path(folder, root, &by_id, &mut map, &mut visiting)?;
    }
    Ok(map)
}

fn sanitize_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len().max(1));
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            out.push(ch);
        } else {
            out.push('-');
        }
    }
    let trimmed = out.trim_matches('-');
    if trimmed.is_empty() {
        "item".to_string()
    } else {
        trimmed.to_lowercase()
    }
}

fn write_json_file<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(value)?;
    fs::write(path, bytes).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

fn read_json_file<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T> {
    let bytes = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    let value = serde_json::from_slice(&bytes)
        .with_context(|| format!("failed to parse {}", path.display()))?;
    Ok(value)
}
