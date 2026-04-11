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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ItemKey {
    pub kind: ItemKind,
    pub id: Option<ItemId>,
}

impl ItemKey {
    pub fn new(kind: ItemKind, id: Option<ItemId>) -> Self {
        debug_assert_eq!(kind.is_persisted(), id.is_some());
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
