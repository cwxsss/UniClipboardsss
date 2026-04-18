use async_trait::async_trait;
use diesel::prelude::*;

use uc_core::{DeviceId, MemberRepositoryPort, MembershipError, SpaceMember};

use crate::db::models::{NewSpaceMemberRow, SpaceMemberRow};
use crate::db::ports::{DbExecutor, InsertMapper, RowMapper};
use crate::db::schema::space_member::dsl::*;

pub struct DieselSpaceMemberRepository<E, M> {
    executor: E,
    mapper: M,
}

impl<E, M> DieselSpaceMemberRepository<E, M> {
    pub fn new(executor: E, mapper: M) -> Self {
        Self { executor, mapper }
    }
}

#[async_trait]
impl<E, M> MemberRepositoryPort for DieselSpaceMemberRepository<E, M>
where
    E: DbExecutor,
    M: InsertMapper<SpaceMember, NewSpaceMemberRow>
        + RowMapper<SpaceMemberRow, SpaceMember>
        + Send
        + Sync,
{
    async fn get(
        &self,
        device_id_value: &DeviceId,
    ) -> Result<Option<SpaceMember>, MembershipError> {
        let id = device_id_value.as_str().to_string();
        self.executor
            .run(move |conn| {
                let row = space_member
                    .filter(device_id.eq(&id))
                    .first::<SpaceMemberRow>(conn)
                    .optional()
                    .map_err(|e| anyhow::anyhow!(e.to_string()))?;

                match row {
                    Some(r) => {
                        let member = self
                            .mapper
                            .to_domain(&r)
                            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
                        Ok(Some(member))
                    }
                    None => Ok(None),
                }
            })
            .map_err(|e| MembershipError::Repository(e.to_string()))
    }

    async fn list(&self) -> Result<Vec<SpaceMember>, MembershipError> {
        self.executor
            .run(|conn| {
                let rows = space_member
                    .load::<SpaceMemberRow>(conn)
                    .map_err(|e| anyhow::anyhow!(e.to_string()))?;

                let mut members = Vec::with_capacity(rows.len());
                for row in rows {
                    let id = row.device_id.clone();
                    let member = self.mapper.to_domain(&row).map_err(|e| {
                        anyhow::anyhow!("Failed to map space_member device_id {}: {}", id, e)
                    })?;
                    members.push(member);
                }

                Ok(members)
            })
            .map_err(|e| MembershipError::Repository(e.to_string()))
    }

    async fn save(&self, member: &SpaceMember) -> Result<(), MembershipError> {
        let row = self
            .mapper
            .to_row(member)
            .map_err(|e| MembershipError::Repository(e.to_string()))?;

        self.executor
            .run(move |conn| {
                diesel::insert_into(space_member)
                    .values(&row)
                    .on_conflict(device_id)
                    .do_update()
                    .set((
                        device_name.eq(row.device_name.clone()),
                        identity_fingerprint.eq(row.identity_fingerprint.clone()),
                        joined_at.eq(row.joined_at),
                        sync_preferences.eq(row.sync_preferences.clone()),
                    ))
                    .execute(conn)
                    .map_err(|e| anyhow::anyhow!(e.to_string()))?;
                Ok(())
            })
            .map_err(|e| MembershipError::Repository(e.to_string()))
    }

    async fn remove(&self, device_id_value: &DeviceId) -> Result<bool, MembershipError> {
        let id = device_id_value.as_str().to_string();
        let affected = self
            .executor
            .run(move |conn| {
                diesel::delete(space_member.filter(device_id.eq(&id)))
                    .execute(conn)
                    .map_err(|e| anyhow::anyhow!(e.to_string()))
            })
            .map_err(|e| MembershipError::Repository(e.to_string()))?;

        Ok(affected > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::executor::DieselSqliteExecutor;
    use crate::db::mappers::space_member_mapper::SpaceMemberRowMapper;
    use crate::db::pool::init_db_pool;
    use chrono::Utc;
    use tempfile::{tempdir, TempDir};
    use uc_core::{DeviceId, MemberSyncPreferences, SpaceMember};

    fn make_repo() -> (
        DieselSpaceMemberRepository<DieselSqliteExecutor, SpaceMemberRowMapper>,
        TempDir,
    ) {
        let tempdir = tempdir().unwrap();
        let database_url = tempdir.path().join("space-member.sqlite");
        let pool = init_db_pool(database_url.to_str().unwrap()).unwrap();
        let repo =
            DieselSpaceMemberRepository::new(DieselSqliteExecutor::new(pool), SpaceMemberRowMapper);
        (repo, tempdir)
    }

    fn fixture_member(id: &str) -> SpaceMember {
        SpaceMember {
            device_id: DeviceId::new(id),
            device_name: format!("device-{id}"),
            identity_fingerprint: format!("fp-{id}"),
            joined_at: Utc::now(),
            sync_preferences: MemberSyncPreferences::default(),
        }
    }

    #[tokio::test]
    async fn save_then_get_roundtrip() {
        let (repo, _tempdir) = make_repo();
        let member = fixture_member("dev-a");
        repo.save(&member).await.unwrap();

        let loaded = repo.get(&member.device_id).await.unwrap().unwrap();
        assert_eq!(loaded.device_id, member.device_id);
        assert_eq!(loaded.device_name, member.device_name);
        assert_eq!(loaded.identity_fingerprint, member.identity_fingerprint);
        assert_eq!(loaded.sync_preferences, member.sync_preferences);
    }

    #[tokio::test]
    async fn get_missing_returns_none() {
        let (repo, _tempdir) = make_repo();
        let result = repo.get(&DeviceId::new("missing")).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn save_is_upsert() {
        let (repo, _tempdir) = make_repo();
        let mut member = fixture_member("dev-b");
        repo.save(&member).await.unwrap();

        member.device_name = "renamed".to_string();
        repo.save(&member).await.unwrap();

        let loaded = repo.get(&member.device_id).await.unwrap().unwrap();
        assert_eq!(loaded.device_name, "renamed");
    }

    #[tokio::test]
    async fn list_returns_all_saved() {
        let (repo, _tempdir) = make_repo();
        repo.save(&fixture_member("a")).await.unwrap();
        repo.save(&fixture_member("b")).await.unwrap();
        repo.save(&fixture_member("c")).await.unwrap();

        let mut members = repo.list().await.unwrap();
        members.sort_by(|x, y| x.device_id.as_str().cmp(y.device_id.as_str()));
        assert_eq!(members.len(), 3);
        assert_eq!(members[0].device_id.as_str(), "a");
        assert_eq!(members[2].device_id.as_str(), "c");
    }

    #[tokio::test]
    async fn remove_returns_true_when_present_false_when_absent() {
        let (repo, _tempdir) = make_repo();
        let member = fixture_member("dev-c");
        repo.save(&member).await.unwrap();

        let first = repo.remove(&member.device_id).await.unwrap();
        let second = repo.remove(&member.device_id).await.unwrap();
        assert!(first);
        assert!(!second);
        assert!(repo.get(&member.device_id).await.unwrap().is_none());
    }

    /// Verifies the `up.sql` data migration maps old paired_device rows into
    /// space_member rows with MemberSyncPreferences derived as documented.
    #[tokio::test]
    async fn migration_copies_trusted_paired_devices_with_default_preferences() {
        use diesel::connection::SimpleConnection;

        let (repo, _tempdir) = make_repo();

        // Seed paired_device rows after migrations have already run. One with
        // NULL sync_settings (should map to defaults), one with custom settings
        // (auto_sync=false, text=true, image=false), one Pending (ignored).
        repo.executor
            .run(|conn| {
                conn.batch_execute(
                    r#"
                    INSERT INTO paired_device
                        (peer_id, pairing_state, identity_fingerprint, paired_at, last_seen_at, device_name, sync_settings)
                    VALUES
                        ('peer-default', 'Trusted', 'fp-default', 1000, NULL, 'Default Device', NULL),
                        ('peer-custom',  'Trusted', 'fp-custom',  2000, NULL, 'Custom Device',
                            '{"auto_sync":false,"sync_frequency":"realtime","content_types":{"text":true,"image":false,"link":true,"file":false,"code_snippet":true,"rich_text":false}}'),
                        ('peer-pending', 'Pending', 'fp-pending', 3000, NULL, 'Pending Device', NULL);
                    "#,
                )?;
                Ok(())
            })
            .unwrap();

        // Rerun the data-migration INSERT by hand — `init_db_pool` has already
        // applied the CREATE TABLE portion against an empty paired_device, so
        // we invoke only the SELECT → INSERT step to verify the JSON mapping.
        repo.executor
            .run(|conn| {
                conn.batch_execute(
                    r#"
                    INSERT INTO space_member (device_id, device_name, identity_fingerprint, joined_at, sync_preferences)
                    SELECT
                        peer_id,
                        device_name,
                        identity_fingerprint,
                        paired_at,
                        CASE
                            WHEN sync_settings IS NULL THEN
                                json_object(
                                    'send_enabled', json('true'),
                                    'receive_enabled', json('true'),
                                    'send_content_types', json_object(
                                        'text', json('true'), 'image', json('true'), 'link', json('true'),
                                        'file', json('true'), 'code_snippet', json('true'), 'rich_text', json('true')
                                    ),
                                    'receive_content_types', json_object(
                                        'text', json('true'), 'image', json('true'), 'link', json('true'),
                                        'file', json('true'), 'code_snippet', json('true'), 'rich_text', json('true')
                                    )
                                )
                            ELSE
                                json_object(
                                    'send_enabled',
                                        CASE json_extract(sync_settings, '$.auto_sync') WHEN 0 THEN json('false') ELSE json('true') END,
                                    'receive_enabled',
                                        CASE json_extract(sync_settings, '$.auto_sync') WHEN 0 THEN json('false') ELSE json('true') END,
                                    'send_content_types', json_object(
                                        'text',         CASE json_extract(sync_settings, '$.content_types.text')         WHEN 0 THEN json('false') ELSE json('true') END,
                                        'image',        CASE json_extract(sync_settings, '$.content_types.image')        WHEN 0 THEN json('false') ELSE json('true') END,
                                        'link',         CASE json_extract(sync_settings, '$.content_types.link')         WHEN 0 THEN json('false') ELSE json('true') END,
                                        'file',         CASE json_extract(sync_settings, '$.content_types.file')         WHEN 0 THEN json('false') ELSE json('true') END,
                                        'code_snippet', CASE json_extract(sync_settings, '$.content_types.code_snippet') WHEN 0 THEN json('false') ELSE json('true') END,
                                        'rich_text',    CASE json_extract(sync_settings, '$.content_types.rich_text')    WHEN 0 THEN json('false') ELSE json('true') END
                                    ),
                                    'receive_content_types', json_object(
                                        'text',         CASE json_extract(sync_settings, '$.content_types.text')         WHEN 0 THEN json('false') ELSE json('true') END,
                                        'image',        CASE json_extract(sync_settings, '$.content_types.image')        WHEN 0 THEN json('false') ELSE json('true') END,
                                        'link',         CASE json_extract(sync_settings, '$.content_types.link')         WHEN 0 THEN json('false') ELSE json('true') END,
                                        'file',         CASE json_extract(sync_settings, '$.content_types.file')         WHEN 0 THEN json('false') ELSE json('true') END,
                                        'code_snippet', CASE json_extract(sync_settings, '$.content_types.code_snippet') WHEN 0 THEN json('false') ELSE json('true') END,
                                        'rich_text',    CASE json_extract(sync_settings, '$.content_types.rich_text')    WHEN 0 THEN json('false') ELSE json('true') END
                                    )
                                )
                        END
                    FROM paired_device
                    WHERE pairing_state = 'Trusted';
                    "#,
                )?;
                Ok(())
            })
            .unwrap();

        let mut members = repo.list().await.unwrap();
        members.sort_by(|x, y| x.device_id.as_str().cmp(y.device_id.as_str()));

        // Only the two Trusted rows should have been copied; Pending is skipped.
        assert_eq!(members.len(), 2);

        let default_row = &members[1]; // peer-default
        assert_eq!(default_row.device_id.as_str(), "peer-default");
        assert_eq!(
            default_row.sync_preferences,
            MemberSyncPreferences::default()
        );

        let custom_row = &members[0]; // peer-custom
        assert_eq!(custom_row.device_id.as_str(), "peer-custom");
        assert!(!custom_row.sync_preferences.send_enabled);
        assert!(!custom_row.sync_preferences.receive_enabled);
        assert!(custom_row.sync_preferences.send_content_types.text);
        assert!(!custom_row.sync_preferences.send_content_types.image);
        assert!(custom_row.sync_preferences.send_content_types.link);
        assert!(!custom_row.sync_preferences.send_content_types.file);
        assert!(custom_row.sync_preferences.send_content_types.code_snippet);
        assert!(!custom_row.sync_preferences.send_content_types.rich_text);
        // receive should mirror send
        assert_eq!(
            custom_row.sync_preferences.send_content_types,
            custom_row.sync_preferences.receive_content_types
        );
    }
}
