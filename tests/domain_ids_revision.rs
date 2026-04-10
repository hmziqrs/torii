use anyhow::Result;
use torii::domain::{
    ids::{BlobId, CollectionId, WorkspaceId},
    revision::RevisionMetadata,
};

#[test]
fn typed_ids_roundtrip_and_parse_errors() -> Result<()> {
    let workspace_id = WorkspaceId::new();
    let workspace_id_text = workspace_id.to_string();
    let parsed_workspace = WorkspaceId::parse(&workspace_id_text)?;
    assert_eq!(parsed_workspace, workspace_id);

    let collection_id = CollectionId::new();
    let collection_id_text = collection_id.to_string();
    let parsed_collection: CollectionId = collection_id_text.parse()?;
    assert_eq!(parsed_collection, collection_id);

    assert!(WorkspaceId::parse("not-a-uuid").is_err());
    assert!(BlobId::new("").is_err());
    assert!(BlobId::new("   ").is_err());
    assert_eq!(BlobId::new("abc123")?.to_string(), "abc123");

    Ok(())
}

#[test]
fn revision_metadata_touch_bumps_revision_and_timestamp() {
    let mut revision = RevisionMetadata::new_now();
    let created_at = revision.created_at;
    let previous_updated_at = revision.updated_at;
    let previous_revision = revision.revision;

    std::thread::sleep(std::time::Duration::from_millis(5));
    revision.touch();

    assert_eq!(revision.created_at, created_at);
    assert!(revision.updated_at >= previous_updated_at);
    assert_eq!(revision.revision, previous_revision + 1);
}
