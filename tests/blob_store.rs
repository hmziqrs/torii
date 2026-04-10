mod common;

use std::{collections::HashSet, io::Read as _};

use anyhow::Result;
use torii::infra::blobs::BlobStore;

#[test]
fn blob_store_writes_reads_and_cleans_orphans() -> Result<()> {
    let (paths, _db) = common::test_database("blob-store")?;
    let blob_store = BlobStore::new(&paths)?;

    let large = vec![b'a'; 8192];
    let stored = blob_store.write_from_reader(
        std::io::Cursor::new(large.clone()),
        Some("application/octet-stream"),
    )?;
    assert!(stored.path.exists());
    assert!(stored.preview_truncated);
    assert_eq!(stored.preview.len(), 4096);

    let preview = blob_store.read_preview(&stored.hash, 32)?;
    assert_eq!(preview.len(), 32);
    assert_eq!(preview, vec![b'a'; 32]);

    let full = blob_store.read_all(&stored.hash)?;
    assert_eq!(full.len(), 8192);

    let mut stream = blob_store.open_read(&stored.hash)?;
    let mut from_stream = Vec::new();
    stream.read_to_end(&mut from_stream)?;
    assert_eq!(from_stream, large);

    let orphan = blob_store.write_bytes(b"orphan", Some("text/plain"))?;
    assert!(orphan.path.exists());

    let mut referenced = HashSet::new();
    referenced.insert(stored.hash.clone());
    let removed = blob_store.cleanup_orphan_blobs(&referenced)?;
    assert_eq!(removed, 1);
    assert!(stored.path.exists());
    assert!(!orphan.path.exists());

    Ok(())
}
