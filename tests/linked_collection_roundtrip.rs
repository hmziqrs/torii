use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};

use anyhow::Result;
use torii::{
    domain::{
        collection::{Collection, CollectionStorageConfig, CollectionStorageKind},
        environment::Environment,
        folder::Folder,
        ids::CollectionId,
        request::RequestItem,
    },
    infra::linked_collection_format::{
        LinkedCollectionState, LinkedSiblingId, ensure_not_reserved_name, read_linked_collection,
        write_linked_collection,
    },
};

#[test]
fn linked_collection_roundtrip_preserves_ids_and_order() -> Result<()> {
    let root =
        std::env::temp_dir().join(format!("torii-linked-roundtrip-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(&root)?;

    let workspace_id = torii::domain::ids::WorkspaceId::new();
    let collection_id = CollectionId::new();
    let mut collection = Collection::new(workspace_id, "Linked Collection", 0);
    collection.id = collection_id;
    collection.storage_kind = CollectionStorageKind::Linked;
    collection.storage_config = CollectionStorageConfig {
        linked_root_path: Some(PathBuf::from(&root)),
    };

    let mut folder_auth = Folder::new(collection_id, None, "Auth", 0);
    let mut folder_users = Folder::new(collection_id, None, "Users", 2);
    let folder_nested = Folder::new(collection_id, Some(folder_users.id), "Nested", 0);
    folder_auth.sort_order = 0;
    folder_users.sort_order = 2;

    let req_signin = RequestItem::new(
        collection_id,
        Some(folder_auth.id),
        "Sign In",
        "POST",
        "/sign-in",
        1,
    );
    let req_list = RequestItem::new(
        collection_id,
        Some(folder_users.id),
        "List Users",
        "GET",
        "/users",
        0,
    );
    let req_root = RequestItem::new(collection_id, None, "Health", "GET", "/health", 1);

    let env_local = Environment::new(workspace_id, "Local");

    let root_order = vec![
        LinkedSiblingId::Folder {
            id: folder_auth.id.to_string(),
        },
        LinkedSiblingId::Request {
            id: req_root.id.to_string(),
        },
        LinkedSiblingId::Folder {
            id: folder_users.id.to_string(),
        },
    ];
    let mut folder_orders = HashMap::new();
    folder_orders.insert(
        folder_auth.id,
        vec![LinkedSiblingId::Request {
            id: req_signin.id.to_string(),
        }],
    );
    folder_orders.insert(
        folder_users.id,
        vec![
            LinkedSiblingId::Request {
                id: req_list.id.to_string(),
            },
            LinkedSiblingId::Folder {
                id: folder_nested.id.to_string(),
            },
        ],
    );
    folder_orders.insert(folder_nested.id, Vec::new());

    let state = LinkedCollectionState {
        collection: collection.clone(),
        folders: vec![
            folder_auth.clone(),
            folder_users.clone(),
            folder_nested.clone(),
        ],
        requests: vec![req_signin.clone(), req_list.clone(), req_root.clone()],
        environments: vec![env_local.clone()],
        root_child_order: root_order.clone(),
        folder_child_orders: folder_orders.clone(),
    };

    write_linked_collection(&root, &state)?;
    let roundtrip = read_linked_collection(&root)?;

    assert_eq!(roundtrip.collection.id, collection.id);
    assert_eq!(
        roundtrip.collection.storage_kind,
        CollectionStorageKind::Linked
    );
    assert_eq!(roundtrip.root_child_order, root_order);
    assert_eq!(roundtrip.folder_child_orders, folder_orders);

    let folder_ids = roundtrip
        .folders
        .iter()
        .map(|f| f.id)
        .collect::<HashSet<_>>();
    assert!(folder_ids.contains(&folder_auth.id));
    assert!(folder_ids.contains(&folder_users.id));
    assert!(folder_ids.contains(&folder_nested.id));

    let request_ids = roundtrip
        .requests
        .iter()
        .map(|r| r.id)
        .collect::<HashSet<_>>();
    assert!(request_ids.contains(&req_signin.id));
    assert!(request_ids.contains(&req_list.id));
    assert!(request_ids.contains(&req_root.id));

    assert_eq!(roundtrip.environments.len(), 1);
    assert_eq!(roundtrip.environments[0].id, env_local.id);
    assert_eq!(roundtrip.environments[0].workspace_id, workspace_id);

    Ok(())
}

#[test]
fn linked_collection_rejects_reserved_name() {
    let err = ensure_not_reserved_name("   ").expect_err("expected empty-name rejection");
    assert!(err.to_string().contains("empty"));
}
