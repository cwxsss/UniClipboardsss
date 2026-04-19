//! 默认 `KeyScopePort` 实现——单用户模式下固定返回 `profile_id = "default"`。
//!
//! 历史位置:`uc-platform/src/key_scope.rs`。Slice 4 (U4-D) 起搬到 uc-infra:
//! `profile_id` 不是平台差异,而是业务/应用层概念,不应由 uc-platform 承担。

use anyhow::Result;
use uc_core::crypto::model::KeyScope;
use uc_core::ports::security::key_scope::{KeyScopePort, ScopeError};

pub struct DefaultKeyScope {
    scope: KeyScope,
}

impl DefaultKeyScope {
    pub fn new() -> Self {
        Self {
            scope: KeyScope {
                profile_id: "default".to_string(),
            },
        }
    }
}

impl Default for DefaultKeyScope {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl KeyScopePort for DefaultKeyScope {
    async fn current_scope(&self) -> Result<KeyScope, ScopeError> {
        Ok(self.scope.clone())
    }
}
