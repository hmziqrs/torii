use crate::{
    domain::request::RequestItem,
    repos::request_repo::{RequestRepoError, RequestRepoRef},
};

/// Persist a brand-new draft request by creating the backing row first, then
/// saving the full edited draft payload against the created revision.
pub fn persist_new_draft_request(
    request_repo: &RequestRepoRef,
    draft: &RequestItem,
) -> Result<RequestItem, RequestRepoError> {
    let created = request_repo
        .create(
            draft.collection_id,
            draft.parent_folder_id,
            &draft.name,
            &draft.method,
            &draft.url,
        )
        .map_err(RequestRepoError::Storage)?;

    let mut to_save = draft.clone();
    to_save.id = created.id;
    to_save.collection_id = created.collection_id;
    to_save.parent_folder_id = created.parent_folder_id;
    to_save.sort_order = created.sort_order;
    to_save.meta = created.meta.clone();

    request_repo.save(&to_save, created.meta.revision)
}
