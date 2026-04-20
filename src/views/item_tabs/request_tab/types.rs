use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RequestSectionTab {
    Params,
    Auth,
    Headers,
    Body,
    Scripts,
    Tests,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ResponseTab {
    Body,
    Preview,
    Headers,
    Cookies,
    Timing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ResponseMetaHover {
    None,
    Status,
    Time,
    Size,
    Network,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BodyFileTarget {
    Binary,
    FormDataIndex(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum KvTarget {
    Params,
    Headers,
    BodyUrlEncoded,
    BodyFormDataText,
}

pub(super) struct KeyValueEditorRow {
    pub(super) id: u64,
    pub(super) enabled: bool,
    pub(super) key_input: Entity<InputState>,
    pub(super) value_input: Entity<InputState>,
}

pub(super) const LARGE_BODY_FILE_CONFIRM_BYTES: u64 = 100 * 1024 * 1024;

#[derive(Debug, Default)]
pub(super) struct ReentrancyGuard {
    pub(super) active: bool,
    pub(super) deferred: bool,
}

impl ReentrancyGuard {
    pub(super) fn enter(&mut self) -> bool {
        if self.active {
            self.deferred = true;
            return false;
        }
        self.active = true;
        true
    }

    pub(super) fn leave_and_take_deferred(&mut self) -> bool {
        self.active = false;
        let deferred = self.deferred;
        self.deferred = false;
        deferred
    }

    pub(super) fn is_active(&self) -> bool {
        self.active
    }
}
