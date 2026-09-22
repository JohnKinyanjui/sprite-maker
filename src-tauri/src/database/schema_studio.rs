use crate::error::{CommandError, CommandResult};
use rusqlite::Transaction;

pub(crate) fn migrate_v4(transaction: &Transaction<'_>) -> CommandResult<()> {
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

pub(crate) fn migrate_v5(transaction: &Transaction<'_>) -> CommandResult<()> {
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

pub(crate) fn migrate_v6(transaction: &Transaction<'_>) -> CommandResult<()> {
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
pub(crate) fn migrate_v9(transaction: &Transaction<'_>) -> CommandResult<()> {
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

pub(crate) fn migrate_v10(transaction: &Transaction<'_>) -> CommandResult<()> {
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

pub(crate) fn migrate_v11(transaction: &Transaction<'_>) -> CommandResult<()> {
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

pub(crate) fn migrate_v12(transaction: &Transaction<'_>) -> CommandResult<()> {
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

pub(crate) fn migrate_v13(transaction: &Transaction<'_>) -> CommandResult<()> {
    transaction
        .execute_batch(
            r#"
        ALTER TABLE animations ADD COLUMN review_status TEXT NOT NULL DEFAULT 'draft';

        INSERT INTO migrations(version, applied_at) VALUES (13, datetime('now'));
        "#,
        )
        .map_err(|error| CommandError::new("migration_failed", error.to_string()))?;
    Ok(())
}

pub(crate) fn migrate_v14(transaction: &Transaction<'_>) -> CommandResult<()> {
    transaction
        .execute_batch(
            r#"
        CREATE TABLE IF NOT EXISTS conversation_log_entries (
          id TEXT PRIMARY KEY,
          conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
          request_id TEXT,
          level TEXT NOT NULL DEFAULT 'info',
          category TEXT NOT NULL,
          event_type TEXT NOT NULL,
          message TEXT NOT NULL,
          details_json TEXT NOT NULL DEFAULT '{}',
          created_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_conversation_log_entries_conversation
          ON conversation_log_entries(conversation_id, created_at);

        INSERT INTO migrations(version, applied_at) VALUES (14, datetime('now'));
        "#,
        )
        .map_err(|error| CommandError::new("migration_failed", error.to_string()))?;
    Ok(())
}
