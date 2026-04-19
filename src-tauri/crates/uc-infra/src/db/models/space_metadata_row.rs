use crate::db::schema::space_metadata;
use diesel::prelude::*;

#[derive(Debug, Queryable)]
#[diesel(table_name = space_metadata)]
pub struct SpaceMetadataRow {
    pub space_id: String,
    pub payload: Vec<u8>,
    pub updated_at_ms: i64,
}

#[derive(Debug, Insertable, AsChangeset)]
#[diesel(table_name = space_metadata)]
pub struct NewSpaceMetadataRow {
    pub space_id: String,
    pub payload: Vec<u8>,
    pub updated_at_ms: i64,
}
