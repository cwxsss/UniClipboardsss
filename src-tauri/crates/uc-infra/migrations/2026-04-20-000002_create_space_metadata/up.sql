-- 空间元数据持久化（v2 加密模型）。
--
-- 存储每个空间的 SRK 派生参数 + 被 dmk_wrap_key 包装的 DMK。
-- payload 是不透明的持久化格式 blob（当前为 serde_json 编码的 v2 结构），
-- 由 uc-infra::security::space_encryption::payload 模块序列化/反序列化。
-- uc-core 的 SpaceMetadataRepositoryPort 只看到 &[u8]，不知道格式细节。

CREATE TABLE space_metadata (
    space_id       TEXT    PRIMARY KEY NOT NULL,
    payload        BLOB    NOT NULL,
    updated_at_ms  INTEGER NOT NULL
);
