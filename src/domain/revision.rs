use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevisionMetadata {
    pub created_at: i64,
    pub updated_at: i64,
    pub revision: i64,
}

impl RevisionMetadata {
    pub fn new_now() -> Self {
        let ts = now_unix_ts();
        Self {
            created_at: ts,
            updated_at: ts,
            revision: 1,
        }
    }

    pub fn touch(&mut self) {
        self.updated_at = now_unix_ts();
        self.revision += 1;
    }
}

pub fn now_unix_ts() -> i64 {
    time::OffsetDateTime::now_utc().unix_timestamp()
}
