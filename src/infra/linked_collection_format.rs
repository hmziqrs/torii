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
    ids::{EnvironmentId, FolderId, WorkspaceId},
    request::RequestItem,
    revision::RevisionMetadata,
};

pub const COLLECTION_META_FILE: &str = ".collection.json";
pub const FOLDER_META_FILE: &str = ".folder.json";
pub const REQUEST_FILE_EXT: &str = ".request.json";
pub const ENV_FILE_EXT: &str = ".env.json";
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct EnvironmentFileCompat {
    environment: EnvironmentCompat,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct EnvironmentCompat {
    id: EnvironmentId,
    #[serde(default)]
    workspace_id: Option<WorkspaceId>,
    #[serde(default)]
    collection_id: Option<String>,
    name: String,
    variables_json: String,
    meta: RevisionMetadata,
}

pub fn write_linked_collection(root: &Path, state: &LinkedCollectionState) -> Result<()> {
    if state.collection.storage_kind != CollectionStorageKind::Linked {
        return Err(anyhow!(
            "linked writer requires linked collection storage kind"
        ));
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

    // Remove existing managed request files before rewriting so rename/migration
    // does not leave stale duplicates behind.
    clear_request_and_folder_artifacts_recursive(root)?;
    clear_environment_files(root)?;

    let collection_meta = CollectionMetaFile {
        format_version: FORMAT_VERSION,
        collection: state.collection.clone(),
        ordered_root_child_ids: if state.root_child_order.is_empty() {
            derive_root_order(state)
        } else {
            state.root_child_order.clone()
        },
    };
    write_json_file(&root.join(COLLECTION_META_FILE), &collection_meta)?;

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
        let file_name = format!("{}{}", sanitize_name(&request.name), REQUEST_FILE_EXT);
        write_json_file(
            &parent_dir.join(file_name),
            &RequestFile {
                request: request.clone(),
            },
        )?;
    }

    for environment in &state.environments {
        let file_name = format!("{}{}", sanitize_name(&environment.name), ENV_FILE_EXT);
        write_json_file(
            &root.join(file_name),
            &EnvironmentFile {
                environment: environment.clone(),
            },
        )?;
    }

    Ok(())
}

pub fn read_linked_collection(root: &Path) -> Result<LinkedCollectionState> {
    let collection_meta_path = root.join(COLLECTION_META_FILE);
    let collection_meta: CollectionMetaFile = read_json_file(&collection_meta_path)?;
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
        root,
        &collection_meta.collection,
        None,
        &mut folders,
        &mut requests,
        &mut folder_child_orders,
    )?;

    let environments =
        read_environment_files_in_dir(root, collection_meta.collection.workspace_id)?;

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
    if name.trim().is_empty() {
        return Err(anyhow!("name cannot be empty"));
    }
    Ok(())
}

fn read_dir_recursive(
    root: &Path,
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
            if name.starts_with('.') {
                continue;
            }
            let folder_meta_path = child_path.join(FOLDER_META_FILE);
            let (folder, ordered_child_ids) = if folder_meta_path.exists() {
                let mut folder_meta: FolderMetaFile = read_json_file(&folder_meta_path)?;
                folder_meta.folder.collection_id = collection.id;
                folder_meta.folder.parent_folder_id = parent_folder_id;
                (folder_meta.folder, folder_meta.ordered_child_ids)
            } else {
                let relative = child_path
                    .strip_prefix(root)
                    .unwrap_or(child_path.as_path());
                let mut folder = Folder::new(collection.id, parent_folder_id, name.to_string(), 0);
                folder.id = FolderId::from(uuid::Uuid::new_v5(
                    &uuid::Uuid::NAMESPACE_URL,
                    format!("torii-folder:{}", relative.display()).as_bytes(),
                ));
                (folder, Vec::new())
            };
            let folder_id = folder.id;
            folder_child_orders.insert(folder_id, ordered_child_ids);
            folders.push(folder);
            read_dir_recursive(
                root,
                &child_path,
                collection,
                Some(folder_id),
                folders,
                requests,
                folder_child_orders,
            )?;
            continue;
        }
        if !is_request_file_name(name) {
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
    for folder in state
        .folders
        .iter()
        .filter(|f| f.parent_folder_id.is_none())
    {
        rows.push((folder.sort_order, folder.id.to_string(), true));
    }
    for request in state
        .requests
        .iter()
        .filter(|r| r.parent_folder_id.is_none())
    {
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
        let path = base.join(sanitize_name(&folder.name));
        if map.values().any(|existing| existing == &path) {
            return Err(anyhow!(
                "duplicate folder path '{}' generated from folder names",
                path.display()
            ));
        }
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

fn is_request_file_name(name: &str) -> bool {
    name.ends_with(REQUEST_FILE_EXT)
}

fn is_environment_file_name(name: &str) -> bool {
    name.ends_with(ENV_FILE_EXT)
}

fn write_json_file<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(value)?;
    fs::write(path, bytes).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

fn is_folder_meta_file_name(name: &str) -> bool {
    name == FOLDER_META_FILE
}

fn clear_request_and_folder_artifacts_recursive(dir: &Path) -> Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    for entry in
        fs::read_dir(dir).with_context(|| format!("failed to read dir {}", dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            if path
                .file_name()
                .and_then(|it| it.to_str())
                .is_some_and(|name| name.starts_with('.'))
            {
                continue;
            }
            clear_request_and_folder_artifacts_recursive(&path)?;
            continue;
        }
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|it| it.to_str()) else {
            continue;
        };
        if is_request_file_name(name) || is_folder_meta_file_name(name) {
            fs::remove_file(&path).with_context(|| {
                format!("failed to remove stale linked artifact {}", path.display())
            })?;
        }
    }
    Ok(())
}

fn clear_environment_files(dir: &Path) -> Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    for entry in
        fs::read_dir(dir).with_context(|| format!("failed to read dir {}", dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|it| it.to_str()) else {
            continue;
        };
        if is_environment_file_name(name) {
            fs::remove_file(&path).with_context(|| {
                format!("failed to remove stale environment file {}", path.display())
            })?;
        }
    }
    Ok(())
}

fn read_environment_files_in_dir(
    dir: &Path,
    workspace_id: WorkspaceId,
) -> Result<Vec<Environment>> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut environments = Vec::new();
    for entry in
        fs::read_dir(dir).with_context(|| format!("failed to read env dir {}", dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if !path
            .file_name()
            .and_then(|it| it.to_str())
            .is_some_and(is_environment_file_name)
        {
            continue;
        }
        let file: EnvironmentFileCompat = read_json_file(&path)?;
        environments.push(Environment {
            id: file.environment.id,
            workspace_id: file.environment.workspace_id.unwrap_or(workspace_id),
            name: file.environment.name,
            variables_json: file.environment.variables_json,
            meta: file.environment.meta,
        });
    }
    Ok(environments)
}

fn read_json_file<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T> {
    let bytes = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    let value = serde_json::from_slice(&bytes)
        .with_context(|| format!("failed to parse {}", path.display()))?;
    Ok(value)
}
