use crate::{
    error::{CommandError, CommandResult},
    models::{ProjectBackup, Workspace},
    workspace::workspace_path,
    AppState,
};
use chrono::Utc;
use rusqlite::{params, Transaction};
use std::path::{Path, PathBuf};
use tauri::State;
use uuid::Uuid;

use super::archive::{
    copy_tree, create_project_backup_internal, workspace_record, BACKUP_FORMAT_VERSION,
};

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
        ("conversation_log_entries", "conversation_id IN (SELECT id FROM project_backup.conversations WHERE workspace_id=?1)"),
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
        let exists: i64 = transaction.query_row(
            "SELECT COUNT(*) FROM project_backup.sqlite_master WHERE type='table' AND name=?1",
            [table],
            |row| row.get(0),
        )?;
        if exists == 0 {
            continue;
        }
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

pub(super) fn restore_project_backup_internal(
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
