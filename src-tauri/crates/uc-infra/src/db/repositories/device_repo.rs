use async_trait::async_trait;
use diesel::prelude::*;

use uc_core::device::{Device, DeviceId};
use uc_core::ports::{DeviceRepositoryError, DeviceRepositoryPort};

use crate::db::models::{DeviceRow, NewDeviceRow};
use crate::db::ports::{DbExecutor, InsertMapper, RowMapper};
use crate::db::schema::t_device::dsl::*;

pub struct DieselDeviceRepository<E, M> {
    executor: E,
    mapper: M,
}

impl<E, M> DieselDeviceRepository<E, M> {
    pub fn new(executor: E, mapper: M) -> Self {
        Self { executor, mapper }
    }
}

#[async_trait]
impl<E, M> DeviceRepositoryPort for DieselDeviceRepository<E, M>
where
    E: DbExecutor,
    M: InsertMapper<Device, NewDeviceRow> + RowMapper<DeviceRow, Device> + Send + Sync,
{
    async fn find_by_id(
        &self,
        device_id: &DeviceId,
    ) -> Result<Option<Device>, DeviceRepositoryError> {
        let id_str = device_id.as_str().to_string();
        self.executor
            .run(move |conn| {
                let row = t_device
                    .filter(id.eq(&id_str))
                    .first::<DeviceRow>(conn)
                    .optional()
                    .map_err(|e| DeviceRepositoryError::Storage(e.to_string()))?;

                match row {
                    Some(r) => {
                        let device = self
                            .mapper
                            .to_domain(&r)
                            .map_err(|e| DeviceRepositoryError::Storage(e.to_string()))?;
                        Ok(Some(device))
                    }
                    None => Ok(None),
                }
            })
            .map_err(|e| DeviceRepositoryError::Storage(e.to_string()))
    }

    async fn save(&self, device: Device) -> Result<(), DeviceRepositoryError> {
        let row = self
            .mapper
            .to_row(&device)
            .map_err(|e| DeviceRepositoryError::Storage(e.to_string()))?;

        self.executor
            .run(move |conn| {
                diesel::insert_into(t_device)
                    .values(&row)
                    .on_conflict(id)
                    .do_update()
                    .set((
                        name.eq(row.name.clone()),
                        platform.eq(row.platform.clone()),
                        is_local.eq(row.is_local),
                    ))
                    .execute(conn)
                    .map_err(|e| DeviceRepositoryError::Storage(e.to_string()))?;
                Ok(())
            })
            .map_err(|e| DeviceRepositoryError::Storage(e.to_string()))
    }

    async fn delete(&self, device_id: &DeviceId) -> Result<(), DeviceRepositoryError> {
        let id_str = device_id.as_str().to_string();
        self.executor
            .run(move |conn| {
                diesel::delete(t_device.filter(id.eq(&id_str)))
                    .execute(conn)
                    .map_err(|e| DeviceRepositoryError::Storage(e.to_string()))?;
                Ok(())
            })
            .map_err(|e| DeviceRepositoryError::Storage(e.to_string()))
    }

    async fn list_all(&self) -> Result<Vec<Device>, DeviceRepositoryError> {
        self.executor
            .run(|conn| {
                let rows = t_device
                    .load::<DeviceRow>(conn)
                    .map_err(|e| anyhow::anyhow!(e.to_string()))?;

                let devices: Result<Vec<Device>, _> = rows
                    .into_iter()
                    .map(|row| self.mapper.to_domain(&row))
                    .collect();
                devices.map_err(|e| anyhow::anyhow!(e.to_string()))
            })
            .map_err(|e| DeviceRepositoryError::Storage(e.to_string()))
    }
}
