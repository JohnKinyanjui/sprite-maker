use crate::{
    error::{CommandError, CommandResult},
    models::ReferenceImage,
    AppState,
};
use chrono::Utc;
use image::GenericImageView;
use rusqlite::{params, OptionalExtension};
use std::path::{Path, PathBuf};
use tauri::{Manager, State};
use uuid::Uuid;

const REFERENCE_CATEGORIES: &[&str] = &[
    "character_appearance",
    "clothing",
    "face",
    "weapon",
    "pose",
    "art_style",
    "environment",
    "palette",
    "animation",
    "vfx",
    "anatomy",
    "lighting",
    "other",
];

fn reference_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ReferenceImage> {
    Ok(ReferenceImage {
        id: row.get(0)?,
        project_id: row.get(1)?,
        worktree_id: row.get(2)?,
        name: row.get(3)?,
        path: row.get(4)?,
        relative_path: row.get(5)?,
        category: row.get(6)?,
        notes: row.get(7)?,
        format: row.get(8)?,
        width: row.get(9)?,
        height: row.get(10)?,
        file_size: row.get(11)?,
        content_hash: row.get(12)?,
        created_at: row.get(13)?,
        updated_at: row.get(14)?,
    })
}

struct PromptReference {
    name: String,
    path: String,
    category: String,
    notes: Option<String>,
    width: u32,
    height: u32,
    format: String,
    content_hash: String,
}

fn prompt_reference_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<PromptReference> {
    Ok(PromptReference {
        name: row.get(0)?,
        path: row.get(1)?,
        category: row.get(2)?,
        notes: row.get(3)?,
        width: row.get(4)?,
        height: row.get(5)?,
        format: row.get(6)?,
        content_hash: row.get(7)?,
    })
}

fn select_reference() -> &'static str {
    "SELECT id, project_id, worktree_id, name, path, relative_path, category, notes, format, width, height, file_size, content_hash, created_at, updated_at FROM reference_images"
}

fn validate_category(category: &str) -> CommandResult<()> {
    if REFERENCE_CATEGORIES.contains(&category) {
        Ok(())
    } else {
        Err(CommandError::new(
            "invalid_reference_category",
            "Choose a supported reference category",
        ))
    }
}

fn portable_file_name(path: &Path, extension: &str) -> String {
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("reference");
    let slug: String = stem
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
    format!(
        "{}-{}.{}",
        &Uuid::new_v4().simple().to_string()[..8],
        if slug.is_empty() { "reference" } else { slug },
        extension
    )
}

fn file_hash(path: &Path) -> CommandResult<String> {
    let bytes = std::fs::read(path)?;
    Ok(blake3::hash(&bytes).to_hex().to_string())
}

fn reference_extension(bytes: &[u8]) -> CommandResult<&'static str> {
    match image::guess_format(bytes)
        .map_err(|error| CommandError::new("invalid_reference_image", error.to_string()))?
    {
        image::ImageFormat::Png => Ok("png"),
        image::ImageFormat::Jpeg => Ok("jpg"),
        image::ImageFormat::WebP => Ok("webp"),
        image::ImageFormat::Gif => Ok("gif"),
        _ => Err(CommandError::new(
            "unsupported_reference_format",
            "Reference images must be PNG, JPEG, WebP, or GIF",
        )),
    }
}

fn store_reference_bytes(
    worktree_id: String,
    source_name: String,
    bytes: Vec<u8>,
    category: String,
    notes: Option<String>,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> CommandResult<ReferenceImage> {
    validate_category(&category)?;
    if bytes.is_empty() || bytes.len() > 25 * 1024 * 1024 {
        return Err(CommandError::new(
            "invalid_reference_size",
            "Reference images must be between 1 byte and 25 MB",
        ));
    }
    let image = image::load_from_memory(&bytes)
        .map_err(|error| CommandError::new("invalid_reference_image", error.to_string()))?;
    let (width, height) = image.dimensions();
    let format = reference_extension(&bytes)?.to_string();
    let source = PathBuf::from(if source_name.trim().is_empty() {
        "pasted-reference"
    } else {
        source_name.as_str()
    });
    let (project_id, project_path, worktree_slug): (String, String, String) = {
        let connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
        connection
            .query_row(
                r#"SELECT p.id, p.path, w.slug
                   FROM worktrees w JOIN projects p ON p.id = w.project_id
                   WHERE w.id = ?1"#,
                [&worktree_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?
            .ok_or_else(|| {
                CommandError::new("worktree_not_found", "The worktree no longer exists")
            })?
    };
    let project_root = PathBuf::from(&project_path);
    let reference_directory = project_root
        .join("worktrees")
        .join(worktree_slug)
        .join("references");
    std::fs::create_dir_all(&reference_directory)?;
    let destination = reference_directory.join(portable_file_name(&source, &format));
    std::fs::write(&destination, &bytes)?;
    app.asset_protocol_scope()
        .allow_file(&destination)
        .map_err(|error| CommandError::new("asset_scope_error", error.to_string()))?;
    let relative_path = destination
        .strip_prefix(&project_root)
        .unwrap_or(&destination)
        .to_string_lossy()
        .replace('\\', "/");
    let now = Utc::now().to_rfc3339();
    let reference = ReferenceImage {
        id: Uuid::new_v4().to_string(),
        project_id,
        worktree_id,
        name: source
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("Pasted reference")
            .to_string(),
        path: destination.to_string_lossy().into_owned(),
        relative_path,
        category,
        notes: notes.filter(|value| !value.trim().is_empty()),
        format,
        width,
        height,
        file_size: bytes.len() as u64,
        content_hash: file_hash(&destination)?,
        created_at: now.clone(),
        updated_at: now,
    };
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    connection.execute(
        r#"INSERT INTO reference_images(
            id, project_id, worktree_id, name, path, relative_path, category, notes,
            format, width, height, file_size, content_hash, created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)"#,
        params![
            reference.id,
            reference.project_id,
            reference.worktree_id,
            reference.name,
            reference.path,
            reference.relative_path,
            reference.category,
            reference.notes,
            reference.format,
            reference.width,
            reference.height,
            reference.file_size,
            reference.content_hash,
            reference.created_at,
            reference.updated_at
        ],
    )?;
    Ok(reference)
}

#[tauri::command]
pub fn list_reference_images(
    worktree_id: String,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> CommandResult<Vec<ReferenceImage>> {
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    let mut statement = connection.prepare(&format!(
        "{} WHERE worktree_id = ?1 ORDER BY updated_at DESC",
        select_reference()
    ))?;
    let rows = statement.query_map([worktree_id], reference_row)?;
    let references: Vec<_> = rows.filter_map(Result::ok).collect();
    for reference in &references {
        app.asset_protocol_scope()
            .allow_file(&reference.path)
            .map_err(|error| CommandError::new("asset_scope_error", error.to_string()))?;
    }
    Ok(references)
}

#[tauri::command]
pub fn import_reference_image(
    worktree_id: String,
    source_path: String,
    category: String,
    notes: Option<String>,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> CommandResult<ReferenceImage> {
    let source = PathBuf::from(&source_path);
    if !source.is_file() {
        return Err(CommandError::new(
            "reference_not_found",
            "The selected reference image no longer exists",
        ));
    }
    let bytes = std::fs::read(&source)?;
    let source_name = source
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("reference")
        .to_string();
    store_reference_bytes(worktree_id, source_name, bytes, category, notes, app, state)
}

#[tauri::command]
pub fn import_reference_bytes(
    worktree_id: String,
    file_name: String,
    bytes: Vec<u8>,
    category: String,
    notes: Option<String>,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> CommandResult<ReferenceImage> {
    store_reference_bytes(worktree_id, file_name, bytes, category, notes, app, state)
}

#[tauri::command]
pub fn update_reference_image(
    id: String,
    name: String,
    category: String,
    notes: Option<String>,
    state: State<'_, AppState>,
) -> CommandResult<ReferenceImage> {
    validate_category(&category)?;
    let name = name.trim();
    if name.is_empty() {
        return Err(CommandError::new(
            "invalid_reference_name",
            "Reference name cannot be empty",
        ));
    }
    let now = Utc::now().to_rfc3339();
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    let changed = connection.execute(
        "UPDATE reference_images SET name=?2, category=?3, notes=?4, updated_at=?5 WHERE id=?1",
        params![
            id,
            name,
            category,
            notes.filter(|value| !value.trim().is_empty()),
            now
        ],
    )?;
    if changed == 0 {
        return Err(CommandError::new(
            "reference_not_found",
            "The reference image no longer exists",
        ));
    }
    connection
        .query_row(
            &format!("{} WHERE id = ?1", select_reference()),
            [id],
            reference_row,
        )
        .map_err(Into::into)
}

#[tauri::command]
pub fn delete_reference_image(id: String, state: State<'_, AppState>) -> CommandResult<()> {
    let path: Option<String> = {
        let connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
        connection
            .query_row(
                "SELECT path FROM reference_images WHERE id=?1",
                [&id],
                |row| row.get(0),
            )
            .optional()?
    };
    let path = path.ok_or_else(|| {
        CommandError::new(
            "reference_not_found",
            "The reference image no longer exists",
        )
    })?;
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    connection.execute("DELETE FROM reference_images WHERE id=?1", [&id])?;
    let path = PathBuf::from(path);
    if path.is_file() {
        std::fs::remove_file(path)?;
    }
    Ok(())
}

#[tauri::command]
pub fn set_conversation_reference(
    conversation_id: String,
    reference_id: String,
    active: bool,
    strength: Option<f64>,
    state: State<'_, AppState>,
) -> CommandResult<()> {
    let strength = strength.unwrap_or(1.0).clamp(0.0, 2.0);
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    let compatible: bool = connection.query_row(
        r#"SELECT EXISTS(
             SELECT 1 FROM conversations c
             JOIN reference_images r ON r.project_id = c.workspace_id
             WHERE c.id=?1 AND r.id=?2
           )"#,
        params![conversation_id, reference_id],
        |row| row.get(0),
    )?;
    if !compatible {
        return Err(CommandError::new(
            "invalid_conversation_reference",
            "The reference and conversation must belong to the same project",
        ));
    }
    if active {
        connection.execute(
            r#"INSERT INTO conversation_references(conversation_id, reference_id, active, strength, created_at)
               VALUES (?1, ?2, 1, ?3, ?4)
               ON CONFLICT(conversation_id, reference_id) DO UPDATE SET active=1, strength=excluded.strength"#,
            params![conversation_id, reference_id, strength, Utc::now().to_rfc3339()],
        )?;
    } else {
        connection.execute(
            "DELETE FROM conversation_references WHERE conversation_id=?1 AND reference_id=?2",
            params![conversation_id, reference_id],
        )?;
    }
    Ok(())
}

#[tauri::command]
pub fn list_conversation_reference_ids(
    conversation_id: String,
    state: State<'_, AppState>,
) -> CommandResult<Vec<String>> {
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    let mut statement = connection.prepare(
        "SELECT reference_id FROM conversation_references WHERE conversation_id=?1 AND active=1 ORDER BY created_at",
    )?;
    let rows = statement.query_map([conversation_id], |row| row.get(0))?;
    Ok(rows.filter_map(Result::ok).collect())
}

pub fn prompt_context(
    state: &State<'_, AppState>,
    conversation_id: &str,
    reference_ids: &[String],
    maximum: usize,
) -> CommandResult<(String, Vec<String>)> {
    if reference_ids.len() > maximum {
        return Err(CommandError::new(
            "too_many_references",
            format!("The selected provider supports at most {maximum} reference images"),
        ));
    }
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    let mut lines = Vec::new();
    let mut paths = Vec::new();
    for id in reference_ids {
        let reference = connection
            .query_row(
                r#"SELECT r.name, r.path, r.category, r.notes, r.width, r.height, r.format, r.content_hash
                   FROM reference_images r
                   JOIN conversations c ON c.workspace_id = r.project_id
                   WHERE c.id=?1 AND r.id=?2"#,
                params![conversation_id, id],
                prompt_reference_row,
            )
            .optional()?
            .ok_or_else(|| {
                CommandError::new(
                    "invalid_conversation_reference",
                    "A selected reference is unavailable in this project",
                )
            })?;
        if !Path::new(&reference.path).is_file() {
            return Err(CommandError::new(
                "reference_file_missing",
                format!(
                    "The reference image {} is missing from disk",
                    reference.name
                ),
            ));
        }
        lines.push(format!(
            "- {} [{}]: {} — {}x{} {}, content hash {}{}",
            reference.name,
            reference.category,
            reference.path,
            reference.width,
            reference.height,
            reference.format,
            reference.content_hash,
            reference
                .notes
                .filter(|value| !value.trim().is_empty())
                .map(|value| format!(" — {value}"))
                .unwrap_or_default()
        ));
        paths.push(reference.path);
    }
    let text = if lines.is_empty() {
        String::new()
    } else {
        format!(
            "ACTIVE REFERENCE IMAGES (ATTACHED AS REAL IMAGE INPUTS)\nBefore planning, drawing, masking, or rigging, visually inspect every attached image. Report only what is actually visible: subject/object type, facing direction, silhouette, canvas occupancy, transparent bounds, articulated parts, likely joints/pivots, contact points, overlaps, and any ambiguity. Never infer anatomy from the filename or user wording. Base masks and pivots on observed pixel boundaries, and preserve the requested identity/style/pose roles. Do not copy protected characters or brands.\n{}",
            lines.join("\n")
        )
    };
    Ok((text, paths))
}
