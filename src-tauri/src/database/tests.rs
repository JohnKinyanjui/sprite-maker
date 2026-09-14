use super::*;
use uuid::Uuid;

fn test_directory() -> std::path::PathBuf {
    std::env::temp_dir().join(format!("sprite-studio-db-test-{}", Uuid::new_v4()))
}

#[test]
fn migration_creates_the_project_and_worktree_domain_schema() {
    let directory = test_directory();
    let connection = open(&directory.join("studio.sqlite3")).expect("database should open");
    for table in [
        "projects",
        "worktrees",
        "conversations",
        "messages",
        "generations",
        "assets",
        "asset_worktrees",
        "asset_versions",
        "animations",
        "animation_frames",
        "reference_images",
        "conversation_references",
        "generation_references",
        "animation_templates",
        "animation_template_phases",
        "animation_template_references",
        "animation_plans",
        "background_jobs",
        "sprite_sheets",
        "sprite_sheet_items",
        "vfx_effects",
        "quality_reports",
        "quality_checks",
        "quality_warnings",
        "frame_quality_cache",
        "animation_revisions",
        "animation_plan_phases",
        "rigs",
        "settings",
        "provider_settings",
    ] {
        let exists: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                [table],
                |row| row.get(0),
            )
            .expect("schema query should work");
        assert_eq!(exists, 1, "missing table {table}");
    }
    let migration_version: i64 = connection
        .query_row("SELECT MAX(version) FROM migrations", [], |row| row.get(0))
        .expect("migration version should be recorded");
    assert_eq!(migration_version, 13);

    let archived_column: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('conversations') WHERE name='archived_at'",
            [],
            |row| row.get(0),
        )
        .expect("archive column query should work");
    assert_eq!(archived_column, 1);
    drop(connection);
    std::fs::remove_dir_all(directory).expect("temporary database should be removable");
}

#[test]
fn sqlite_state_survives_a_restart() {
    let directory = test_directory();
    let path = directory.join("studio.sqlite3");
    {
        let connection = open(&path).expect("database should open");
        connection.execute(
                "INSERT INTO projects(id,name,path,created_at,last_opened_at) VALUES ('w1','Test','/tmp/test','now','now')", []
            ).expect("project should insert");
        connection.execute(
                "INSERT INTO conversations(id,workspace_id,title,provider,created_at,updated_at) VALUES ('c1','w1','Test','codex','now','now')", []
            ).expect("conversation should insert");
        connection.execute(
                "INSERT INTO messages(id,conversation_id,role,content,status,created_at) VALUES ('m1','c1','assistant','partial','running','now')", []
            ).expect("running message should insert");
    }
    let connection = open(&path).expect("database should reopen");
    let name: String = connection
        .query_row("SELECT name FROM projects WHERE id='w1'", [], |row| {
            row.get(0)
        })
        .expect("project should persist");
    assert_eq!(name, "Test");
    let status: String = connection
        .query_row("SELECT status FROM messages WHERE id='m1'", [], |row| {
            row.get(0)
        })
        .expect("message should persist");
    assert_eq!(status, "cancelled");
    drop(connection);
    std::fs::remove_dir_all(directory).expect("temporary database should be removable");
}

#[test]
fn shared_open_does_not_cancel_in_flight_gui_work() {
    let directory = test_directory();
    let path = directory.join("studio.sqlite3");
    {
        let connection = open(&path).expect("database should open");
        connection
                .execute(
                    "INSERT INTO projects(id,name,path,created_at,last_opened_at) VALUES ('w1','Test','/tmp/test','now','now')",
                    [],
                )
                .expect("project should insert");
        connection
                .execute(
                    "INSERT INTO conversations(id,workspace_id,title,provider,created_at,updated_at) VALUES ('c1','w1','Test','codex','now','now')",
                    [],
                )
                .expect("conversation should insert");
        connection
                .execute(
                    "INSERT INTO messages(id,conversation_id,role,content,status,created_at) VALUES ('m1','c1','assistant','partial','running','now')",
                    [],
                )
                .expect("running message should insert");
        connection
                .execute(
                    "INSERT INTO background_jobs(id,project_id,kind,status,progress,stage,created_at,updated_at) VALUES ('j1','w1','sprite_sheet','queued',0.0,'Queued','now','now')",
                    [],
                )
                .expect("queued job should insert");
    }
    let connection = open_shared(&path).expect("shared open");
    let message_status: String = connection
        .query_row("SELECT status FROM messages WHERE id='m1'", [], |row| {
            row.get(0)
        })
        .expect("message");
    assert_eq!(message_status, "running");
    let job_status: String = connection
        .query_row(
            "SELECT status FROM background_jobs WHERE id='j1'",
            [],
            |row| row.get(0),
        )
        .expect("job");
    assert_eq!(job_status, "queued");
    drop(connection);
    std::fs::remove_dir_all(directory).expect("temporary database should be removable");
}
