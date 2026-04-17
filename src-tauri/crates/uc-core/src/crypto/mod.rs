//! Cryptographic utilities
//!
//! 这个模块提供加密相关的工具函数,包括:
//!
//! - **身份指纹**: 设备身份的稳定标识和验证
//! - **签名/验签**: Ed25519 签名操作
//! - **随机数生成**: 密码学安全的随机数

pub mod identity_fingerprint;

pub use identity_fingerprint::{FingerprintError, IdentityFingerprint, ShortCodeGenerator};
