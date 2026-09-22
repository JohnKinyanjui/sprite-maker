use crate::error::CommandResult;
use rusqlite::Connection;

pub(crate) fn migrate(connection: &mut Connection) -> CommandResult<()> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS migrations (version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL);",
    )?;
    let version = connection
        .query_row("SELECT MAX(version) FROM migrations", [], |row| {
            row.get::<_, Option<i64>>(0)
        })?
        .unwrap_or(0);

    if version < 1 {
        super::schema_core::migrate_v1(connection)?;
    }

    if version < 2 {
        let transaction = connection.transaction()?;
        super::schema_core::migrate_v2(&transaction)?;
        transaction.commit()?;
    }
    if version < 3 {
        let transaction = connection.transaction()?;
        super::schema_core::migrate_v3(&transaction)?;
        transaction.commit()?;
    }
    if version < 4 {
        let transaction = connection.transaction()?;
        super::schema_studio::migrate_v4(&transaction)?;
        transaction.commit()?;
    }
    if version < 5 {
        let transaction = connection.transaction()?;
        super::schema_studio::migrate_v5(&transaction)?;
        transaction.commit()?;
    }
    if version < 6 {
        let transaction = connection.transaction()?;
        super::schema_studio::migrate_v6(&transaction)?;
        transaction.commit()?;
    }
    if version < 7 {
        let transaction = connection.transaction()?;
        super::schema_jobs::migrate_v7(&transaction)?;
        transaction.commit()?;
    }
    if version < 8 {
        let transaction = connection.transaction()?;
        super::schema_jobs::migrate_v8(&transaction)?;
        transaction.commit()?;
    }
    if version < 9 {
        let transaction = connection.transaction()?;
        super::schema_studio::migrate_v9(&transaction)?;
        transaction.commit()?;
    }
    if version < 10 {
        let transaction = connection.transaction()?;
        super::schema_studio::migrate_v10(&transaction)?;
        transaction.commit()?;
    }
    if version < 11 {
        let transaction = connection.transaction()?;
        super::schema_studio::migrate_v11(&transaction)?;
        transaction.commit()?;
    }
    if version < 12 {
        let transaction = connection.transaction()?;
        super::schema_studio::migrate_v12(&transaction)?;
        transaction.commit()?;
    }
    if version < 13 {
        let transaction = connection.transaction()?;
        super::schema_studio::migrate_v13(&transaction)?;
        transaction.commit()?;
    }
    if version < 14 {
        let transaction = connection.transaction()?;
        super::schema_studio::migrate_v14(&transaction)?;
        transaction.commit()?;
    }
    Ok(())
}
