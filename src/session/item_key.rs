use std::{fmt, str::FromStr};

use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};

use crate::domain::item_id::ItemId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ItemKind {
    Workspace,
    Collection,
    Folder,
    Environment,
    Request,
    Settings,
    About,
}

impl ItemKind {
    pub fn is_persisted(self) -> bool {
        matches!(
            self,
            Self::Workspace
                | Self::Collection
                | Self::Folder
                | Self::Environment
                | Self::Request
        )
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Workspace => "workspace",
            Self::Collection => "collection",
            Self::Folder => "folder",
            Self::Environment => "environment",
            Self::Request => "request",
            Self::Settings => "settings",
            Self::About => "about",
        }
    }
}

impl fmt::Display for ItemKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ItemKind {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "workspace" => Ok(Self::Workspace),
            "collection" => Ok(Self::Collection),
            "folder" => Ok(Self::Folder),
            "environment" => Ok(Self::Environment),
            "request" => Ok(Self::Request),
            "settings" => Ok(Self::Settings),
            "about" => Ok(Self::About),
            other => Err(anyhow!("unknown item kind: {other}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ItemKey {
    pub kind: ItemKind,
    pub id: Option<ItemId>,
}

impl ItemKey {
    pub fn new(kind: ItemKind, id: Option<ItemId>) -> Self {
        assert_eq!(kind.is_persisted(), id.is_some());
        Self { kind, id }
    }

    pub fn workspace(id: impl Into<ItemId>) -> Self {
        Self::new(ItemKind::Workspace, Some(id.into()))
    }

    pub fn collection(id: impl Into<ItemId>) -> Self {
        Self::new(ItemKind::Collection, Some(id.into()))
    }

    pub fn folder(id: impl Into<ItemId>) -> Self {
        Self::new(ItemKind::Folder, Some(id.into()))
    }

    pub fn environment(id: impl Into<ItemId>) -> Self {
        Self::new(ItemKind::Environment, Some(id.into()))
    }

    pub fn request(id: impl Into<ItemId>) -> Self {
        Self::new(ItemKind::Request, Some(id.into()))
    }

    pub fn settings() -> Self {
        Self::new(ItemKind::Settings, None)
    }

    pub fn about() -> Self {
        Self::new(ItemKind::About, None)
    }

    pub fn is_persisted(self) -> bool {
        self.kind.is_persisted()
    }

    pub fn to_storage_parts(self) -> (String, Option<String>) {
        let id = self.id.map(|id| match id {
            ItemId::Workspace(id) => id.to_string(),
            ItemId::Collection(id) => id.to_string(),
            ItemId::Folder(id) => id.to_string(),
            ItemId::Environment(id) => id.to_string(),
            ItemId::Request(id) => id.to_string(),
        });

        (self.kind.to_string(), id)
    }

    pub fn from_storage_parts(kind: &str, id: Option<&str>) -> Result<Self> {
        use crate::domain::ids::{CollectionId, EnvironmentId, FolderId, RequestId, WorkspaceId};

        let kind = ItemKind::from_str(kind)?;
        let id = match kind {
            ItemKind::Workspace => Some(ItemId::Workspace(WorkspaceId::parse(
                id.ok_or_else(|| anyhow!("missing workspace item id"))?,
            )?)),
            ItemKind::Collection => Some(ItemId::Collection(CollectionId::parse(
                id.ok_or_else(|| anyhow!("missing collection item id"))?,
            )?)),
            ItemKind::Folder => Some(ItemId::Folder(FolderId::parse(
                id.ok_or_else(|| anyhow!("missing folder item id"))?,
            )?)),
            ItemKind::Environment => Some(ItemId::Environment(EnvironmentId::parse(
                id.ok_or_else(|| anyhow!("missing environment item id"))?,
            )?)),
            ItemKind::Request => Some(ItemId::Request(RequestId::parse(
                id.ok_or_else(|| anyhow!("missing request item id"))?,
            )?)),
            ItemKind::Settings | ItemKind::About => None,
        };

        Ok(Self::new(kind, id))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TabKey(pub ItemKey);

impl TabKey {
    pub fn new(item: ItemKey) -> Self {
        Self(item)
    }

    pub fn item(self) -> ItemKey {
        self.0
    }
}

impl From<ItemKey> for TabKey {
    fn from(value: ItemKey) -> Self {
        Self::new(value)
    }
}
