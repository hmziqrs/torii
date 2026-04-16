use super::*;

impl RequestTabView {
    pub(super) fn normalize_auth_secret_ownership_for_save(
        &self,
        request: &mut RequestItem,
        services: &Arc<AppServices>,
    ) -> Result<(), String> {
        let target_owner_kind = "request";
        let target_owner_id = request.id.to_string();

        let source_owner = match self.editor.identity() {
            EditorIdentity::Draft(draft_id) => Some(("request_draft", draft_id.to_string())),
            EditorIdentity::Persisted(id) => Some(("request", id.to_string())),
        };

        match &mut request.auth {
            AuthType::None => Ok(()),
            AuthType::Basic {
                password_secret_ref,
                ..
            } => self.rebind_secret_ref(
                password_secret_ref,
                "basic_password",
                source_owner.as_ref().map(|(k, v)| (*k, v.as_str())),
                target_owner_kind,
                &target_owner_id,
                services,
            ),
            AuthType::Bearer { token_secret_ref } => self.rebind_secret_ref(
                token_secret_ref,
                "bearer_token",
                source_owner.as_ref().map(|(k, v)| (*k, v.as_str())),
                target_owner_kind,
                &target_owner_id,
                services,
            ),
            AuthType::ApiKey {
                value_secret_ref, ..
            } => self.rebind_secret_ref(
                value_secret_ref,
                "api_key_value",
                source_owner.as_ref().map(|(k, v)| (*k, v.as_str())),
                target_owner_kind,
                &target_owner_id,
                services,
            ),
        }
    }

    pub(super) fn rebind_secret_ref(
        &self,
        slot: &mut Option<String>,
        secret_kind: &str,
        source_owner: Option<(&str, &str)>,
        target_owner_kind: &str,
        target_owner_id: &str,
        services: &Arc<AppServices>,
    ) -> Result<(), String> {
        let Some(current_ref) = slot.clone() else {
            return Ok(());
        };

        let value = services
            .secret_store
            .get_secret(&current_ref)
            .map_err(|e| {
                format!(
                    "{}: {e}",
                    es_fluent::localize("request_tab_save_failed", None)
                )
            })?
            .ok_or_else(|| es_fluent::localize("request_tab_secret_missing", None).to_string())?;

        let new_ref = services
            .secret_manager
            .upsert_secret(target_owner_kind, target_owner_id, secret_kind, &value)
            .map_err(|e| {
                format!(
                    "{}: {e}",
                    es_fluent::localize("request_tab_save_failed", None)
                )
            })?;

        *slot = Some(new_ref.key_name.clone());

        if let Some((owner_kind, owner_id)) = source_owner {
            if owner_kind == "request_draft" {
                let _ = services
                    .secret_manager
                    .delete_secret(owner_kind, owner_id, secret_kind);
                if current_ref != new_ref.key_name {
                    let _ = services.secret_store.delete_secret(&current_ref);
                }
            }
        }

        Ok(())
    }

    pub(super) fn clone_auth_secrets_for_duplicate(
        &self,
        source: &RequestItem,
        duplicate: &mut RequestItem,
        services: &Arc<AppServices>,
    ) -> Result<(), String> {
        let target_owner_id = duplicate.id.to_string();

        match (&source.auth, &mut duplicate.auth) {
            (
                AuthType::Basic {
                    password_secret_ref: src,
                    ..
                },
                AuthType::Basic {
                    password_secret_ref: dst,
                    ..
                },
            ) => self.clone_one_secret(
                src.as_ref(),
                dst,
                "basic_password",
                &target_owner_id,
                services,
            ),
            (
                AuthType::Bearer {
                    token_secret_ref: src,
                },
                AuthType::Bearer {
                    token_secret_ref: dst,
                },
            ) => self.clone_one_secret(
                src.as_ref(),
                dst,
                "bearer_token",
                &target_owner_id,
                services,
            ),
            (
                AuthType::ApiKey {
                    value_secret_ref: src,
                    ..
                },
                AuthType::ApiKey {
                    value_secret_ref: dst,
                    ..
                },
            ) => self.clone_one_secret(
                src.as_ref(),
                dst,
                "api_key_value",
                &target_owner_id,
                services,
            ),
            _ => Ok(()),
        }
    }

    pub(super) fn clone_one_secret(
        &self,
        source_ref: Option<&String>,
        destination_ref: &mut Option<String>,
        secret_kind: &str,
        target_owner_id: &str,
        services: &Arc<AppServices>,
    ) -> Result<(), String> {
        let Some(source_ref) = source_ref else {
            *destination_ref = None;
            return Ok(());
        };

        let value = services
            .secret_store
            .get_secret(source_ref)
            .map_err(|e| {
                format!(
                    "{}: {e}",
                    es_fluent::localize("request_tab_duplicate_failed", None)
                )
            })?
            .ok_or_else(|| es_fluent::localize("request_tab_secret_missing", None).to_string())?;

        let new_ref = services
            .secret_manager
            .upsert_secret("request", target_owner_id, secret_kind, &value)
            .map_err(|e| {
                format!(
                    "{}: {e}",
                    es_fluent::localize("request_tab_duplicate_failed", None)
                )
            })?;

        *destination_ref = Some(new_ref.key_name);
        Ok(())
    }
}
