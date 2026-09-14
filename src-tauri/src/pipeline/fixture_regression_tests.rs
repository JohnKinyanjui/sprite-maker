use super::{
    contract::size_contract_check_inner,
    direction_meta::write_direction_meta,
    frame_score::score_animation_frames_inner,
    normalize::normalize_animation_inner,
    production_score::production_score_inner,
    strip::{score_strip_inner, split_sprite_strip_inner},
    test_fixtures::{
        copy_walk_strip_fixture, list_strip_fixtures, save_walk_animation_from_split,
        setup_hero_workspace, temp_pipeline_state, walk_strip_fixture_path,
    },
};
use crate::animations::write_animation_gif;
use crate::models::{AnimationDirectionMeta, NormalizeAnimationInput};
use uuid::Uuid;

pub(crate) const PRODUCTION_SCORE_CI_MIN: f64 = 60.0;
use image::codecs::gif::GifDecoder;
use image::AnimationDecoder;
use std::fs::File;
use std::io::BufReader;

#[test]
fn committed_strip_fixtures_include_walk_horizontal() {
    let fixtures = list_strip_fixtures();
    assert!(
        fixtures.iter().any(|path| path.file_name().and_then(|name| name.to_str()) == Some("walk-4-horizontal.png")),
        "expected walk-4-horizontal.png in {}",
        super::test_fixtures::strips_fixture_dir().display()
    );
}

#[test]
fn walk_strip_fixture_scores_four_frames() {
    let path = walk_strip_fixture_path();
    assert!(path.is_file(), "missing walk strip fixture");

    let report = score_strip_inner(path.to_string_lossy().as_ref(), None, Some("horizontal"))
        .expect("score strip");
    assert!(
        (4..=5).contains(&report.suggested_frame_count),
        "walk fixture should infer 4–5 frames, got {}",
        report.suggested_frame_count
    );
    assert!(report.inference_confidence > 0.0);

    let explicit = score_strip_inner(path.to_string_lossy().as_ref(), Some(4), Some("horizontal"))
        .expect("explicit frame count");
    assert_eq!(explicit.frame_count_used, 4);
}

#[test]
fn walk_strip_fixture_splits_into_four_assets() {
    let (root, state) = temp_pipeline_state();
    let (workspace, project) = setup_hero_workspace(&root, &state);
    let strip_path = project.join("assets/imports/walk-strip.png");
    copy_walk_strip_fixture(&strip_path);

    let split = split_sprite_strip_inner(
        &workspace.id,
        strip_path.to_string_lossy().as_ref(),
        "horizontal",
        4,
        None,
        true,
        "characters",
        &state,
    )
    .expect("split strip");
    assert_eq!(split.asset_ids.len(), 4);

    drop(state);
    std::fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn walk_strip_fixture_passes_size_contract_after_normalize() {
    let (root, state) = temp_pipeline_state();
    let (workspace, project) = setup_hero_workspace(&root, &state);
    let strip_path = project.join("assets/imports/walk-strip.png");
    copy_walk_strip_fixture(&strip_path);

    let split = split_sprite_strip_inner(
        &workspace.id,
        strip_path.to_string_lossy().as_ref(),
        "horizontal",
        4,
        None,
        true,
        "characters",
        &state,
    )
    .expect("split strip");
    let animation = save_walk_animation_from_split(&workspace.id, split.asset_ids, &state);

    normalize_animation_inner(
        None,
        &state,
        NormalizeAnimationInput {
            animation_id: animation.id.clone(),
            anchor_slug: Some("hero".into()),
            lock_first_frame: Some(true),
            shared_scale: Some(true),
            padding: None,
        },
    )
    .expect("normalize");

    let report =
        size_contract_check_inner(&state, &animation.id, Some("hero")).expect("size contract");
    assert!(
        report.passed,
        "walk fixture should pass size contract after normalize: {:?}",
        report.violations
    );

    drop(state);
    std::fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn walk_strip_fixture_frame_scores_are_stable() {
    let (root, state) = temp_pipeline_state();
    let (workspace, project) = setup_hero_workspace(&root, &state);
    let strip_path = project.join("assets/imports/walk-strip.png");
    copy_walk_strip_fixture(&strip_path);

    let split = split_sprite_strip_inner(
        &workspace.id,
        strip_path.to_string_lossy().as_ref(),
        "horizontal",
        4,
        None,
        true,
        "characters",
        &state,
    )
    .expect("split strip");
    let animation = save_walk_animation_from_split(&workspace.id, split.asset_ids, &state);

    let report = score_animation_frames_inner(&animation.id, &state).expect("frame scores");
    assert_eq!(report.frame_count, 4);
    assert_eq!(report.frames.len(), 4);
    assert!(report.mean_score > 0.0);
    assert_eq!(report.frames[0].motion_delta, 0.0);

    drop(state);
    std::fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn walk_animation_exports_gif_preview_with_multiple_frames() {
    let (root, state) = temp_pipeline_state();
    let (workspace, project) = setup_hero_workspace(&root, &state);
    let strip_path = project.join("assets/imports/walk-strip.png");
    copy_walk_strip_fixture(&strip_path);

    let split = split_sprite_strip_inner(
        &workspace.id,
        strip_path.to_string_lossy().as_ref(),
        "horizontal",
        4,
        None,
        true,
        "characters",
        &state,
    )
    .expect("split strip");
    let animation = save_walk_animation_from_split(&workspace.id, split.asset_ids, &state);

    let gif_path = project.join("exports/walk-preview.gif");
    write_animation_gif(&animation.id, &gif_path, &state).expect("write gif");
    assert!(gif_path.is_file(), "gif file should exist");

    let decoder = GifDecoder::new(BufReader::new(File::open(&gif_path).expect("open gif")))
        .expect("gif decoder");
    let frames = decoder.into_frames().collect_frames().expect("gif frames");
    assert!(
        frames.len() >= 2,
        "walk gif should contain at least 2 frames, got {}",
        frames.len()
    );
    let first = frames[0].buffer();
    assert!(first.width() > 0);
    assert!(first.height() > 0);

    drop(state);
    std::fs::remove_dir_all(root).expect("cleanup");
}

fn insert_character_worktree(state: &crate::AppState, workspace_id: &str) -> String {
    let connection = state.db.lock().expect("db lock");
    let id = Uuid::new_v4().to_string();
    connection
        .execute(
            "INSERT INTO worktrees (id, project_id, name, slug, kind, description, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, NULL, datetime('now'), datetime('now'))",
            rusqlite::params![id, workspace_id, "Hero", "hero", "character"],
        )
        .expect("worktree insert");
    id
}

#[test]
fn production_score_fixture_gate() {
    let (root, state) = temp_pipeline_state();
    let (workspace, project) = setup_hero_workspace(&root, &state);
    let worktree_id = insert_character_worktree(&state, &workspace.id);
    let strip_path = project.join("assets/imports/walk-strip.png");
    copy_walk_strip_fixture(&strip_path);

    let split = split_sprite_strip_inner(
        &workspace.id,
        strip_path.to_string_lossy().as_ref(),
        "horizontal",
        4,
        None,
        true,
        "characters",
        &state,
    )
    .expect("split strip");
    let animation = save_walk_animation_from_split(&workspace.id, split.asset_ids, &state);
    {
        let connection = state.db.lock().expect("db");
        connection
            .execute(
                "UPDATE animations SET worktree_id=?1 WHERE id=?2",
                rusqlite::params![worktree_id, animation.id],
            )
            .expect("attach worktree");
    }
    write_direction_meta(
        &project,
        &AnimationDirectionMeta {
            animation_id: animation.id.clone(),
            direction_family: "walk".into(),
            facing: "w".into(),
            mirrored_from: None,
            anchor_slug: Some("hero".into()),
        },
    )
    .expect("direction meta");

    normalize_animation_inner(
        None,
        &state,
        NormalizeAnimationInput {
            animation_id: animation.id.clone(),
            anchor_slug: Some("hero".into()),
            lock_first_frame: Some(true),
            shared_scale: Some(true),
            padding: None,
        },
    )
    .expect("normalize");

    let report = production_score_inner(&state, &workspace.id, &worktree_id, Some("hero"))
        .expect("production score");
    assert!(
        report.overall_score >= PRODUCTION_SCORE_CI_MIN,
        "walk fixture production score should be >= {} (got {})",
        PRODUCTION_SCORE_CI_MIN,
        report.overall_score
    );

    drop(state);
    std::fs::remove_dir_all(root).expect("cleanup");
}
