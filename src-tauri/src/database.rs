use crate::error::{CommandError, CommandResult};
use rusqlite::{Connection, Transaction};
use std::path::Path;

pub fn open(path: &Path) -> CommandResult<Connection> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut connection = Connection::open(path)?;
    connection.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")?;
    migrate(&mut connection)?;
    connection.execute(
        "UPDATE messages SET status = 'cancelled' WHERE status = 'running'",
        [],
    )?;
    connection.execute(
        "UPDATE background_jobs SET status='failed', stage='Interrupted', error_message='The application closed before this job finished', completed_at=datetime('now'), updated_at=datetime('now') WHERE status IN ('queued','running','analyzing')",
        [],
    )?;
    Ok(connection)
}

fn migrate(connection: &mut Connection) -> CommandResult<()> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS migrations (version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL);",
    )?;
    let version = connection
        .query_row("SELECT MAX(version) FROM migrations", [], |row| {
            row.get::<_, Option<i64>>(0)
        })?
        .unwrap_or(0);

    if version < 1 {
        let transaction = connection.transaction()?;
        transaction.execute_batch(
            r#"
        CREATE TABLE workspaces (
          id TEXT PRIMARY KEY,
          name TEXT NOT NULL,
          path TEXT NOT NULL UNIQUE,
          created_at TEXT NOT NULL,
          last_opened_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_workspaces_last_opened ON workspaces(last_opened_at DESC);

        CREATE TABLE conversations (
          id TEXT PRIMARY KEY,
          workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
          title TEXT NOT NULL,
          provider TEXT NOT NULL DEFAULT 'codex',
          provider_session_id TEXT,
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_conversations_workspace ON conversations(workspace_id, updated_at DESC);

        CREATE TABLE messages (
          id TEXT PRIMARY KEY,
          conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
          role TEXT NOT NULL,
          kind TEXT NOT NULL DEFAULT 'text',
          content TEXT NOT NULL DEFAULT '',
          status TEXT NOT NULL DEFAULT 'completed',
          metadata_json TEXT NOT NULL DEFAULT '{}',
          created_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_messages_conversation ON messages(conversation_id, created_at);

        CREATE TABLE generations (
          id TEXT PRIMARY KEY,
          workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
          conversation_id TEXT REFERENCES conversations(id) ON DELETE SET NULL,
          prompt TEXT NOT NULL,
          provider TEXT NOT NULL,
          source_asset_id TEXT,
          operation TEXT NOT NULL,
          status TEXT NOT NULL,
          created_at TEXT NOT NULL
        );

        CREATE TABLE assets (
          id TEXT PRIMARY KEY,
          workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
          name TEXT NOT NULL,
          path TEXT NOT NULL UNIQUE,
          relative_path TEXT NOT NULL,
          category TEXT NOT NULL,
          format TEXT NOT NULL,
          width INTEGER NOT NULL,
          height INTEGER NOT NULL,
          file_size INTEGER NOT NULL,
          has_alpha INTEGER NOT NULL DEFAULT 0,
          generation_id TEXT REFERENCES generations(id) ON DELETE SET NULL,
          created_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_assets_workspace ON assets(workspace_id, category, name);

        CREATE TABLE animations (
          id TEXT PRIMARY KEY,
          workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
          name TEXT NOT NULL,
          fps REAL NOT NULL,
          looping INTEGER NOT NULL DEFAULT 1,
          frames_json TEXT NOT NULL DEFAULT '[]',
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_animations_workspace ON animations(workspace_id, updated_at DESC);

        CREATE TABLE settings (
          key TEXT PRIMARY KEY,
          value_json TEXT NOT NULL,
          updated_at TEXT NOT NULL
        );

        CREATE TABLE provider_settings (
          provider TEXT PRIMARY KEY,
          enabled INTEGER NOT NULL DEFAULT 1,
          settings_json TEXT NOT NULL DEFAULT '{}',
          updated_at TEXT NOT NULL
        );

        INSERT INTO migrations(version, applied_at) VALUES (1, datetime('now'));
        "#,
        )
        .map_err(|error| CommandError::new("migration_failed", error.to_string()))?;
        transaction.commit()?;
    }

    if version < 2 {
        let transaction = connection.transaction()?;
        migrate_v2(&transaction)?;
        transaction.commit()?;
    }
    if version < 3 {
        let transaction = connection.transaction()?;
        migrate_v3(&transaction)?;
        transaction.commit()?;
    }
    if version < 4 {
        let transaction = connection.transaction()?;
        migrate_v4(&transaction)?;
        transaction.commit()?;
    }
    if version < 5 {
        let transaction = connection.transaction()?;
        migrate_v5(&transaction)?;
        transaction.commit()?;
    }
    if version < 6 {
        let transaction = connection.transaction()?;
        migrate_v6(&transaction)?;
        transaction.commit()?;
    }
    if version < 7 {
        let transaction = connection.transaction()?;
        migrate_v7(&transaction)?;
        transaction.commit()?;
    }
    if version < 8 {
        let transaction = connection.transaction()?;
        migrate_v8(&transaction)?;
        transaction.commit()?;
    }
    if version < 9 {
        let transaction = connection.transaction()?;
        migrate_v9(&transaction)?;
        transaction.commit()?;
    }
    if version < 10 {
        let transaction = connection.transaction()?;
        migrate_v10(&transaction)?;
        transaction.commit()?;
    }
    if version < 11 {
        let transaction = connection.transaction()?;
        migrate_v11(&transaction)?;
        transaction.commit()?;
    }
    if version < 12 {
        let transaction = connection.transaction()?;
        migrate_v12(&transaction)?;
        transaction.commit()?;
    }
    Ok(())
}

fn migrate_v2(transaction: &Transaction<'_>) -> CommandResult<()> {
    transaction
        .execute_batch(
            r#"
        ALTER TABLE workspaces RENAME TO projects;

        CREATE TABLE worktrees (
          id TEXT PRIMARY KEY,
          project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
          name TEXT NOT NULL,
          slug TEXT NOT NULL,
          kind TEXT NOT NULL CHECK(kind IN ('general','character','environment','creature','object','tileset','animation','vfx','ui')),
          description TEXT,
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL,
          UNIQUE(project_id, slug)
        );
        CREATE INDEX idx_worktrees_project ON worktrees(project_id, updated_at DESC);

        ALTER TABLE conversations ADD COLUMN worktree_id TEXT REFERENCES worktrees(id) ON DELETE CASCADE;
        ALTER TABLE generations ADD COLUMN worktree_id TEXT REFERENCES worktrees(id) ON DELETE SET NULL;
        ALTER TABLE animations ADD COLUMN worktree_id TEXT REFERENCES worktrees(id) ON DELETE SET NULL;

        CREATE TABLE asset_worktrees (
          asset_id TEXT NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
          worktree_id TEXT NOT NULL REFERENCES worktrees(id) ON DELETE CASCADE,
          relationship TEXT NOT NULL DEFAULT 'owned' CHECK(relationship IN ('owned','referenced')),
          created_at TEXT NOT NULL,
          PRIMARY KEY(asset_id, worktree_id)
        );
        CREATE INDEX idx_asset_worktrees_worktree ON asset_worktrees(worktree_id, relationship);

        CREATE TABLE asset_versions (
          id TEXT PRIMARY KEY,
          asset_id TEXT NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
          version_number INTEGER NOT NULL CHECK(version_number > 0),
          parent_version_id TEXT REFERENCES asset_versions(id) ON DELETE SET NULL,
          generation_id TEXT REFERENCES generations(id) ON DELETE SET NULL,
          path TEXT NOT NULL,
          format TEXT NOT NULL,
          width INTEGER NOT NULL,
          height INTEGER NOT NULL,
          file_size INTEGER NOT NULL,
          has_alpha INTEGER NOT NULL DEFAULT 0,
          content_hash TEXT NOT NULL,
          change_kind TEXT NOT NULL DEFAULT 'imported',
          available INTEGER NOT NULL DEFAULT 1,
          selected INTEGER NOT NULL DEFAULT 0,
          created_at TEXT NOT NULL,
          UNIQUE(asset_id, version_number)
        );
        CREATE INDEX idx_asset_versions_asset ON asset_versions(asset_id, version_number DESC);

        CREATE TABLE animation_frames (
          id TEXT PRIMARY KEY,
          animation_id TEXT NOT NULL REFERENCES animations(id) ON DELETE CASCADE,
          asset_id TEXT NOT NULL REFERENCES assets(id) ON DELETE RESTRICT,
          position INTEGER NOT NULL CHECK(position >= 0),
          duration_ms INTEGER CHECK(duration_ms IS NULL OR duration_ms > 0),
          offset_x INTEGER NOT NULL DEFAULT 0,
          offset_y INTEGER NOT NULL DEFAULT 0,
          pivot_x REAL,
          pivot_y REAL,
          created_at TEXT NOT NULL,
          UNIQUE(animation_id, position)
        );
        CREATE INDEX idx_animation_frames_animation ON animation_frames(animation_id, position);

        INSERT INTO worktrees(id, project_id, name, slug, kind, description, created_at, updated_at)
        SELECT lower(hex(randomblob(16))), id, 'General', 'general', 'general',
               'Migrated project-level assets and conversations', created_at, last_opened_at
        FROM projects;

        INSERT INTO asset_versions(
          id, asset_id, version_number, path, format, width, height, file_size,
          has_alpha, content_hash, change_kind, selected, created_at
        )
        SELECT lower(hex(randomblob(16))), id, 1, path, format, width, height, file_size,
               has_alpha, '', 'migrated', 1, created_at
        FROM assets;

        INSERT INTO asset_worktrees(asset_id, worktree_id, relationship, created_at)
        SELECT a.id, w.id, 'owned', a.created_at
        FROM assets a
        JOIN worktrees w ON w.project_id = a.workspace_id AND w.kind = 'general';

        INSERT INTO migrations(version, applied_at) VALUES (2, datetime('now'));
        "#,
        )
        .map_err(|error| CommandError::new("migration_failed", error.to_string()))?;

    backfill_animation_frames(transaction, false)
}

fn migrate_v3(transaction: &Transaction<'_>) -> CommandResult<()> {
    backfill_animation_frames(transaction, true)?;
    transaction
        .execute_batch(
            r#"
        UPDATE animations
        SET worktree_id = (
          SELECT aw.worktree_id
          FROM animation_frames af
          JOIN asset_worktrees aw ON aw.asset_id = af.asset_id
          JOIN worktrees w ON w.id = aw.worktree_id
          WHERE af.animation_id = animations.id AND w.kind <> 'general'
          GROUP BY aw.worktree_id
          HAVING COUNT(DISTINCT af.position) = (
            SELECT COUNT(*) FROM animation_frames all_frames
            WHERE all_frames.animation_id = animations.id
          )
          ORDER BY MIN(aw.created_at)
          LIMIT 1
        )
        WHERE worktree_id IS NULL
          AND EXISTS (
            SELECT 1 FROM animation_frames af
            WHERE af.animation_id = animations.id
          );

        INSERT INTO migrations(version, applied_at) VALUES (3, datetime('now'));
        "#,
        )
        .map_err(|error| CommandError::new("migration_failed", error.to_string()))?;
    Ok(())
}

fn backfill_animation_frames(
    transaction: &Transaction<'_>,
    only_missing: bool,
) -> CommandResult<()> {
    let query = if only_missing {
        r#"SELECT id, frames_json, created_at
           FROM animations
           WHERE frames_json <> '[]'
             AND NOT EXISTS (
               SELECT 1 FROM animation_frames
               WHERE animation_frames.animation_id = animations.id
             )"#
    } else {
        "SELECT id, frames_json, created_at FROM animations WHERE frames_json <> '[]'"
    };
    let mut statement = transaction.prepare(query)?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
        ))
    })?;
    for row in rows {
        let (animation_id, frames_json, created_at) = row?;
        let frames: Vec<crate::models::AnimationFrame> =
            serde_json::from_str(&frames_json).unwrap_or_default();
        for (position, frame) in frames.into_iter().enumerate() {
            transaction.execute(
                r#"INSERT INTO animation_frames(
                    id, animation_id, asset_id, position, duration_ms, created_at
                )
                SELECT ?1, ?2, assets.id, ?4, ?5, ?6
                FROM assets
                WHERE assets.id = ?3"#,
                rusqlite::params![
                    uuid::Uuid::new_v4().to_string(),
                    animation_id,
                    frame.asset_id,
                    position as i64,
                    frame.duration_ms,
                    created_at
                ],
            )?;
        }
    }
    Ok(())
}

fn migrate_v4(transaction: &Transaction<'_>) -> CommandResult<()> {
    transaction
        .execute_batch(
            r#"
        CREATE TABLE reference_images (
          id TEXT PRIMARY KEY,
          project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
          worktree_id TEXT NOT NULL REFERENCES worktrees(id) ON DELETE CASCADE,
          name TEXT NOT NULL,
          path TEXT NOT NULL UNIQUE,
          relative_path TEXT NOT NULL,
          category TEXT NOT NULL CHECK(category IN (
            'character_appearance','clothing','face','weapon','pose','art_style',
            'environment','palette','animation','vfx','anatomy','lighting','other'
          )),
          notes TEXT,
          format TEXT NOT NULL,
          width INTEGER NOT NULL,
          height INTEGER NOT NULL,
          file_size INTEGER NOT NULL,
          content_hash TEXT NOT NULL,
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL
        );
        CREATE INDEX idx_reference_images_worktree
          ON reference_images(worktree_id, category, updated_at DESC);

        CREATE TABLE conversation_references (
          conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
          reference_id TEXT NOT NULL REFERENCES reference_images(id) ON DELETE CASCADE,
          active INTEGER NOT NULL DEFAULT 1,
          strength REAL NOT NULL DEFAULT 1.0 CHECK(strength >= 0.0 AND strength <= 2.0),
          created_at TEXT NOT NULL,
          PRIMARY KEY(conversation_id, reference_id)
        );
        CREATE INDEX idx_conversation_references_active
          ON conversation_references(conversation_id, active);

        CREATE TABLE generation_references (
          generation_id TEXT NOT NULL REFERENCES generations(id) ON DELETE CASCADE,
          reference_id TEXT NOT NULL REFERENCES reference_images(id) ON DELETE RESTRICT,
          role TEXT NOT NULL DEFAULT 'reference',
          strength REAL NOT NULL DEFAULT 1.0,
          created_at TEXT NOT NULL,
          PRIMARY KEY(generation_id, reference_id)
        );

        INSERT INTO migrations(version, applied_at) VALUES (4, datetime('now'));
        "#,
        )
        .map_err(|error| CommandError::new("migration_failed", error.to_string()))?;
    Ok(())
}

fn migrate_v5(transaction: &Transaction<'_>) -> CommandResult<()> {
    transaction
        .execute_batch(
            r#"
        CREATE TABLE animation_templates (
          id TEXT PRIMARY KEY,
          project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
          source_animation_id TEXT REFERENCES animations(id) ON DELETE SET NULL,
          name TEXT NOT NULL,
          intent TEXT NOT NULL,
          motion_description TEXT NOT NULL,
          direction TEXT NOT NULL DEFAULT 'side',
          looping INTEGER NOT NULL DEFAULT 1,
          fps REAL NOT NULL,
          width INTEGER NOT NULL,
          height INTEGER NOT NULL,
          pivot_x REAL,
          pivot_y REAL,
          frame_mode TEXT NOT NULL CHECK(frame_mode IN ('fixed','auto')),
          preferred_frames INTEGER NOT NULL CHECK(preferred_frames > 0),
          min_frames INTEGER NOT NULL CHECK(min_frames > 0),
          max_frames INTEGER NOT NULL CHECK(max_frames >= min_frames),
          generation_prompt TEXT NOT NULL,
          negative_prompt TEXT NOT NULL DEFAULT '',
          weapon_behavior TEXT,
          provider_settings_json TEXT NOT NULL DEFAULT '{}',
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL,
          UNIQUE(project_id, name)
        );
        CREATE INDEX idx_animation_templates_project
          ON animation_templates(project_id, updated_at DESC);

        CREATE TABLE animation_template_phases (
          id TEXT PRIMARY KEY,
          template_id TEXT NOT NULL REFERENCES animation_templates(id) ON DELETE CASCADE,
          position INTEGER NOT NULL CHECK(position >= 0),
          name TEXT NOT NULL,
          description TEXT NOT NULL,
          frame_count INTEGER NOT NULL CHECK(frame_count > 0),
          timing_weight REAL NOT NULL DEFAULT 1.0,
          movement_offset_x REAL NOT NULL DEFAULT 0.0,
          movement_offset_y REAL NOT NULL DEFAULT 0.0,
          weapon_position TEXT,
          pose_reference_id TEXT REFERENCES reference_images(id) ON DELETE SET NULL,
          UNIQUE(template_id, position)
        );
        CREATE INDEX idx_animation_template_phases_template
          ON animation_template_phases(template_id, position);

        CREATE TABLE animation_template_references (
          template_id TEXT NOT NULL REFERENCES animation_templates(id) ON DELETE CASCADE,
          reference_id TEXT NOT NULL REFERENCES reference_images(id) ON DELETE RESTRICT,
          role TEXT NOT NULL DEFAULT 'pose',
          created_at TEXT NOT NULL,
          PRIMARY KEY(template_id, reference_id)
        );

        INSERT INTO migrations(version, applied_at) VALUES (5, datetime('now'));
        "#,
        )
        .map_err(|error| CommandError::new("migration_failed", error.to_string()))?;
    Ok(())
}

fn migrate_v6(transaction: &Transaction<'_>) -> CommandResult<()> {
    transaction
        .execute_batch(
            r#"
        CREATE TABLE animation_plans (
          id TEXT PRIMARY KEY,
          animation_id TEXT NOT NULL UNIQUE REFERENCES animations(id) ON DELETE CASCADE,
          frame_mode TEXT NOT NULL CHECK(frame_mode IN ('fixed','auto')),
          selected_frame_count INTEGER NOT NULL CHECK(selected_frame_count > 0),
          minimum_frame_count INTEGER NOT NULL CHECK(minimum_frame_count > 0),
          maximum_frame_count INTEGER NOT NULL CHECK(maximum_frame_count >= minimum_frame_count),
          fps INTEGER NOT NULL CHECK(fps > 0),
          looping INTEGER NOT NULL,
          allow_interpolation INTEGER NOT NULL DEFAULT 1,
          allow_auto_adjust INTEGER NOT NULL DEFAULT 0,
          explanation TEXT NOT NULL,
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL
        );
        CREATE INDEX idx_animation_plans_animation ON animation_plans(animation_id);

        CREATE TABLE animation_plan_phases (
          id TEXT PRIMARY KEY,
          plan_id TEXT NOT NULL REFERENCES animation_plans(id) ON DELETE CASCADE,
          position INTEGER NOT NULL CHECK(position >= 0),
          name TEXT NOT NULL,
          description TEXT NOT NULL,
          frame_count INTEGER NOT NULL CHECK(frame_count > 0),
          timing_weight REAL NOT NULL DEFAULT 1.0,
          UNIQUE(plan_id, position)
        );
        CREATE INDEX idx_animation_plan_phases_plan
          ON animation_plan_phases(plan_id, position);

        INSERT INTO migrations(version, applied_at) VALUES (6, datetime('now'));
        "#,
        )
        .map_err(|error| CommandError::new("migration_failed", error.to_string()))?;
    Ok(())
}

fn migrate_v7(transaction: &Transaction<'_>) -> CommandResult<()> {
    transaction
        .execute_batch(
            r#"
        CREATE TABLE background_jobs (
          id TEXT PRIMARY KEY,
          project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
          worktree_id TEXT REFERENCES worktrees(id) ON DELETE SET NULL,
          kind TEXT NOT NULL,
          target_type TEXT,
          target_id TEXT,
          status TEXT NOT NULL CHECK(status IN ('queued','running','analyzing','completed','failed','cancelled')),
          progress REAL NOT NULL DEFAULT 0.0 CHECK(progress >= 0.0 AND progress <= 1.0),
          stage TEXT NOT NULL DEFAULT 'Queued',
          error_message TEXT,
          cancel_requested INTEGER NOT NULL DEFAULT 0,
          result_path TEXT,
          metadata_json TEXT NOT NULL DEFAULT '{}',
          created_at TEXT NOT NULL,
          started_at TEXT,
          completed_at TEXT,
          updated_at TEXT NOT NULL
        );
        CREATE INDEX idx_background_jobs_project
          ON background_jobs(project_id, created_at DESC);
        CREATE INDEX idx_background_jobs_status
          ON background_jobs(status, updated_at DESC);

        CREATE TABLE sprite_sheets (
          id TEXT PRIMARY KEY,
          project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
          worktree_id TEXT REFERENCES worktrees(id) ON DELETE SET NULL,
          animation_id TEXT NOT NULL REFERENCES animations(id) ON DELETE CASCADE,
          job_id TEXT REFERENCES background_jobs(id) ON DELETE SET NULL,
          name TEXT NOT NULL,
          layout TEXT NOT NULL CHECK(layout IN ('horizontal','vertical','grid')),
          frame_width INTEGER NOT NULL CHECK(frame_width > 0),
          frame_height INTEGER NOT NULL CHECK(frame_height > 0),
          padding INTEGER NOT NULL DEFAULT 0 CHECK(padding >= 0),
          spacing INTEGER NOT NULL DEFAULT 0 CHECK(spacing >= 0),
          rows INTEGER NOT NULL CHECK(rows > 0),
          columns INTEGER NOT NULL CHECK(columns > 0),
          scale INTEGER NOT NULL DEFAULT 1 CHECK(scale BETWEEN 1 AND 8),
          transparent INTEGER NOT NULL DEFAULT 1,
          alignment TEXT NOT NULL DEFAULT 'bottom_center' CHECK(alignment IN ('top_left','center','bottom_center')),
          pivot_x REAL NOT NULL DEFAULT 0.5,
          pivot_y REAL NOT NULL DEFAULT 1.0,
          png_path TEXT NOT NULL,
          metadata_path TEXT NOT NULL,
          width INTEGER NOT NULL CHECK(width > 0),
          height INTEGER NOT NULL CHECK(height > 0),
          frame_count INTEGER NOT NULL CHECK(frame_count > 0),
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL
        );
        CREATE INDEX idx_sprite_sheets_project
          ON sprite_sheets(project_id, updated_at DESC);
        CREATE INDEX idx_sprite_sheets_animation
          ON sprite_sheets(animation_id, updated_at DESC);

        CREATE TABLE sprite_sheet_items (
          id TEXT PRIMARY KEY,
          sprite_sheet_id TEXT NOT NULL REFERENCES sprite_sheets(id) ON DELETE CASCADE,
          animation_id TEXT NOT NULL REFERENCES animations(id) ON DELETE CASCADE,
          asset_id TEXT NOT NULL REFERENCES assets(id) ON DELETE RESTRICT,
          position INTEGER NOT NULL CHECK(position >= 0),
          row_index INTEGER NOT NULL CHECK(row_index >= 0),
          column_index INTEGER NOT NULL CHECK(column_index >= 0),
          x INTEGER NOT NULL CHECK(x >= 0),
          y INTEGER NOT NULL CHECK(y >= 0),
          width INTEGER NOT NULL CHECK(width > 0),
          height INTEGER NOT NULL CHECK(height > 0),
          duration_ms INTEGER NOT NULL CHECK(duration_ms > 0),
          UNIQUE(sprite_sheet_id, position)
        );
        CREATE INDEX idx_sprite_sheet_items_sheet
          ON sprite_sheet_items(sprite_sheet_id, position);

        CREATE TABLE vfx_effects (
          id TEXT PRIMARY KEY,
          project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
          worktree_id TEXT NOT NULL REFERENCES worktrees(id) ON DELETE CASCADE,
          animation_id TEXT REFERENCES animations(id) ON DELETE SET NULL,
          name TEXT NOT NULL,
          effect_type TEXT NOT NULL,
          blend_mode TEXT NOT NULL DEFAULT 'normal' CHECK(blend_mode IN ('normal','add','screen','multiply')),
          center_x REAL NOT NULL DEFAULT 0.5,
          center_y REAL NOT NULL DEFAULT 0.5,
          opacity REAL NOT NULL DEFAULT 1.0 CHECK(opacity >= 0.0 AND opacity <= 1.0),
          looping INTEGER NOT NULL DEFAULT 0,
          fps REAL NOT NULL DEFAULT 12.0,
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL
        );
        CREATE INDEX idx_vfx_effects_worktree
          ON vfx_effects(worktree_id, updated_at DESC);

        UPDATE animation_templates
        SET preferred_frames = max(min_frames, min(preferred_frames, max_frames))
        WHERE frame_mode = 'auto';

        INSERT INTO migrations(version, applied_at) VALUES (7, datetime('now'));
        "#,
        )
        .map_err(|error| CommandError::new("migration_failed", error.to_string()))?;
    Ok(())
}

fn migrate_v8(transaction: &Transaction<'_>) -> CommandResult<()> {
    transaction
        .execute_batch(
            r#"
        CREATE TABLE quality_reports (
          id TEXT PRIMARY KEY,
          project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
          worktree_id TEXT REFERENCES worktrees(id) ON DELETE SET NULL,
          animation_id TEXT NOT NULL REFERENCES animations(id) ON DELETE CASCADE,
          job_id TEXT REFERENCES background_jobs(id) ON DELETE SET NULL,
          status TEXT NOT NULL CHECK(status IN ('running','completed','failed','cancelled')),
          overall_score REAL NOT NULL DEFAULT 0.0,
          character_consistency_score REAL NOT NULL DEFAULT 0.0,
          motion_continuity_score REAL NOT NULL DEFAULT 0.0,
          frame_alignment_score REAL NOT NULL DEFAULT 0.0,
          weapon_consistency_score REAL NOT NULL DEFAULT 0.0,
          loop_quality_score REAL NOT NULL DEFAULT 0.0,
          transparency_score REAL NOT NULL DEFAULT 0.0,
          frame_count INTEGER NOT NULL CHECK(frame_count > 0),
          analyzer_version TEXT NOT NULL,
          created_at TEXT NOT NULL,
          completed_at TEXT,
          updated_at TEXT NOT NULL
        );
        CREATE INDEX idx_quality_reports_animation
          ON quality_reports(animation_id, created_at DESC);

        CREATE TABLE quality_checks (
          id TEXT PRIMARY KEY,
          report_id TEXT NOT NULL REFERENCES quality_reports(id) ON DELETE CASCADE,
          position INTEGER NOT NULL CHECK(position >= 0),
          check_type TEXT NOT NULL,
          frame_index INTEGER CHECK(frame_index IS NULL OR frame_index >= 0),
          comparison_frame_index INTEGER CHECK(comparison_frame_index IS NULL OR comparison_frame_index >= 0),
          severity TEXT NOT NULL CHECK(severity IN ('info','warning','error')),
          score REAL NOT NULL CHECK(score >= 0.0 AND score <= 100.0),
          message TEXT NOT NULL,
          metric_value REAL,
          metric_unit TEXT,
          repair_action TEXT,
          created_at TEXT NOT NULL,
          UNIQUE(report_id, position)
        );
        CREATE INDEX idx_quality_checks_report
          ON quality_checks(report_id, position);
        CREATE INDEX idx_quality_checks_frame
          ON quality_checks(report_id, frame_index, severity);

        CREATE TABLE quality_warnings (
          id TEXT PRIMARY KEY,
          report_id TEXT NOT NULL REFERENCES quality_reports(id) ON DELETE CASCADE,
          check_id TEXT NOT NULL REFERENCES quality_checks(id) ON DELETE CASCADE,
          acknowledged INTEGER NOT NULL DEFAULT 0,
          ignored INTEGER NOT NULL DEFAULT 0,
          resolution_note TEXT,
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL,
          UNIQUE(report_id, check_id)
        );

        CREATE TABLE frame_quality_cache (
          asset_id TEXT PRIMARY KEY REFERENCES assets(id) ON DELETE CASCADE,
          content_hash TEXT NOT NULL,
          width INTEGER NOT NULL,
          height INTEGER NOT NULL,
          alpha_min_x INTEGER,
          alpha_min_y INTEGER,
          alpha_max_x INTEGER,
          alpha_max_y INTEGER,
          centroid_x REAL,
          centroid_y REAL,
          alpha_coverage REAL NOT NULL,
          opaque_edge_pixels INTEGER NOT NULL,
          perceptual_hash TEXT NOT NULL,
          palette_signature TEXT NOT NULL,
          updated_at TEXT NOT NULL
        );
        CREATE INDEX idx_frame_quality_cache_hash
          ON frame_quality_cache(content_hash);

        INSERT INTO migrations(version, applied_at) VALUES (8, datetime('now'));
        "#,
        )
        .map_err(|error| CommandError::new("migration_failed", error.to_string()))?;
    Ok(())
}

fn migrate_v9(transaction: &Transaction<'_>) -> CommandResult<()> {
    transaction
        .execute_batch(
            r#"
        CREATE TABLE animation_revisions (
          id TEXT PRIMARY KEY,
          animation_id TEXT NOT NULL UNIQUE REFERENCES animations(id) ON DELETE CASCADE,
          parent_animation_id TEXT REFERENCES animations(id) ON DELETE SET NULL,
          source_quality_report_id TEXT REFERENCES quality_reports(id) ON DELETE SET NULL,
          change_kind TEXT NOT NULL,
          summary TEXT NOT NULL DEFAULT '',
          created_at TEXT NOT NULL
        );
        CREATE INDEX idx_animation_revisions_parent
          ON animation_revisions(parent_animation_id, created_at);

        INSERT INTO animation_revisions(
          id, animation_id, parent_animation_id, source_quality_report_id,
          change_kind, summary, created_at
        )
        SELECT lower(hex(randomblob(16))), id, NULL, NULL, 'original',
               'Original animation timeline', created_at
        FROM animations;

        INSERT INTO migrations(version, applied_at) VALUES (9, datetime('now'));
        "#,
        )
        .map_err(|error| CommandError::new("migration_failed", error.to_string()))?;
    Ok(())
}

fn migrate_v10(transaction: &Transaction<'_>) -> CommandResult<()> {
    transaction
        .execute_batch(
            r#"
        UPDATE conversations
        SET worktree_id = (
          SELECT worktrees.id FROM worktrees
          WHERE worktrees.project_id = conversations.workspace_id
            AND worktrees.kind = 'general'
          ORDER BY worktrees.created_at LIMIT 1
        )
        WHERE worktree_id IS NULL;

        CREATE INDEX IF NOT EXISTS idx_conversations_worktree
          ON conversations(workspace_id, worktree_id, updated_at DESC);

        INSERT INTO migrations(version, applied_at) VALUES (10, datetime('now'));
        "#,
        )
        .map_err(|error| CommandError::new("migration_failed", error.to_string()))?;
    Ok(())
}

fn migrate_v11(transaction: &Transaction<'_>) -> CommandResult<()> {
    transaction
        .execute_batch(
            r#"
        ALTER TABLE conversations ADD COLUMN archived_at TEXT;

        CREATE INDEX IF NOT EXISTS idx_conversations_active_worktree
          ON conversations(workspace_id, worktree_id, archived_at, updated_at DESC);

        INSERT INTO migrations(version, applied_at) VALUES (11, datetime('now'));
        "#,
        )
        .map_err(|error| CommandError::new("migration_failed", error.to_string()))?;
    Ok(())
}

fn migrate_v12(transaction: &Transaction<'_>) -> CommandResult<()> {
    transaction
        .execute_batch(
            r#"
        CREATE TABLE rigs (
          id TEXT PRIMARY KEY,
          workspace_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
          worktree_id TEXT REFERENCES worktrees(id) ON DELETE SET NULL,
          asset_id TEXT REFERENCES assets(id) ON DELETE SET NULL,
          name TEXT NOT NULL,
          morphology TEXT NOT NULL DEFAULT 'biped',
          fps REAL NOT NULL DEFAULT 8,
          looping INTEGER NOT NULL DEFAULT 1,
          spec_json TEXT NOT NULL DEFAULT '{}',
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_rigs_workspace ON rigs(workspace_id, updated_at DESC);

        INSERT INTO migrations(version, applied_at) VALUES (12, datetime('now'));
        "#,
        )
        .map_err(|error| CommandError::new("migration_failed", error.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
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
        assert_eq!(migration_version, 12);

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
}
