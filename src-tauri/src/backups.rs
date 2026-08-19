use crate::{
    error::{CommandError, CommandResult},
    models::{ProjectBackup, Workspace},
    workspace::workspace_path,
    AppState,
};
use chrono::Utc;
use rusqlite::{backup::Backup, params, Connection, OptionalExtension, Transaction};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use tauri::State;
use uuid::Uuid;

const BACKUP_FORMAT_VERSION: u32 = 1;

fn safe_slug(value: &str) -> String {
    let slug: String = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let slug = slug.trim_matches('-');
    if slug.is_empty() {
        "sprite-project".into()
    } else {
        slug.into()
    }
}

fn copy_tree(source: &Path, destination: &Path) -> CommandResult<(u64, u64)> {
    std::fs::create_dir_all(destination)?;
    let mut file_count = 0;
    let mut total_bytes = 0;
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        let metadata = std::fs::symlink_metadata(&source_path)?;
        if metadata.file_type().is_symlink() {
            return Err(CommandError::new(
                "backup_symlink",
                format!(
                    "Backups do not follow symbolic links: {}",
                    source_path.display()
                ),
            ));
        }
        if metadata.is_dir() {
            let (nested_count, nested_bytes) = copy_tree(&source_path, &destination_path)?;
            file_count += nested_count;
            total_bytes += nested_bytes;
        } else if metadata.is_file() {
            std::fs::copy(&source_path, &destination_path)?;
            file_count += 1;
            total_bytes += metadata.len();
        }
    }
    Ok((file_count, total_bytes))
}

fn workspace_record(state: &AppState, project_id: &str) -> CommandResult<Workspace> {
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    connection
        .query_row(
            "SELECT id,name,path,created_at,last_opened_at FROM projects WHERE id=?1",
            [project_id],
            |row| {
                Ok(Workspace {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    path: row.get(2)?,
                    created_at: row.get(3)?,
                    last_opened_at: row.get(4)?,
                })
            },
        )
        .optional()?
        .ok_or_else(|| {
            CommandError::new("workspace_not_found", "Workspace is no longer registered")
        })
}

fn create_database_snapshot(
    state: &AppState,
    destination: &Path,
    project_id: &str,
) -> CommandResult<()> {
    let source = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    let mut target = Connection::open(destination)?;
    let backup = Backup::new(&source, &mut target)?;
    backup.run_to_completion(32, Duration::from_millis(5), None)?;
    drop(backup);
    target.execute_batch("PRAGMA foreign_keys=ON; PRAGMA secure_delete=ON;")?;
    target.execute("DELETE FROM projects WHERE id<>?1", [project_id])?;
    target.execute("DELETE FROM provider_settings", [])?;
    target.execute(
        "DELETE FROM settings
         WHERE key NOT IN ('workspace-style:'||?1,'active-worktree:'||?1)
           AND NOT EXISTS (
             SELECT 1 FROM conversations c
             WHERE c.workspace_id=?1 AND settings.key LIKE '%:'||c.id
           )",
        [project_id],
    )?;
    target.execute_batch("VACUUM")?;
    Ok(())
}

fn create_project_backup_internal(
    project_id: &str,
    destination_directory: &Path,
    state: &AppState,
) -> CommandResult<ProjectBackup> {
    let workspace = workspace_record(state, project_id)?;
    let root = workspace_path(state, project_id)?;
    let busy: bool = state.db.lock().map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?.query_row(
        "SELECT EXISTS(
           SELECT 1 FROM conversations c JOIN messages m ON m.conversation_id=c.id
           WHERE c.workspace_id=?1 AND m.status='running'
           UNION ALL
           SELECT 1 FROM background_jobs WHERE project_id=?1 AND status IN ('queued','running','analyzing')
         )",
        [project_id],
        |row| row.get(0),
    )?;
    if busy {
        return Err(CommandError::new(
            "project_busy",
            "Wait for active generation and background jobs to finish before creating a backup",
        ));
    }
    std::fs::create_dir_all(destination_directory)?;
    let destination_directory = destination_directory.canonicalize()?;
    if destination_directory.starts_with(&root) {
        return Err(CommandError::new(
            "unsafe_backup_destination",
            "Choose a backup destination outside the project folder",
        ));
    }
    let timestamp = Utc::now();
    let label = format!(
        "{}-{}.sprite-studio-backup",
        safe_slug(&workspace.name),
        timestamp.format("%Y%m%d-%H%M%S")
    );
    let mut final_path = destination_directory.join(&label);
    if final_path.exists() {
        let suffix = Uuid::new_v4().simple().to_string();
        final_path = destination_directory.join(format!(
            "{}-{}.sprite-studio-backup",
            label.trim_end_matches(".sprite-studio-backup"),
            &suffix[..8]
        ));
    }
    let staging = destination_directory.join(format!(
        ".sprite-studio-backup-{}.incomplete",
        Uuid::new_v4()
    ));
    std::fs::create_dir_all(&staging)?;
    let result = (|| -> CommandResult<ProjectBackup> {
        let (file_count, total_bytes) = copy_tree(&root, &staging.join("workspace"))?;
        create_database_snapshot(state, &staging.join("metadata.sqlite3"), project_id)?;
        let backup = ProjectBackup {
            format_version: BACKUP_FORMAT_VERSION,
            project_id: workspace.id,
            project_name: workspace.name,
            source_path: workspace.path,
            backup_path: final_path.to_string_lossy().into_owned(),
            created_at: timestamp.to_rfc3339(),
            file_count,
            total_bytes,
        };
        std::fs::write(
            staging.join("manifest.json"),
            serde_json::to_vec_pretty(&backup)
                .map_err(|error| CommandError::new("serialization_error", error.to_string()))?,
        )?;
        std::fs::rename(&staging, &final_path)?;
        Ok(backup)
    })();
    if result.is_err() && staging.exists() {
        let _ = std::fs::remove_dir_all(&staging);
    }
    result
}

#[tauri::command]
pub fn create_project_backup(
    project_id: String,
    destination_directory: String,
    state: State<'_, AppState>,
) -> CommandResult<ProjectBackup> {
    create_project_backup_internal(&project_id, Path::new(&destination_directory), &state)
}

fn read_manifest(backup_path: &Path) -> CommandResult<ProjectBackup> {
    let manifest_path = backup_path.join("manifest.json");
    let manifest: ProjectBackup = serde_json::from_slice(&std::fs::read(manifest_path)?)
        .map_err(|error| CommandError::new("invalid_backup", error.to_string()))?;
    if manifest.format_version != BACKUP_FORMAT_VERSION
        || !backup_path.join("workspace").is_dir()
        || !backup_path.join("metadata.sqlite3").is_file()
    {
        return Err(CommandError::new(
            "invalid_backup",
            "This is not a complete Sprite Studio project backup",
        ));
    }
    Ok(manifest)
}

fn copy_project_rows(
    transaction: &Transaction<'_>,
    project_id: &str,
    old_root: &str,
    new_root: &str,
) -> CommandResult<()> {
    transaction.execute(
        "INSERT INTO projects(id,name,path,created_at,last_opened_at) SELECT id,name,?2,created_at,?3 FROM project_backup.projects WHERE id=?1",
        params![project_id, new_root, Utc::now().to_rfc3339()],
    )?;
    for (table, predicate) in [
        ("worktrees", "project_id=?1"),
        ("conversations", "workspace_id=?1"),
        ("messages", "conversation_id IN (SELECT id FROM project_backup.conversations WHERE workspace_id=?1)"),
        ("generations", "workspace_id=?1"),
        ("assets", "workspace_id=?1"),
        ("asset_worktrees", "asset_id IN (SELECT id FROM project_backup.assets WHERE workspace_id=?1)"),
        ("asset_versions", "asset_id IN (SELECT id FROM project_backup.assets WHERE workspace_id=?1) ORDER BY version_number"),
        ("rigs", "workspace_id=?1"),
        ("animations", "workspace_id=?1"),
        ("animation_frames", "animation_id IN (SELECT id FROM project_backup.animations WHERE workspace_id=?1)"),
        ("reference_images", "project_id=?1"),
        ("conversation_references", "conversation_id IN (SELECT id FROM project_backup.conversations WHERE workspace_id=?1)"),
        ("generation_references", "generation_id IN (SELECT id FROM project_backup.generations WHERE workspace_id=?1)"),
        ("animation_templates", "project_id=?1"),
        ("animation_template_phases", "template_id IN (SELECT id FROM project_backup.animation_templates WHERE project_id=?1)"),
        ("animation_template_references", "template_id IN (SELECT id FROM project_backup.animation_templates WHERE project_id=?1)"),
        ("animation_plans", "animation_id IN (SELECT id FROM project_backup.animations WHERE workspace_id=?1)"),
        ("animation_plan_phases", "plan_id IN (SELECT id FROM project_backup.animation_plans WHERE animation_id IN (SELECT id FROM project_backup.animations WHERE workspace_id=?1))"),
        ("background_jobs", "project_id=?1"),
        ("sprite_sheets", "project_id=?1"),
        ("sprite_sheet_items", "sprite_sheet_id IN (SELECT id FROM project_backup.sprite_sheets WHERE project_id=?1)"),
        ("vfx_effects", "project_id=?1"),
        ("quality_reports", "project_id=?1"),
        ("quality_checks", "report_id IN (SELECT id FROM project_backup.quality_reports WHERE project_id=?1)"),
        ("quality_warnings", "report_id IN (SELECT id FROM project_backup.quality_reports WHERE project_id=?1)"),
        ("frame_quality_cache", "asset_id IN (SELECT id FROM project_backup.assets WHERE workspace_id=?1)"),
        ("animation_revisions", "animation_id IN (SELECT id FROM project_backup.animations WHERE workspace_id=?1)"),
    ] {
        transaction.execute(
            &format!("INSERT INTO {table} SELECT * FROM project_backup.{table} WHERE {predicate}"),
            [project_id],
        )?;
    }
    transaction.execute(
        "INSERT OR REPLACE INTO settings SELECT * FROM project_backup.settings s WHERE s.key IN ('workspace-style:'||?1,'active-worktree:'||?1) OR EXISTS (SELECT 1 FROM project_backup.conversations c WHERE c.workspace_id=?1 AND s.key LIKE '%:'||c.id)",
        [project_id],
    )?;
    for (table, column) in [
        ("assets", "path"),
        ("asset_versions", "path"),
        ("reference_images", "path"),
        ("background_jobs", "result_path"),
        ("sprite_sheets", "png_path"),
        ("sprite_sheets", "metadata_path"),
    ] {
        transaction.execute(
            &format!("UPDATE {table} SET {column}=?2||substr({column},length(?1)+1) WHERE {column} IS NOT NULL AND {column} LIKE ?1||'%'"),
            params![old_root, new_root],
        )?;
    }
    let inserted: bool = transaction.query_row(
        "SELECT EXISTS(SELECT 1 FROM projects WHERE id=?1 AND path=?2)",
        params![project_id, new_root],
        |row| row.get(0),
    )?;
    if !inserted {
        return Err(CommandError::new(
            "restore_failed",
            "Restored project metadata could not be verified",
        ));
    }
    Ok(())
}

#[tauri::command]
pub fn restore_project_backup(
    project_id: String,
    backup_path: String,
    state: State<'_, AppState>,
) -> CommandResult<Workspace> {
    restore_project_backup_internal(&project_id, Path::new(&backup_path), &state)
}

#[tauri::command]
pub fn import_project_backup(
    backup_path: String,
    destination_path: String,
    state: State<'_, AppState>,
) -> CommandResult<Workspace> {
    let backup_path = PathBuf::from(backup_path).canonicalize()?;
    let manifest = read_manifest(&backup_path)?;
    let existing: bool = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM projects WHERE id=?1)",
            [&manifest.project_id],
            |row| row.get(0),
        )?;
    if existing {
        return Err(CommandError::new(
            "backup_already_registered",
            "This project is already registered. Open it and use Workspace > Restore instead.",
        ));
    }
    let destination = PathBuf::from(destination_path);
    std::fs::create_dir_all(&destination)?;
    let destination = destination.canonicalize()?;
    if destination.parent().is_none() || destination.components().count() < 3 {
        return Err(CommandError::new(
            "unsafe_restore",
            "Choose a dedicated project folder",
        ));
    }
    if std::fs::read_dir(&destination)?.next().is_some() {
        return Err(CommandError::new(
            "restore_destination_not_empty",
            "Choose an empty folder for the restored project",
        ));
    }
    if destination.starts_with(&backup_path) || backup_path.starts_with(&destination) {
        return Err(CommandError::new(
            "unsafe_restore",
            "The restored project and its backup must be separate folders",
        ));
    }
    copy_tree(&backup_path.join("workspace"), &destination)?;
    let import_result = (|| -> CommandResult<()> {
        let mut connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
        connection.execute(
            "ATTACH DATABASE ?1 AS project_backup",
            [backup_path
                .join("metadata.sqlite3")
                .to_string_lossy()
                .as_ref()],
        )?;
        let insert_result = (|| -> CommandResult<()> {
            let transaction = connection.transaction()?;
            copy_project_rows(
                &transaction,
                &manifest.project_id,
                &manifest.source_path,
                destination.to_string_lossy().as_ref(),
            )?;
            transaction.commit()?;
            Ok(())
        })();
        let _ = connection.execute_batch("DETACH DATABASE project_backup");
        insert_result
    })();
    if let Err(error) = import_result {
        let _ = std::fs::remove_dir_all(&destination);
        return Err(error);
    }
    workspace_record(&state, &manifest.project_id)
}

fn restore_project_backup_internal(
    project_id: &str,
    backup_path: &Path,
    state: &AppState,
) -> CommandResult<Workspace> {
    let root = workspace_path(state, project_id)?;
    if root.parent().is_none() || root.components().count() < 3 {
        return Err(CommandError::new(
            "unsafe_restore",
            "Refusing to replace a broad filesystem path",
        ));
    }
    let backup_path = backup_path.canonicalize()?;
    let manifest = read_manifest(&backup_path)?;
    if manifest.project_id != project_id {
        return Err(CommandError::new(
            "wrong_project_backup",
            "Choose a backup created from this project",
        ));
    }
    let running: bool = state.db.lock().map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?.query_row(
        "SELECT EXISTS(SELECT 1 FROM conversations c JOIN messages m ON m.conversation_id=c.id WHERE c.workspace_id=?1 AND m.status='running')",
        [&project_id],
        |row| row.get(0),
    )?;
    if running {
        return Err(CommandError::new(
            "project_busy",
            "Stop active chat generation before restoring a backup",
        ));
    }
    let safety_directory = root
        .parent()
        .expect("validated parent")
        .join(".sprite-studio-safety-backups");
    create_project_backup_internal(project_id, &safety_directory, state)?;
    let staging = root
        .parent()
        .expect("validated parent")
        .join(format!(".sprite-studio-restore-{}", Uuid::new_v4()));
    let previous = root
        .parent()
        .expect("validated parent")
        .join(format!(".sprite-studio-previous-{}", Uuid::new_v4()));
    copy_tree(&backup_path.join("workspace"), &staging)?;
    std::fs::rename(&root, &previous)?;
    if let Err(error) = std::fs::rename(&staging, &root) {
        let _ = std::fs::rename(&previous, &root);
        return Err(error.into());
    }
    let restore_result = (|| -> CommandResult<()> {
        let mut connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
        connection.execute(
            "ATTACH DATABASE ?1 AS project_backup",
            [backup_path
                .join("metadata.sqlite3")
                .to_string_lossy()
                .as_ref()],
        )?;
        let import_result = (|| -> CommandResult<()> {
            let transaction = connection.transaction()?;
            transaction.execute("DELETE FROM projects WHERE id=?1", [project_id])?;
            copy_project_rows(
                &transaction,
                project_id,
                &manifest.source_path,
                root.to_string_lossy().as_ref(),
            )?;
            transaction.commit()?;
            Ok(())
        })();
        let _ = connection.execute_batch("DETACH DATABASE project_backup");
        import_result
    })();
    if let Err(error) = restore_result {
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::rename(&previous, &root);
        return Err(error);
    }
    std::fs::remove_dir_all(previous)?;
    workspace_record(state, project_id)
}

#[cfg(test)]
mod tests {
    use super::{create_project_backup_internal, restore_project_backup_internal, safe_slug};
    use crate::{database, AppState};
    use rusqlite::Connection;
    use std::{collections::HashMap, sync::Mutex};
    use uuid::Uuid;

    #[test]
    fn backup_names_are_portable() {
        assert_eq!(safe_slug("Pirate Train!"), "pirate-train");
        assert_eq!(safe_slug("***"), "sprite-project");
    }

    #[test]
    fn project_backup_restores_files_and_metadata_after_making_a_safety_copy() {
        let root =
            std::env::temp_dir().join(format!("sprite-studio-backup-test-{}", Uuid::new_v4()));
        let project = root.join("project");
        let backups = root.join("backups");
        std::fs::create_dir_all(project.join("assets/characters")).expect("project folders");
        let connection = database::open(&root.join("app.sqlite3")).expect("database opens");
        connection.execute(
            "INSERT INTO projects(id,name,path,created_at,last_opened_at) VALUES ('project','Test',?1,'created','opened')",
            [project.to_string_lossy().as_ref()],
        ).expect("project inserts");
        connection.execute(
            "INSERT INTO projects(id,name,path,created_at,last_opened_at) VALUES ('private-project','Private',?1,'created','opened')",
            [root.join("private-project").to_string_lossy().as_ref()],
        ).expect("other project inserts");
        connection.execute(
            "INSERT INTO provider_settings(provider,settings_json,updated_at) VALUES ('private-provider','{\"apiKey\":\"secret\"}','now')",
            [],
        ).expect("provider credentials insert");
        connection.execute(
            "INSERT INTO settings(key,value_json,updated_at) VALUES ('theme','\"dark\"','now'),('workspace-style:project','\"pixel-rpg\"','now')",
            [],
        ).expect("settings insert");
        connection.execute(
            "INSERT INTO worktrees(id,project_id,name,slug,kind,created_at,updated_at) VALUES ('general','project','General','general','general','created','created')",
            [],
        ).expect("worktree inserts");
        connection.execute(
            "INSERT INTO conversations(id,workspace_id,worktree_id,title,provider,created_at,updated_at) VALUES ('chat','project','general','Before backup','codex','created','created')",
            [],
        ).expect("chat inserts");
        connection.execute(
            "INSERT INTO rigs(id,workspace_id,worktree_id,name,spec_json,created_at,updated_at) VALUES ('rig','project','general','Before backup rig','{}','created','created')",
            [],
        ).expect("rig inserts");
        std::fs::write(project.join("assets/characters/hero.txt"), "before")
            .expect("fixture writes");
        let state = AppState {
            db: Mutex::new(connection),
            cancellers: Mutex::new(HashMap::new()),
        };

        let backup =
            create_project_backup_internal("project", &backups, &state).expect("backup creates");
        let backup_database =
            Connection::open(std::path::Path::new(&backup.backup_path).join("metadata.sqlite3"))
                .expect("backup database opens");
        assert_eq!(
            backup_database
                .query_row("SELECT COUNT(*) FROM projects", [], |row| row
                    .get::<_, i64>(0),)
                .expect("project count reads"),
            1
        );
        assert_eq!(
            backup_database
                .query_row("SELECT COUNT(*) FROM provider_settings", [], |row| row
                    .get::<_, i64>(0),)
                .expect("provider setting count reads"),
            0
        );
        assert_eq!(
            backup_database
                .query_row(
                    "SELECT COUNT(*) FROM settings WHERE key='theme'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .expect("global setting count reads"),
            0
        );
        drop(backup_database);
        std::fs::write(project.join("assets/characters/hero.txt"), "after")
            .expect("project mutates");
        state
            .db
            .lock()
            .expect("database lock")
            .execute(
                "UPDATE conversations SET title='After backup' WHERE id='chat'",
                [],
            )
            .expect("chat mutates");
        state
            .db
            .lock()
            .expect("database lock")
            .execute("UPDATE rigs SET name='After backup rig' WHERE id='rig'", [])
            .expect("rig mutates");

        restore_project_backup_internal(
            "project",
            std::path::Path::new(&backup.backup_path),
            &state,
        )
        .expect("backup restores");

        assert_eq!(
            std::fs::read_to_string(project.join("assets/characters/hero.txt"))
                .expect("file reads"),
            "before"
        );
        let title: String = state
            .db
            .lock()
            .expect("database lock")
            .query_row(
                "SELECT title FROM conversations WHERE id='chat'",
                [],
                |row| row.get(0),
            )
            .expect("chat reads");
        assert_eq!(title, "Before backup");
        let (rig_name, rig_worktree): (String, String) = state
            .db
            .lock()
            .expect("database lock")
            .query_row(
                "SELECT name,worktree_id FROM rigs WHERE id='rig'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("rig reads");
        assert_eq!(rig_name, "Before backup rig");
        assert_eq!(rig_worktree, "general");
        assert!(root.join(".sprite-studio-safety-backups").is_dir());
        drop(state);
        std::fs::remove_dir_all(root).expect("fixture removes");
    }
}
