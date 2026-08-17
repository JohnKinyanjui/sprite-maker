use crate::{
    error::{CommandError, CommandResult},
    models::Workspace,
    AppState,
};
use chrono::Utc;
use rusqlite::{params, OptionalExtension};
use std::path::{Path, PathBuf};
use tauri::State;
use uuid::Uuid;

fn row_to_workspace(row: &rusqlite::Row<'_>) -> rusqlite::Result<Workspace> {
    Ok(Workspace {
        id: row.get(0)?,
        name: row.get(1)?,
        path: row.get(2)?,
        created_at: row.get(3)?,
        last_opened_at: row.get(4)?,
    })
}

fn initialize_workspace(path: &Path) -> CommandResult<()> {
    for folder in [
        "assets/characters",
        "assets/creatures",
        "assets/terrain",
        "assets/props",
        "assets/effects",
        "animations",
        "exports",
        "worktrees",
    ] {
        std::fs::create_dir_all(path.join(folder))?;
    }
    let metadata = path.join(".sprite-studio");
    std::fs::create_dir_all(&metadata)?;
    std::fs::create_dir_all(metadata.join("packs"))?;
    let gitignore = metadata.join(".gitignore");
    if !gitignore.exists() {
        std::fs::write(gitignore, "thumbnails/\n")?;
    }
    let sprite_tool = metadata.join("sprite_tool.py");
    let bundled_tool = include_str!("../resources/sprite_tool.py");
    if !sprite_tool.exists() || std::fs::read_to_string(&sprite_tool)? != bundled_tool {
        std::fs::write(sprite_tool, bundled_tool)?;
    }
    let rig_tool = metadata.join("sprite_rig.py");
    let bundled_rig_tool = include_str!("../resources/sprite_rig.py");
    if !rig_tool.exists() || std::fs::read_to_string(&rig_tool)? != bundled_rig_tool {
        std::fs::write(rig_tool, bundled_rig_tool)?;
    }
    let rig_mcp = metadata.join("sprite_rig_mcp.py");
    let bundled_rig_mcp = include_str!("../resources/sprite_rig_mcp.py");
    if !rig_mcp.exists() || std::fs::read_to_string(&rig_mcp)? != bundled_rig_mcp {
        std::fs::write(rig_mcp, bundled_rig_mcp)?;
    }
    let polish_tool = metadata.join("sprite_polish.py");
    let bundled_polish_tool = include_str!("../resources/sprite_polish.py");
    if !polish_tool.exists() || std::fs::read_to_string(&polish_tool)? != bundled_polish_tool {
        std::fs::write(polish_tool, bundled_polish_tool)?;
    }
    let terrain_cleanup = metadata.join("terrain_cleanup.py");
    let bundled_terrain_cleanup = include_str!("../resources/terrain_cleanup.py");
    if !terrain_cleanup.exists()
        || std::fs::read_to_string(&terrain_cleanup)? != bundled_terrain_cleanup
    {
        std::fs::write(terrain_cleanup, bundled_terrain_cleanup)?;
    }
    Ok(())
}

fn normalized_path(path: &str, create: bool) -> CommandResult<PathBuf> {
    let candidate = PathBuf::from(path);
    if create {
        std::fs::create_dir_all(&candidate)?;
    }
    if !candidate.is_dir() {
        return Err(CommandError::new(
            "workspace_missing",
            format!("Workspace directory does not exist: {path}"),
        ));
    }
    candidate.canonicalize().map_err(CommandError::from)
}

pub fn workspace_path(state: &AppState, workspace_id: &str) -> CommandResult<PathBuf> {
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    let path: Option<String> = connection
        .query_row(
            "SELECT path FROM projects WHERE id = ?1",
            [workspace_id],
            |row| row.get(0),
        )
        .optional()?;
    let path = path.ok_or_else(|| {
        CommandError::new("workspace_not_found", "Workspace is no longer registered")
    })?;
    let path = normalized_path(&path, false)?;
    initialize_workspace(&path)?;
    Ok(path)
}

#[tauri::command]
pub fn list_workspaces(state: State<'_, AppState>) -> CommandResult<Vec<Workspace>> {
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    let mut statement = connection.prepare("SELECT id, name, path, created_at, last_opened_at FROM projects ORDER BY last_opened_at DESC")?;
    let rows = statement.query_map([], row_to_workspace)?;
    Ok(rows.filter_map(Result::ok).collect())
}

/// Sidebar payload in one command: all projects plus the active project's
/// worktrees and chats, queried under a single database lock so the shell
/// renders in one IPC round trip.
#[tauri::command]
pub fn load_sidebar_state(
    workspace_id: Option<String>,
    state: State<'_, AppState>,
) -> CommandResult<crate::models::SidebarSnapshot> {
    load_sidebar_state_inner(&state, workspace_id.as_deref())
}

pub(crate) fn load_sidebar_state_inner(
    state: &AppState,
    workspace_id: Option<&str>,
) -> CommandResult<crate::models::SidebarSnapshot> {
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    let mut statement = connection.prepare("SELECT id, name, path, created_at, last_opened_at FROM projects ORDER BY last_opened_at DESC")?;
    let workspaces: Vec<Workspace> = statement
        .query_map([], row_to_workspace)?
        .filter_map(Result::ok)
        .collect();
    let mut worktrees = Vec::new();
    let mut conversations = Vec::new();
    if let Some(workspace_id) = workspace_id {
        let mut statement = connection.prepare(
            "SELECT id, project_id, name, slug, kind, description, created_at, updated_at FROM worktrees WHERE project_id = ?1 ORDER BY CASE kind WHEN 'general' THEN 0 ELSE 1 END, updated_at DESC, name",
        )?;
        worktrees = statement
            .query_map([workspace_id], crate::worktrees::row_to_worktree)?
            .filter_map(Result::ok)
            .collect();
        let mut statement = connection.prepare(
            "SELECT id, workspace_id, worktree_id, title, provider, provider_session_id, created_at, updated_at, archived_at FROM conversations WHERE workspace_id = ?1 AND archived_at IS NULL ORDER BY updated_at DESC",
        )?;
        conversations = statement
            .query_map([workspace_id], crate::conversations::conversation_row)?
            .filter_map(Result::ok)
            .collect();
    }
    Ok(crate::models::SidebarSnapshot {
        workspaces,
        worktrees,
        conversations,
    })
}

#[tauri::command]
pub fn create_workspace(
    name: String,
    path: String,
    state: State<'_, AppState>,
) -> CommandResult<Workspace> {
    let name = name.trim();
    if name.is_empty() {
        return Err(CommandError::new(
            "invalid_name",
            "Workspace name cannot be empty",
        ));
    }
    let path = normalized_path(&path, true)?;
    initialize_workspace(&path)?;
    register_workspace(name, &path, &state)
}

#[tauri::command]
pub fn open_workspace(path: String, state: State<'_, AppState>) -> CommandResult<Workspace> {
    let path = normalized_path(&path, false)?;
    initialize_workspace(&path)?;
    let fallback_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("Untitled workspace");
    register_workspace(fallback_name, &path, &state)
}

fn register_workspace(name: &str, path: &Path, state: &AppState) -> CommandResult<Workspace> {
    let now = Utc::now().to_rfc3339();
    let path_string = path.to_string_lossy().into_owned();
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    let existing: Option<Workspace> = connection
        .query_row(
            "SELECT id, name, path, created_at, last_opened_at FROM projects WHERE path = ?1",
            [&path_string],
            row_to_workspace,
        )
        .optional()?;
    if let Some(mut workspace) = existing {
        connection.execute(
            "UPDATE projects SET last_opened_at = ?1 WHERE id = ?2",
            params![now, workspace.id],
        )?;
        ensure_default_worktree(&connection, &workspace.id, &now)?;
        workspace.last_opened_at = now;
        return Ok(workspace);
    }
    let workspace = Workspace {
        id: Uuid::new_v4().to_string(),
        name: name.to_string(),
        path: path_string,
        created_at: now.clone(),
        last_opened_at: now,
    };
    connection.execute(
        "INSERT INTO projects(id, name, path, created_at, last_opened_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![workspace.id, workspace.name, workspace.path, workspace.created_at, workspace.last_opened_at],
    )?;
    ensure_default_worktree(&connection, &workspace.id, &workspace.created_at)?;
    Ok(workspace)
}

fn ensure_default_worktree(
    connection: &rusqlite::Connection,
    project_id: &str,
    timestamp: &str,
) -> CommandResult<()> {
    connection.execute(
        "INSERT OR IGNORE INTO worktrees(id, project_id, name, slug, kind, description, created_at, updated_at) VALUES (?1, ?2, 'General', 'general', 'general', 'Project-level assets and conversations', ?3, ?3)",
        params![Uuid::new_v4().to_string(), project_id, timestamp],
    )?;
    Ok(())
}

#[tauri::command]
pub fn touch_workspace(id: String, state: State<'_, AppState>) -> CommandResult<Workspace> {
    let now = Utc::now().to_rfc3339();
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    connection.execute(
        "UPDATE projects SET last_opened_at = ?1 WHERE id = ?2",
        params![now, id],
    )?;
    let workspace = connection
        .query_row(
            "SELECT id, name, path, created_at, last_opened_at FROM projects WHERE id = ?1",
            [&id],
            row_to_workspace,
        )
        .optional()?
        .ok_or_else(|| {
            CommandError::new("workspace_not_found", "Workspace is no longer registered")
        })?;
    if !Path::new(&workspace.path).is_dir() {
        return Err(CommandError::new(
            "workspace_missing",
            "The workspace directory was moved or deleted",
        ));
    }
    Ok(workspace)
}

#[tauri::command]
pub fn rename_workspace(id: String, name: String, state: State<'_, AppState>) -> CommandResult<()> {
    let name = name.trim();
    if name.is_empty() {
        return Err(CommandError::new(
            "invalid_name",
            "Workspace name cannot be empty",
        ));
    }
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    let changed = connection.execute(
        "UPDATE projects SET name = ?1 WHERE id = ?2",
        params![name, id],
    )?;
    if changed == 0 {
        return Err(CommandError::new(
            "workspace_not_found",
            "Workspace is no longer registered",
        ));
    }
    Ok(())
}

#[tauri::command]
pub fn remove_workspace(id: String, state: State<'_, AppState>) -> CommandResult<()> {
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    connection.execute("DELETE FROM projects WHERE id = ?1", [id])?;
    Ok(())
}

#[tauri::command]
pub fn delete_workspace(id: String, state: State<'_, AppState>) -> CommandResult<()> {
    let path = workspace_path(&state, &id)?;
    if path.parent().is_none() || path.components().count() < 3 {
        return Err(CommandError::new(
            "unsafe_delete",
            "Refusing to delete a broad filesystem path",
        ));
    }
    std::fs::remove_dir_all(&path)?;
    remove_workspace(id, state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database;
    use image::{Rgba, RgbaImage};
    use serde_json::{json, Value};
    use std::{
        collections::{HashMap, HashSet},
        io::{BufRead, BufReader, Write},
        process::{Child, ChildStdin, ChildStdout, Command, Stdio},
        sync::Mutex,
    };

    struct RigRendererFixture {
        root: PathBuf,
    }

    struct RigMcpSession {
        child: Child,
        stdin: Option<ChildStdin>,
        stdout: BufReader<ChildStdout>,
    }

    impl RigMcpSession {
        fn new(root: &Path) -> Self {
            let mut child = Command::new("python3")
                .current_dir(root)
                .arg(".sprite-studio/sprite_rig_mcp.py")
                .arg("--workspace")
                .arg(root)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .expect("python3 should start the installed rig MCP server");
            let stdin = child.stdin.take().expect("MCP stdin should be piped");
            let stdout = child.stdout.take().expect("MCP stdout should be piped");
            Self {
                child,
                stdin: Some(stdin),
                stdout: BufReader::new(stdout),
            }
        }

        fn send(&mut self, message: &Value) {
            let stdin = self.stdin.as_mut().expect("MCP stdin should stay open");
            serde_json::to_writer(&mut *stdin, message).expect("MCP message should serialize");
            stdin
                .write_all(b"\n")
                .expect("MCP message should terminate with a newline");
            stdin.flush().expect("MCP message should flush");
        }

        fn send_raw(&mut self, message: &[u8]) {
            let stdin = self.stdin.as_mut().expect("MCP stdin should stay open");
            stdin
                .write_all(message)
                .expect("raw MCP message should write");
            stdin
                .write_all(b"\n")
                .expect("raw MCP message should terminate with a newline");
            stdin.flush().expect("raw MCP message should flush");
        }

        fn read_response(&mut self) -> Value {
            let mut line = String::new();
            let bytes = self
                .stdout
                .read_line(&mut line)
                .expect("MCP response should be readable");
            assert!(bytes > 0, "MCP server exited before responding");
            serde_json::from_str(&line).unwrap_or_else(|error| {
                panic!("MCP stdout must contain only JSON-RPC: {error}: {line:?}")
            })
        }

        fn request(&mut self, message: &Value) -> Value {
            self.send(message);
            self.read_response()
        }

        fn initialize_legacy(&mut self) {
            let initialized = self.request(&json!({
                "jsonrpc": "2.0",
                "id": "test-initialize",
                "method": "initialize",
                "params": {
                    "protocolVersion": "2025-11-25",
                    "capabilities": {},
                    "clientInfo": {"name": "sprite-studio-test", "version": "1"},
                },
            }));
            assert_eq!(initialized["result"]["protocolVersion"], "2025-11-25");
            self.send(&json!({
                "jsonrpc": "2.0",
                "method": "notifications/initialized",
            }));
        }

        fn call_tool(&mut self, id: u64, name: &str, rig: &str) -> Value {
            self.request(&json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": "tools/call",
                "params": {
                    "name": name,
                    "arguments": {"rig": rig},
                },
            }))
        }
    }

    impl Drop for RigMcpSession {
        fn drop(&mut self) {
            self.stdin.take();
            let _ = self.child.wait();
        }
    }

    impl RigRendererFixture {
        fn new() -> Self {
            let root =
                std::env::temp_dir().join(format!("sprite-studio-rig-v2-test-{}", Uuid::new_v4()));
            initialize_workspace(&root).expect("workspace should initialize");
            std::fs::create_dir_all(root.join(".sprite-studio/rigs"))
                .expect("rig directory should exist");

            let mut master = RgbaImage::new(32, 32);
            for y in 4..14 {
                for x in 8..24 {
                    master.put_pixel(x, y, Rgba([62, 96, 148, 255]));
                }
            }
            for y in 12..27 {
                for x in 9..13 {
                    master.put_pixel(x, y, Rgba([214, 132, 78, 255]));
                }
                for x in 19..23 {
                    master.put_pixel(x, y, Rgba([196, 105, 66, 255]));
                }
            }
            master
                .save(root.join("assets/characters/walker.png"))
                .expect("rig master should save");
            Self { root }
        }

        fn validate(&self, name: &str, spec: &Value) -> std::process::Output {
            self.run(name, spec, true)
        }

        fn write_rig(&self, name: &str, spec: &Value) -> (PathBuf, Vec<u8>) {
            let path = self.root.join(format!(".sprite-studio/rigs/{name}.json"));
            let bytes = serde_json::to_vec(spec).expect("rig should serialize");
            std::fs::write(&path, &bytes).expect("rig should write");
            (path, bytes)
        }

        fn render(&self, name: &str, spec: &Value) -> std::process::Output {
            self.run(name, spec, false)
        }

        fn run(&self, name: &str, spec: &Value, validate_only: bool) -> std::process::Output {
            let relative = format!(".sprite-studio/rigs/{name}.json");
            std::fs::write(
                self.root.join(&relative),
                serde_json::to_vec_pretty(spec).expect("rig should serialize"),
            )
            .expect("rig should write");
            let mut command = Command::new("python3");
            command
                .current_dir(&self.root)
                .arg(".sprite-studio/sprite_rig.py");
            if validate_only {
                command.arg("--validate");
            }
            command
                .arg(&relative)
                .output()
                .expect("python3 should execute the bundled rig renderer")
        }
    }

    impl Drop for RigRendererFixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    fn leg_transform(upper_rotate: i32, lower_rotate: i32) -> Value {
        json!({
            "left_upper": {"rotate": upper_rotate},
            "left_lower": {"rotate": lower_rotate},
        })
    }

    fn legacy_color_rig() -> Value {
        json!({
            "rigVersion": 1,
            "name": "legacy_color_validation",
            "category": "characters",
            "source": "assets/characters/walker.png",
            "fps": 8,
            "palette": {"accent": "#ff8844"},
            "parts": [{
                "name": "leg",
                "mask": {"rect": [9, 12, 4, 15]},
                "pivot": [11, 12],
                "z": 1,
            }],
            "frames": [
                {
                    "transforms": {"leg": {"rotate": -5}},
                    "overlay": [{"type": "pixel", "x": 1, "y": 1, "color": "accent"}],
                },
                {
                    "transforms": {"leg": {"rotate": 5}},
                    "overlay": [{"type": "pixel", "x": 2, "y": 1, "color": "accent"}],
                },
            ],
        })
    }

    fn good_biped_walk() -> Value {
        let phases = [
            "left_contact",
            "left_down",
            "left_passing",
            "left_to_right_double_support",
            "right_contact",
            "right_down",
            "right_passing",
            "right_to_left_double_support",
        ];
        let left_angles = [
            (0, 0),
            (0, 0),
            (0, 0),
            (0, 0),
            (-8, 8),
            (-4, 4),
            (8, -8),
            (0, 0),
        ];
        let right_angles = [
            (-8, 8),
            (-4, 4),
            (8, -8),
            (0, 0),
            (0, 0),
            (0, 0),
            (0, 0),
            (0, 0),
        ];
        let frames = (0..8)
            .map(|index| {
                let mut transforms = leg_transform(left_angles[index].0, left_angles[index].1);
                let object = transforms
                    .as_object_mut()
                    .expect("transforms should be an object");
                object.insert(
                    "right_upper".into(),
                    json!({"rotate": right_angles[index].0}),
                );
                object.insert(
                    "right_lower".into(),
                    json!({"rotate": right_angles[index].1}),
                );
                object.insert(
                    "torso".into(),
                    json!({
                        "dx": if index == 3 { -1 } else if index == 7 { 1 } else { 0 },
                        "dy": if index == 5 { 1 } else { 0 },
                    }),
                );
                let mut contacts = Vec::new();
                if index <= 3 || index == 7 {
                    contacts.push(json!({
                        "part": "left_lower",
                        "anchor": "foot",
                        "state": "planted",
                    }));
                }
                if index >= 3 {
                    contacts.push(json!({
                        "part": "right_lower",
                        "anchor": "foot",
                        "state": "planted",
                    }));
                }
                json!({
                    "phase": phases[index],
                    "contacts": contacts,
                    "root": {"dx": 0, "dy": 0},
                    "transforms": transforms,
                })
            })
            .collect::<Vec<_>>();

        json!({
            "rigVersion": 2,
            "name": "regression_biped_walk",
            "category": "characters",
            "source": "assets/characters/walker.png",
            "fps": 8,
            "looping": true,
            "rootMotion": "in-place",
            "proposal": {
                "morphologyTag": "biped",
                "motionIntent": "walking in a seamless in-place loop",
            },
            "parts": [
                {
                    "name": "torso",
                    "mask": {"rect": [8, 4, 16, 8]},
                    "pivot": [16, 11],
                    "z": 0,
                    "role": "torso",
                    "anchors": {"center": [16, 8]},
                },
                {
                    "name": "left_upper",
                    "mask": {"rect": [9, 12, 4, 7]},
                    "pivot": [11, 12],
                    "z": 2,
                    "role": "left_upper_leg",
                    "anchors": {"hip": [11, 12], "knee": [11, 19]},
                },
                {
                    "name": "left_lower",
                    "mask": {"rect": [9, 19, 4, 8]},
                    "pivot": [11, 19],
                    "z": 3,
                    "role": "left_lower_leg",
                    "parent": "left_upper",
                    "anchors": {"knee": [11, 19], "foot": [11, 27]},
                    "attach": {"parentAnchor": "knee", "selfAnchor": "knee"},
                },
                {
                    "name": "right_upper",
                    "mask": {"rect": [19, 12, 4, 7]},
                    "pivot": [21, 12],
                    "z": 1,
                    "role": "right_upper_leg",
                    "anchors": {"hip": [21, 12], "knee": [21, 19]},
                },
                {
                    "name": "right_lower",
                    "mask": {"rect": [19, 19, 4, 8]},
                    "pivot": [21, 19],
                    "z": 2,
                    "role": "right_lower_leg",
                    "parent": "right_upper",
                    "anchors": {"knee": [21, 19], "foot": [21, 27]},
                    "attach": {"parentAnchor": "knee", "selfAnchor": "knee"},
                },
            ],
            "frames": frames,
        })
    }

    fn ik_biped_walk() -> Value {
        let mut rig = good_biped_walk();
        let left_targets = [
            (11, 26),
            (11, 26),
            (11, 26),
            (11, 26),
            (14, 26),
            (13, 24),
            (11, 25),
            (11, 26),
        ];
        let right_targets = [
            (18, 26),
            (19, 24),
            (21, 25),
            (21, 26),
            (21, 26),
            (21, 26),
            (21, 26),
            (21, 26),
        ];
        for index in 0..8 {
            let torso = rig["frames"][index]["transforms"]["torso"].clone();
            rig["frames"][index]["transforms"] = json!({"torso": torso});
            rig["frames"][index]["ik"] = json!([
                {
                    "chain": ["left_upper", "left_lower"],
                    "endAnchor": "foot",
                    "target": [left_targets[index].0, left_targets[index].1],
                    "bend": 1,
                },
                {
                    "chain": ["right_upper", "right_lower"],
                    "endAnchor": "foot",
                    "target": [right_targets[index].0, right_targets[index].1],
                    "bend": -1,
                },
            ]);
        }
        rig
    }

    fn good_v3_human_walk() -> Value {
        let mut rig = good_biped_walk();
        rig["rigVersion"] = json!(3);
        rig["name"] = json!("profiled_human_walk");
        rig["rigProfile"] = json!("human_sprite_rig");
        rig["parts"][0]["mask"] = json!({"rect": [8, 4, 16, 8]});
        rig["parts"][0]["anchors"] = json!({
            "top": [16, 4], "bottom": [16, 12],
            "left_hip": [11, 12], "right_hip": [21, 12]
        });
        rig["parts"][0]["bone"] = json!({"startAnchor": "top", "endAnchor": "bottom", "radius": 8});
        rig["parts"][1]["mask"] = json!({"rect": [9, 14, 4, 5]});
        rig["parts"][1]["pivot"] = json!([11, 14]);
        rig["parts"][1]["anchors"] = json!({"hip": [11, 14], "knee": [11, 19]});
        rig["parts"][1]["parent"] = json!("pelvis");
        rig["parts"][1]["attach"] = json!({"parentAnchor": "left_hip", "selfAnchor": "hip"});
        rig["parts"][3]["mask"] = json!({"rect": [19, 14, 4, 5]});
        rig["parts"][3]["pivot"] = json!([21, 14]);
        rig["parts"][3]["anchors"] = json!({"hip": [21, 14], "knee": [21, 19]});
        rig["parts"][3]["parent"] = json!("pelvis");
        rig["parts"][3]["attach"] = json!({"parentAnchor": "right_hip", "selfAnchor": "hip"});
        for (index, start, end) in [
            (1, "hip", "knee"),
            (2, "knee", "ankle"),
            (3, "hip", "knee"),
            (4, "knee", "ankle"),
        ] {
            rig["parts"][index]["bone"] =
                json!({"startAnchor": start, "endAnchor": end, "radius": 2});
        }
        rig["parts"][2]["mask"] = json!({"rect": [9, 19, 4, 5]});
        rig["parts"][2]["anchors"] = json!({"knee": [11, 19], "ankle": [11, 24]});
        rig["parts"][4]["mask"] = json!({"rect": [19, 19, 4, 5]});
        rig["parts"][4]["anchors"] = json!({"knee": [21, 19], "ankle": [21, 24]});
        let left_foot = json!({
            "name": "left_foot",
            "mask": {"rect": [9, 24, 4, 3]},
            "pivot": [11, 24],
            "z": 4,
            "role": "left_foot",
            "parent": "left_lower",
            "anchors": {"ankle": [11, 24], "sole": [11, 27]},
            "attach": {"parentAnchor": "ankle", "selfAnchor": "ankle"},
            "bone": {"startAnchor": "ankle", "endAnchor": "sole", "radius": 2}
        });
        let right_foot = json!({
            "name": "right_foot",
            "mask": {"rect": [19, 24, 4, 3]},
            "pivot": [21, 24],
            "z": 3,
            "role": "right_foot",
            "parent": "right_lower",
            "anchors": {"ankle": [21, 24], "sole": [21, 27]},
            "attach": {"parentAnchor": "ankle", "selfAnchor": "ankle"},
            "bone": {"startAnchor": "ankle", "endAnchor": "sole", "radius": 2}
        });
        let pelvis = json!({
            "name": "pelvis",
            "mask": {"rect": [8, 12, 16, 2]},
            "pivot": [16, 12],
            "z": 1,
            "role": "pelvis",
            "parent": "torso",
            "anchors": {
                "top": [16, 12], "bottom": [16, 14],
                "left_hip": [11, 14], "right_hip": [21, 14]
            },
            "attach": {"parentAnchor": "bottom", "selfAnchor": "top"},
            "bone": {"startAnchor": "top", "endAnchor": "bottom", "radius": 8}
        });
        rig["parts"]
            .as_array_mut()
            .expect("parts should be an array")
            .extend([pelvis, left_foot, right_foot]);
        rig["joints"] = json!([
            {"name":"left_hip","kind":"hip","position":[11,14],"visibility":"visible","parts":["pelvis","left_upper"]},
            {"name":"left_knee","kind":"knee","position":[11,19],"visibility":"visible","parts":["left_upper","left_lower"]},
            {"name":"left_ankle","kind":"ankle","position":[11,24],"visibility":"visible","parts":["left_lower","left_foot"]},
            {"name":"right_hip","kind":"hip","position":[21,14],"visibility":"visible","parts":["pelvis","right_upper"]},
            {"name":"right_knee","kind":"knee","position":[21,19],"visibility":"visible","parts":["right_upper","right_lower"]},
            {"name":"right_ankle","kind":"ankle","position":[21,24],"visibility":"visible","parts":["right_lower","right_foot"]}
        ]);
        let poses = [
            "left_contact",
            "left_down",
            "left_passing",
            "left_up",
            "right_contact",
            "right_down",
            "right_passing",
            "right_up",
        ];
        for (index, pose) in poses.into_iter().enumerate() {
            rig["frames"][index]["pose"] = json!(pose);
            rig["frames"][index]["transforms"]["pelvis"] = json!({"dx": 0});
            if index <= 2 {
                // Keep the planted side locked while making the passing leg's pose
                // materially distinct from contact. This avoids a visual pause
                // without introducing root or planted-foot slide.
                let swing = [-15, 0, 15][index];
                rig["frames"][index]["transforms"]["right_upper"]["rotate"] = json!(swing);
                rig["frames"][index]["transforms"]["right_lower"]["rotate"] = json!(-swing);
            }
            let torso_dx = if index == 3 {
                -1
            } else if index == 7 {
                1
            } else {
                0
            };
            rig["frames"][index]["transforms"]["torso"]["dx"] = json!(torso_dx);
            if index == 3 || index == 7 {
                let compensation = -torso_dx;
                rig["frames"][index]["transforms"]["left_foot"] = json!({"dx": compensation});
                rig["frames"][index]["transforms"]["right_foot"] = json!({"dx": compensation});
            }
            let contacts = rig["frames"][index]["contacts"]
                .as_array_mut()
                .expect("contacts should be an array");
            for contact in contacts {
                if contact["part"] == "left_lower" {
                    contact["part"] = json!("left_foot");
                    contact["anchor"] = json!("sole");
                } else if contact["part"] == "right_lower" {
                    contact["part"] = json!("right_foot");
                    contact["anchor"] = json!("sole");
                }
            }
        }
        rig
    }

    fn assert_validation_failed(
        output: &std::process::Output,
        case: &str,
        expected_diagnostic: &str,
    ) {
        assert!(
            !output.status.success(),
            "{case} unexpectedly passed validation: {}",
            String::from_utf8_lossy(&output.stdout)
        );
        let diagnostic = String::from_utf8_lossy(&output.stderr);
        assert!(
            diagnostic.contains("sprite_rig:"),
            "{case} did not emit a renderer diagnostic: {diagnostic}"
        );
        assert!(
            diagnostic.contains(expected_diagnostic),
            "{case} failed for the wrong reason; expected {expected_diagnostic:?}: {diagnostic}"
        );
        assert!(
            !diagnostic.contains("Traceback"),
            "{case} crashed instead of producing a validation diagnostic: {diagnostic}"
        );
    }

    fn fixture() -> (PathBuf, AppState) {
        let root =
            std::env::temp_dir().join(format!("sprite-studio-workspace-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("temporary directory should be created");
        let connection = database::open(&root.join("app.sqlite3")).expect("database should open");
        (
            root,
            AppState {
                db: Mutex::new(connection),
                cancellers: Mutex::new(HashMap::new()),
            },
        )
    }

    #[test]
    fn missing_workspace_returns_a_typed_error() {
        let missing =
            std::env::temp_dir().join(format!("missing-sprite-workspace-{}", Uuid::new_v4()));
        let error = normalized_path(missing.to_string_lossy().as_ref(), false)
            .expect_err("missing directory must fail");
        assert_eq!(error.code, "workspace_missing");
    }

    #[test]
    fn sidebar_snapshot_loads_projects_worktrees_and_chats_together() {
        let (root, state) = fixture();
        let project = root.join("project-a");
        std::fs::create_dir_all(&project).expect("project should exist");
        let workspace =
            register_workspace("Project A", &project, &state).expect("workspace should register");
        let other = root.join("project-b");
        std::fs::create_dir_all(&other).expect("project should exist");
        register_workspace("Project B", &other, &state).expect("workspace should register");
        {
            let connection = state
                .db
                .lock()
                .expect("database lock");
            connection
                .execute(
                    "INSERT INTO conversations(id, workspace_id, worktree_id, title, provider, created_at, updated_at) VALUES ('c1', ?1, NULL, 'General chat', 'codex', 'now', 'now')",
                    [&workspace.id],
                )
                .expect("general chat should insert");
            connection
                .execute(
                    "INSERT INTO conversations(id, workspace_id, worktree_id, title, provider, created_at, updated_at, archived_at) VALUES ('c2', ?1, NULL, 'Archived chat', 'codex', 'now', 'now', 'archived')",
                    [&workspace.id],
                )
                .expect("archived chat should insert");
        }
        let snapshot =
            load_sidebar_state_inner(&state, Some(&workspace.id)).expect("snapshot should load");
        assert_eq!(snapshot.workspaces.len(), 2, "both projects should return");
        assert!(
            snapshot
                .worktrees
                .iter()
                .any(|worktree| worktree.kind == "general"),
            "the registered project's worktrees should be included"
        );
        assert_eq!(
            snapshot.conversations.len(),
            1,
            "only active chats of the requested project should return"
        );
        assert_eq!(snapshot.conversations[0].title, "General chat");

        let bare = load_sidebar_state_inner(&state, None).expect("bare snapshot should load");
        assert_eq!(bare.workspaces.len(), 2);
        assert!(bare.worktrees.is_empty());
        assert!(bare.conversations.is_empty());
        drop(state);
        std::fs::remove_dir_all(root).expect("temporary fixture should be removable");
    }

    #[test]
    fn duplicate_workspace_open_reuses_the_registration() {
        let (root, state) = fixture();
        let project = root.join("project");
        std::fs::create_dir_all(&project).expect("project should exist");
        let first =
            register_workspace("Test", &project, &state).expect("workspace should register");
        let second = register_workspace("Test again", &project, &state)
            .expect("duplicate open should be graceful");
        assert_eq!(first.id, second.id);
        let count: i64 = state
            .db
            .lock()
            .expect("database lock")
            .query_row("SELECT COUNT(*) FROM projects", [], |row| row.get(0))
            .expect("count should work");
        assert_eq!(count, 1);
        let worktree_count: i64 = state
            .db
            .lock()
            .expect("database lock")
            .query_row(
                "SELECT COUNT(*) FROM worktrees WHERE project_id = ?1",
                [&first.id],
                |row| row.get(0),
            )
            .expect("default worktree count should work");
        assert_eq!(worktree_count, 1);
        drop(state);
        std::fs::remove_dir_all(root).expect("temporary fixture should be removable");
    }

    #[test]
    fn initialization_installs_the_codex_sprite_renderers() {
        let root =
            std::env::temp_dir().join(format!("sprite-studio-renderer-test-{}", Uuid::new_v4()));
        initialize_workspace(&root).expect("workspace should initialize");
        let tool = root.join(".sprite-studio/sprite_tool.py");
        assert!(tool.is_file());
        assert!(std::fs::read_to_string(tool)
            .expect("tool should be readable")
            .contains("Small dependency-free pixel sprite renderer"));
        let rig_tool = root.join(".sprite-studio/sprite_rig.py");
        assert!(rig_tool.is_file());
        assert!(std::fs::read_to_string(rig_tool)
            .expect("rig tool should be readable")
            .contains("layered pixel-rig renderer"));
        let rig_mcp = root.join(".sprite-studio/sprite_rig_mcp.py");
        assert!(rig_mcp.is_file());
        assert!(std::fs::read_to_string(rig_mcp)
            .expect("rig MCP server should be readable")
            .contains("Workspace-scoped MCP server"));
        let polish_tool = root.join(".sprite-studio/sprite_polish.py");
        assert!(polish_tool.is_file());
        assert!(std::fs::read_to_string(polish_tool)
            .expect("polish tool should be readable")
            .contains("Normalize one ImageGen frame repair"));
        let terrain_cleanup = root.join(".sprite-studio/terrain_cleanup.py");
        assert!(terrain_cleanup.is_file());
        assert!(std::fs::read_to_string(terrain_cleanup)
            .expect("terrain cleanup tool should be readable")
            .contains("Remove accidental magenta chroma fringe"));
        assert!(root.join(".sprite-studio/packs").is_dir());
        assert!(root.join("assets/characters").is_dir());
        assert!(root.join("assets/creatures").is_dir());
        std::fs::remove_dir_all(root).expect("temporary fixture should be removable");
    }

    #[test]
    fn bundled_rig_renderer_accepts_a_connected_eight_frame_biped_walk() {
        let fixture = RigRendererFixture::new();
        let output = fixture.render("good-walk", &good_biped_walk());
        assert!(
            output.status.success(),
            "good biped walk should render: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let manifest: Value = serde_json::from_slice(&output.stdout)
            .expect("successful rig rendering should print a JSON manifest");
        let files = manifest["files"]
            .as_array()
            .expect("render manifest should list output files");
        assert_eq!(files.len(), 8, "walk should render all eight planned poses");
        assert!(files.iter().all(|file| fixture
            .root
            .join(file.as_str().expect("manifest file should be a string"))
            .is_file()));
    }

    #[test]
    fn bundled_rig_renderer_deforms_a_weighted_pixel_mesh_deterministically() {
        let fixture = RigRendererFixture::new();
        let mut rig = good_biped_walk();
        rig["name"] = json!("weighted_mesh_walk");
        rig["parts"][1]["mesh"] = json!({
            "vertices": [[9, 12], [13, 12], [9, 19], [13, 19]],
            "triangles": [[0, 1, 2], [1, 3, 2]],
            "weights": [
                [{"bone": "left_upper", "weight": 1.0}],
                [{"bone": "left_upper", "weight": 1.0}],
                [{"bone": "left_lower", "weight": 1.0}],
                [{"bone": "left_lower", "weight": 1.0}],
            ],
        });
        let first = fixture.render("weighted-mesh-walk", &rig);
        assert!(
            first.status.success(),
            "weighted mesh walk should render: {}",
            String::from_utf8_lossy(&first.stderr)
        );
        let first_manifest: Value = serde_json::from_slice(&first.stdout)
            .expect("weighted mesh render should print a manifest");
        let first_hashes = first_manifest["quality"]["renderedFrameHashes"].clone();
        let second = fixture.render("weighted-mesh-walk", &rig);
        assert!(
            second.status.success(),
            "weighted mesh rerender should succeed: {}",
            String::from_utf8_lossy(&second.stderr)
        );
        let second_manifest: Value = serde_json::from_slice(&second.stdout)
            .expect("weighted mesh rerender should print a manifest");
        assert_eq!(
            first_hashes, second_manifest["quality"]["renderedFrameHashes"],
            "weighted mesh rasterization must be reproducible byte-for-byte"
        );
    }

    #[test]
    fn bundled_rig_renderer_enforces_profiled_joints_poses_and_residual_ownership() {
        let fixture = RigRendererFixture::new();
        let rig = good_v3_human_walk();
        let rendered = fixture.render("profiled-human-walk", &rig);
        assert!(
            rendered.status.success(),
            "version 3 human rig should render: {}",
            String::from_utf8_lossy(&rendered.stderr)
        );
        let manifest: Value = serde_json::from_slice(&rendered.stdout)
            .expect("profiled render should print a manifest");
        assert_eq!(manifest["rigVersion"], 3);

        let mut mcp = RigMcpSession::new(&fixture.root);
        mcp.initialize_legacy();
        let analyzed = mcp.request(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": "sprite_rig_analyze_motion",
                "arguments": {"rig": ".sprite-studio/rigs/profiled-human-walk.json"},
            },
        }));
        assert_eq!(analyzed["result"]["isError"], false);
        let anatomy = &analyzed["result"]["structuredContent"];
        assert_eq!(anatomy["rigProfile"], "human_sprite_rig");
        assert_eq!(anatomy["visibleJointCount"], 6);
        assert_eq!(anatomy["joints"].as_array().map(Vec::len), Some(6));
        assert_eq!(anatomy["poses"].as_array().map(Vec::len), Some(8));

        let mut wrong_animal_harness = rig.clone();
        wrong_animal_harness["name"] = json!("profiled_quadruped_without_joints");
        wrong_animal_harness["rigProfile"] = json!("four_leg_sprite_rig");
        wrong_animal_harness["proposal"]["morphologyTag"] = json!("quadruped");
        let animal_rejected =
            fixture.validate("profiled-quadruped-without-joints", &wrong_animal_harness);
        assert_validation_failed(
            &animal_rejected,
            "four-leg profile without observed animal joints",
            "missing observed joints",
        );

        let mut residual = rig;
        residual["name"] = json!("profiled_human_residual");
        residual["parts"][1]["mask"] = json!({"rect": [10, 14, 3, 5]});
        let rejected = fixture.validate("profiled-human-residual", &residual);
        assert_validation_failed(
            &rejected,
            "profiled human residual",
            "unclaimed source pixels",
        );
    }

    #[test]
    fn bundled_rig_renderer_solves_two_bone_ik_with_locked_contacts() {
        let fixture = RigRendererFixture::new();
        let output = fixture.render("ik-walk", &ik_biped_walk());
        assert!(
            output.status.success(),
            "IK biped walk should render: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let manifest: Value = serde_json::from_slice(&output.stdout)
            .expect("successful IK rig rendering should print a JSON manifest");
        assert_eq!(manifest["rigVersion"], 2);
        assert_eq!(manifest["quality"]["mechanics"], "passed");
        assert_eq!(manifest["quality"]["uniqueRenderedFrames"], 8);
    }

    #[test]
    fn bundled_rig_renderer_rejects_an_unreachable_ik_target() {
        let fixture = RigRendererFixture::new();
        let mut rig = ik_biped_walk();
        rig["frames"][0]["ik"][0]["target"] = json!([0, 0]);
        let output = fixture.validate("unreachable-ik", &rig);
        assert_validation_failed(&output, "unreachable IK target", "target is unreachable");
    }

    #[test]
    fn bundled_rig_renderer_accepts_a_two_pixel_in_place_weight_shift() {
        let fixture = RigRendererFixture::new();
        let mut rig = ik_biped_walk();
        let root_x = [-1, -1, 0, 1, 1, 1, 0, -1];
        for (index, value) in root_x.into_iter().enumerate() {
            rig["frames"][index]["root"]["dx"] = json!(value);
        }
        let output = fixture.validate("weight-shift", &rig);
        assert!(
            output.status.success(),
            "a one-pixel lateral root amplitude should validate: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn bundled_rig_renderer_accepts_monotonic_non_looping_baked_travel() {
        let fixture = RigRendererFixture::new();
        let mut rig = ik_biped_walk();
        rig["looping"] = json!(false);
        rig["rootMotion"] = json!("baked");
        let root_x = [0, 0, 1, 1, 2, 2, 3, 3];
        for (index, value) in root_x.into_iter().enumerate() {
            rig["frames"][index]["root"]["dx"] = json!(value);
        }
        let output = fixture.validate("baked-travel", &rig);
        assert!(
            output.status.success(),
            "non-looping baked travel should not be forced through loop-seam gates: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn bundled_rig_renderer_accepts_one_adjacent_explicit_hold() {
        let fixture = RigRendererFixture::new();
        let mut rig = good_biped_walk();
        rig["frames"][1]["transforms"] = rig["frames"][0]["transforms"].clone();
        rig["frames"][1]["root"] = rig["frames"][0]["root"].clone();
        rig["frames"][1]["contacts"] = rig["frames"][0]["contacts"].clone();
        rig["frames"][1]["hold"] = json!(true);
        let output = fixture.validate("explicit-hold", &rig);
        assert!(
            output.status.success(),
            "one adjacent explicit hold should validate: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn bundled_rig_renderer_rejects_a_non_adjacent_repeated_pose() {
        let fixture = RigRendererFixture::new();
        let mut rig = good_biped_walk();
        rig["frames"][4]["transforms"] = rig["frames"][0]["transforms"].clone();
        rig["frames"][4]["root"] = rig["frames"][0]["root"].clone();
        let output = fixture.validate("repeated-pose", &rig);
        assert_validation_failed(
            &output,
            "non-adjacent repeated pose",
            "repeats non-adjacent",
        );
    }

    #[test]
    fn bundled_rig_renderer_rejects_a_hip_declared_as_ground_contact() {
        let fixture = RigRendererFixture::new();
        let mut rig = good_biped_walk();
        for index in 0..8 {
            let side = if index <= 3 || index == 7 {
                "left"
            } else {
                "right"
            };
            rig["frames"][index]["contacts"] = json!([{
                "part": format!("{side}_upper"),
                "anchor": "hip",
                "state": "planted",
            }]);
        }
        let output = fixture.validate("hip-contact", &rig);
        assert_validation_failed(&output, "hip ground contact", "not on a lower-leg or foot");
    }

    #[test]
    fn bundled_rig_renderer_rejects_an_oversized_joint_cap() {
        let fixture = RigRendererFixture::new();
        let mut rig = good_biped_walk();
        rig["parts"][4]["mask"] = rig["parts"][3]["mask"].clone();
        rig["parts"][4]["overlapMode"] = json!("joint-cap");
        let output = fixture.validate("oversized-cap", &rig);
        assert_validation_failed(&output, "oversized joint cap", "bounded cap limit");
    }

    #[test]
    fn bundled_rig_renderer_rejects_a_duplicate_loop_endpoint() {
        let fixture = RigRendererFixture::new();
        let mut rig = good_biped_walk();
        let first_transforms = rig["frames"][0]["transforms"].clone();
        let first_root = rig["frames"][0]["root"].clone();
        let first_contacts = rig["frames"][0]["contacts"].clone();
        rig["frames"][7]["transforms"] = first_transforms;
        rig["frames"][7]["root"] = first_root;
        rig["frames"][7]["contacts"] = first_contacts;
        let output = fixture.validate("duplicate-endpoint", &rig);
        assert_validation_failed(
            &output,
            "duplicate loop endpoint",
            "duplicate loop endpoint",
        );
    }

    #[test]
    fn bundled_rig_renderer_rejects_in_place_root_drift() {
        let fixture = RigRendererFixture::new();
        let mut rig = good_biped_walk();
        for index in 0..8 {
            rig["frames"][index]["root"]["dx"] = json!(index);
        }
        let output = fixture.validate("root-drift", &rig);
        assert_validation_failed(&output, "in-place root drift", "in-place root drift");
    }

    #[test]
    fn bundled_rig_renderer_rejects_a_grounded_walk_frame_without_contact() {
        let fixture = RigRendererFixture::new();
        let mut rig = good_biped_walk();
        rig["frames"][2]["contacts"] = json!([]);
        let output = fixture.validate("missing-contact", &rig);
        assert_validation_failed(
            &output,
            "missing planted contact",
            "no planted support contact",
        );
    }

    #[test]
    fn bundled_rig_renderer_rejects_a_separated_child_joint() {
        let fixture = RigRendererFixture::new();
        let mut rig = good_biped_walk();
        rig["frames"][4]["transforms"]["left_lower"]["dx"] = json!(4);
        let output = fixture.validate("joint-separation", &rig);
        assert_validation_failed(&output, "separated child joint", "separates by");
    }

    #[test]
    fn bundled_rig_mcp_negotiates_legacy_and_current_protocols() {
        let fixture = RigRendererFixture::new();
        let mut mcp = RigMcpSession::new(&fixture.root);

        let before_initialize = mcp.request(&json!({
            "jsonrpc": "2.0",
            "id": "before-initialize",
            "method": "tools/list",
            "params": {},
        }));
        assert_eq!(before_initialize["error"]["code"], -32003);
        assert_eq!(
            before_initialize["error"]["message"],
            "MCP server is not initialized"
        );

        let malformed_initialize = mcp.request(&json!({
            "jsonrpc": "2.0",
            "id": "malformed-initialize",
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-11-25",
                "capabilities": {},
            },
        }));
        assert_eq!(malformed_initialize["error"]["code"], -32602);
        assert!(malformed_initialize["error"]["message"]
            .as_str()
            .expect("malformed initialize error should be text")
            .contains("clientInfo"));

        let initialized = mcp.request(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-11-25",
                "capabilities": {},
                "clientInfo": {"name": "sprite-studio-test", "version": "1"},
            },
        }));
        assert_eq!(initialized["id"], 1);
        assert_eq!(initialized["result"]["protocolVersion"], "2025-11-25");
        assert_eq!(
            initialized["result"]["capabilities"]["tools"]["listChanged"],
            false
        );
        assert!(initialized["result"]["instructions"]
            .as_str()
            .expect("server instructions should be text")
            .contains("sprite_rig_validate"));

        let before_notification = mcp.request(&json!({
            "jsonrpc": "2.0",
            "id": "before-initialized-notification",
            "method": "tools/list",
            "params": {},
        }));
        assert_eq!(before_notification["error"]["code"], -32003);

        let repeated_initialize = mcp.request(&json!({
            "jsonrpc": "2.0",
            "id": "repeated-initialize",
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-11-25",
                "capabilities": {},
                "clientInfo": {"name": "sprite-studio-test", "version": "1"},
            },
        }));
        assert_eq!(repeated_initialize["error"]["code"], -32600);
        assert_eq!(
            repeated_initialize["error"]["message"],
            "server is already initialized"
        );

        mcp.send(&json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
        }));
        let listed = mcp.request(&json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list",
            "params": {},
        }));
        assert_eq!(listed["id"], 2, "notifications must not receive responses");
        let tools = listed["result"]["tools"]
            .as_array()
            .expect("tools/list should return an array");
        let names = tools
            .iter()
            .map(|tool| tool["name"].as_str().expect("tool name should be text"))
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            [
                "sprite_rig_validate",
                "sprite_rig_render",
                "sprite_rig_analyze_motion",
                "sprite_rig_analyze_walk",
            ]
        );
        assert_eq!(tools[0]["annotations"]["readOnlyHint"], true);
        assert_eq!(tools[1]["annotations"]["readOnlyHint"], false);
        assert_eq!(
            tools[1]["annotations"]["idempotentHint"], false,
            "render reruns archive prior frames and are not idempotent"
        );
        assert_eq!(tools[2]["annotations"]["readOnlyHint"], true);
        assert_eq!(tools[3]["annotations"]["readOnlyHint"], true);
        assert!(tools.iter().all(|tool| {
            tool["annotations"]["destructiveHint"] == false
                && tool["annotations"]["openWorldHint"] == false
        }));

        let discovered = mcp.request(&json!({
            "jsonrpc": "2.0",
            "id": "discover",
            "method": "server/discover",
            "params": {
                "_meta": {
                    "io.modelcontextprotocol/protocolVersion": "2026-07-28",
                    "io.modelcontextprotocol/clientInfo": {
                        "name": "sprite-studio-test",
                        "version": "1",
                    },
                    "io.modelcontextprotocol/clientCapabilities": {},
                },
            },
        }));
        assert_eq!(discovered["id"], "discover");
        assert_eq!(discovered["result"]["resultType"], "complete");
        assert!(discovered["result"]["supportedVersions"]
            .as_array()
            .expect("discovery should list supported protocol versions")
            .contains(&json!("2026-07-28")));
        assert_eq!(discovered["result"]["cacheScope"], "public");
        assert!(
            discovered["result"]["ttlMs"]
                .as_u64()
                .expect("discovery should provide a nonnegative TTL")
                > 0
        );
        let modern_list = mcp.request(&json!({
            "jsonrpc": "2.0",
            "id": "modern-list",
            "method": "tools/list",
            "params": {
                "_meta": {
                    "io.modelcontextprotocol/protocolVersion": "2026-07-28",
                    "io.modelcontextprotocol/clientCapabilities": {},
                },
            },
        }));
        assert_eq!(modern_list["result"]["resultType"], "complete");
        assert_eq!(modern_list["result"]["cacheScope"], "public");
        assert_eq!(
            modern_list["result"]["_meta"]["io.modelcontextprotocol/serverInfo"]["name"],
            "sprite-studio-rig"
        );
        let unsupported = mcp.request(&json!({
            "jsonrpc": "2.0",
            "id": "unsupported-version",
            "method": "ping",
            "params": {
                "_meta": {
                    "io.modelcontextprotocol/protocolVersion": "2099-01-01",
                    "io.modelcontextprotocol/clientCapabilities": {},
                },
            },
        }));
        assert_eq!(unsupported["error"]["code"], -32022);
        assert_eq!(unsupported["error"]["data"]["requested"], "2099-01-01");
        assert!(unsupported["error"]["data"]["supported"]
            .as_array()
            .expect("unsupported-version error should list supported versions")
            .contains(&json!("2026-07-28")));

        mcp.send_raw(b"{");
        let parse_error = mcp.read_response();
        assert_eq!(parse_error["error"]["code"], -32700);
        let ping = mcp.request(&json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "ping",
            "params": {},
        }));
        assert_eq!(ping["id"], 3, "server should recover after malformed input");
        assert!(ping["result"].is_object());
        mcp.send_raw(br#"{"jsonrpc":"2.0","id":"nan","method":"ping","params":{"value":NaN}}"#);
        let nonfinite = mcp.read_response();
        assert_eq!(nonfinite["error"]["code"], -32600);
        let ping_after_nonfinite = mcp.request(&json!({
            "jsonrpc": "2.0",
            "id": 4,
            "method": "ping",
            "params": {},
        }));
        assert_eq!(
            ping_after_nonfinite["id"], 4,
            "server should recover after a non-finite JSON constant"
        );
    }

    #[test]
    fn bundled_rig_mcp_exercises_real_tools_and_confines_rig_paths() {
        let fixture = RigRendererFixture::new();
        let (rig_path, original_bytes) = fixture.write_rig("mcp-ik-walk", &ik_biped_walk());
        let mut mcp = RigMcpSession::new(&fixture.root);
        let initialized = mcp.request(&json!({
            "jsonrpc": "2.0",
            "id": 0,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-11-25",
                "capabilities": {},
                "clientInfo": {"name": "sprite-studio-test", "version": "1"},
            },
        }));
        assert_eq!(initialized["result"]["protocolVersion"], "2025-11-25");
        mcp.send(&json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
        }));
        let call = |id: u64, name: &str, rig: &str| {
            json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": "tools/call",
                "params": {
                    "name": name,
                    "arguments": {"rig": rig},
                },
            })
        };
        let relative_rig = ".sprite-studio/rigs/mcp-ik-walk.json";

        let validated = mcp.request(&call(1, "sprite_rig_validate", relative_rig));
        assert_eq!(validated["result"]["isError"], false);
        assert_eq!(validated["result"]["structuredContent"]["valid"], true);
        assert_eq!(validated["result"]["structuredContent"]["rigVersion"], 2);
        assert_eq!(
            std::fs::read(&rig_path).expect("validated rig should remain readable"),
            original_bytes,
            "MCP validation must use the non-mutating --check path"
        );
        assert!(!fixture
            .root
            .join(".sprite-studio/last-generation.json")
            .exists());
        assert!(!fixture.root.join(".sprite-studio/versions").exists());
        assert!(!fixture
            .root
            .join("assets/characters/regression_biped_walk_01.png")
            .exists());

        let forged_manifest = b"{\"analysis\":{\"phases\":[\"forged\"]}}\n";
        let manifest_path = fixture.root.join(".sprite-studio/last-generation.json");
        std::fs::write(&manifest_path, forged_manifest)
            .expect("forged stale manifest should write");
        let analyzed = mcp.request(&call(2, "sprite_rig_analyze_motion", relative_rig));
        assert_eq!(analyzed["result"]["isError"], false);
        let analysis = &analyzed["result"]["structuredContent"];
        assert_eq!(analysis["frameCount"], 8);
        assert_eq!(analysis["rootMotion"], "in-place");
        assert_eq!(analysis["phases"].as_array().map(Vec::len), Some(8));
        assert_eq!(
            analysis["transitionEnergyPx"].as_array().map(Vec::len),
            Some(8),
            "a looping eight-frame gait should include the solved loop seam"
        );
        assert!(analysis["plantedContactGroups"]
            .as_object()
            .expect("analysis should group solved contacts")
            .contains_key("left_lower.foot"));
        let left_contacts = analysis["plantedContactGroups"]["left_lower.foot"]
            .as_array()
            .expect("left planted contacts should be an array");
        assert!(left_contacts
            .iter()
            .all(|contact| contact["x"] == 11.0 && contact["y"] == 26.0));
        let is_sha256 = |value: &Value| {
            value.as_str().is_some_and(|hash| {
                hash.len() == 64
                    && hash
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            })
        };
        assert!(is_sha256(&analysis["masterHash"]));
        assert!(is_sha256(&analysis["rigHash"]));
        let analyzed_frame_hashes = analysis["renderedFrameHashes"]
            .as_array()
            .expect("analysis should return ordered rendered-frame hashes");
        assert_eq!(analyzed_frame_hashes.len(), 8);
        assert!(analyzed_frame_hashes.iter().all(is_sha256));
        assert_eq!(
            analyzed_frame_hashes
                .iter()
                .map(|hash| hash.as_str().expect("frame hash should be text"))
                .collect::<HashSet<_>>()
                .len(),
            8,
            "the accepted walk should have eight unique rendered frames"
        );
        let analyzed_master_hash = analysis["masterHash"].clone();
        let analyzed_rig_hash = analysis["rigHash"].clone();
        let analyzed_frame_hashes = analysis["renderedFrameHashes"].clone();
        assert_eq!(
            std::fs::read(&rig_path).expect("analyzed rig should remain readable"),
            original_bytes,
            "walk analysis must not normalize the user's rig"
        );
        assert_eq!(
            std::fs::read(&manifest_path).expect("stale manifest should remain readable"),
            forged_manifest,
            "walk analysis must come from the current rig, not mutate or trust a manifest"
        );
        assert!(!fixture.root.join(".sprite-studio/versions").exists());
        assert!(!fixture
            .root
            .join("assets/characters/regression_biped_walk_01.png")
            .exists());

        #[cfg(unix)]
        {
            let output_frame = fixture
                .root
                .join("assets/characters/regression_biped_walk_01.png");
            std::os::unix::fs::symlink(
                fixture.root.join("missing-external-frame.png"),
                &output_frame,
            )
            .expect("broken output-frame symlink should be created");
            let rejected_output = mcp.request(&call(3, "sprite_rig_render", relative_rig));
            assert_eq!(rejected_output["result"]["isError"], true);
            assert!(rejected_output["result"]["structuredContent"]["error"]
                .as_str()
                .expect("output symlink rejection should be text")
                .contains("output frame 1 cannot be a symbolic link"));
            std::fs::remove_file(output_frame)
                .expect("broken output-frame symlink should be removable");
        }

        let rendered = mcp.request(&call(4, "sprite_rig_render", relative_rig));
        assert_eq!(rendered["result"]["isError"], false);
        let manifest = &rendered["result"]["structuredContent"];
        let files = manifest["files"]
            .as_array()
            .expect("render should return output files");
        assert_eq!(files.len(), 8);
        assert_eq!(manifest["masterHash"], analyzed_master_hash);
        assert_eq!(manifest["rigHash"], analyzed_rig_hash);
        assert_eq!(
            manifest["quality"]["renderedFrameHashes"], analyzed_frame_hashes,
            "analysis and render must identify the same ordered frame sequence"
        );
        assert!(files.iter().all(|file| fixture
            .root
            .join(file.as_str().expect("rendered file should be text"))
            .is_file()));

        let mut invalid = good_biped_walk();
        invalid["frames"][2]["contacts"] = json!([]);
        fixture.write_rig("mcp-invalid", &invalid);
        let rejected = mcp.request(&call(
            5,
            "sprite_rig_validate",
            ".sprite-studio/rigs/mcp-invalid.json",
        ));
        assert_eq!(rejected["result"]["isError"], true);
        assert_eq!(rejected["result"]["structuredContent"]["ok"], false);
        assert!(rejected["result"]["structuredContent"]["error"]
            .as_str()
            .expect("validation error should be text")
            .contains("no planted support contact"));

        let escaped_path = fixture.root.join("outside-rig.json");
        std::fs::write(&escaped_path, b"{}\n").expect("outside fixture should write");
        let escaped = mcp.request(&call(6, "sprite_rig_validate", "outside-rig.json"));
        assert_eq!(escaped["result"]["isError"], true);
        assert!(escaped["result"]["structuredContent"]["error"]
            .as_str()
            .expect("path error should be text")
            .contains("must stay under .sprite-studio/rigs"));

        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(
                &escaped_path,
                fixture.root.join(".sprite-studio/rigs/symlinked-rig.json"),
            )
            .expect("rig symlink should be created");
            let symlinked = mcp.request(&call(
                7,
                "sprite_rig_validate",
                ".sprite-studio/rigs/symlinked-rig.json",
            ));
            assert_eq!(symlinked["result"]["isError"], true);
            assert!(symlinked["result"]["structuredContent"]["error"]
                .as_str()
                .expect("symlink escape should be text")
                .contains("must stay under .sprite-studio/rigs"));
        }
    }

    #[test]
    fn bundled_rig_mcp_rejects_invalid_fps_without_creating_artifacts() {
        let fixture = RigRendererFixture::new();
        let cases = [
            ("text", json!("bad")),
            ("boolean", json!(true)),
            ("zero", json!(0)),
            ("above-limit", json!(61)),
        ];
        let mut mcp = RigMcpSession::new(&fixture.root);
        mcp.initialize_legacy();

        for (index, (case, fps)) in cases.into_iter().enumerate() {
            let mut rig = good_biped_walk();
            rig["name"] = json!(format!("invalid_fps_{case}"));
            rig["fps"] = fps;
            let rig_name = format!("invalid-fps-{case}");
            let (rig_path, rig_bytes) = fixture.write_rig(&rig_name, &rig);
            let response = mcp.call_tool(
                index as u64 + 1,
                "sprite_rig_validate",
                &format!(".sprite-studio/rigs/{rig_name}.json"),
            );
            assert_eq!(
                response["result"]["isError"], true,
                "invalid fps case {case} must fail validation: {response}"
            );
            assert!(response["result"]["structuredContent"]["error"]
                .as_str()
                .expect("fps validation error should be text")
                .contains("fps"));
            assert_eq!(
                std::fs::read(&rig_path).expect("invalid rig should remain readable"),
                rig_bytes,
                "invalid fps validation must not rewrite the rig"
            );
            assert!(!fixture
                .root
                .join(format!("assets/characters/invalid_fps_{case}_01.png"))
                .exists());
        }
        assert!(!fixture
            .root
            .join(".sprite-studio/last-generation.json")
            .exists());
        assert!(!fixture.root.join(".sprite-studio/versions").exists());
    }

    #[test]
    fn bundled_rig_mcp_rejects_nonfinite_json_and_invalid_master_hashes_without_artifacts() {
        let fixture = RigRendererFixture::new();
        let mut cases = vec![("top-level-nan", b"NaN\n".to_vec(), None)];
        for (case, master_hash) in [
            ("false", json!(false)),
            ("empty", json!("")),
            ("malformed", json!("not-a-sha256")),
        ] {
            let mut rig = good_biped_walk();
            let output_name = format!("invalid_master_hash_{case}");
            rig["name"] = json!(output_name.clone());
            rig["masterHash"] = master_hash;
            cases.push((
                case,
                serde_json::to_vec(&rig).expect("invalid hash rig should serialize"),
                Some(output_name),
            ));
        }

        let mut mcp = RigMcpSession::new(&fixture.root);
        mcp.initialize_legacy();
        for (index, (case, rig_bytes, output_name)) in cases.into_iter().enumerate() {
            let rig_name = format!("invalid-master-{case}");
            let rig_path = fixture
                .root
                .join(format!(".sprite-studio/rigs/{rig_name}.json"));
            std::fs::write(&rig_path, &rig_bytes).expect("invalid rig should write");

            let response = mcp.call_tool(
                index as u64 + 1,
                "sprite_rig_validate",
                &format!(".sprite-studio/rigs/{rig_name}.json"),
            );
            assert_eq!(
                response["result"]["isError"], true,
                "invalid {case} case must fail validation: {response}"
            );
            let diagnostic = response["result"]["structuredContent"]["error"]
                .as_str()
                .expect("validation diagnostic should be text");
            assert!(
                diagnostic.starts_with("sprite_rig: "),
                "invalid {case} should return a clean renderer diagnostic: {diagnostic}"
            );
            assert!(
                !diagnostic.contains("Traceback") && !diagnostic.contains('\n'),
                "invalid {case} should not expose a Python traceback: {diagnostic}"
            );
            if case == "top-level-nan" {
                assert!(diagnostic.contains("non-finite JSON constant"));
            } else {
                assert!(diagnostic.contains("masterHash"));
            }
            assert_eq!(
                std::fs::read(&rig_path).expect("invalid rig should remain readable"),
                rig_bytes,
                "invalid {case} validation must not rewrite the rig"
            );
            if let Some(output_name) = output_name {
                assert!(!fixture
                    .root
                    .join(format!("assets/characters/{output_name}_01.png"))
                    .exists());
            }
        }

        let asset_names = std::fs::read_dir(fixture.root.join("assets/characters"))
            .expect("character assets should remain readable")
            .map(|entry| {
                entry
                    .expect("asset directory entry should be readable")
                    .file_name()
                    .into_string()
                    .expect("test asset names should be UTF-8")
            })
            .collect::<HashSet<_>>();
        assert_eq!(asset_names, HashSet::from(["walker.png".to_string()]));
        assert!(!fixture
            .root
            .join(".sprite-studio/last-generation.json")
            .exists());
        assert!(!fixture.root.join(".sprite-studio/versions").exists());
        assert!(!fixture.root.join(".sprite-studio/transactions").exists());
    }

    #[test]
    fn bundled_rig_mcp_rejects_invalid_v1_colors_without_creating_artifacts() {
        let fixture = RigRendererFixture::new();
        let cases = [
            {
                let mut rig = legacy_color_rig();
                rig["palette"]["accent"] = json!("not-a-color");
                ("palette", rig)
            },
            {
                let mut rig = legacy_color_rig();
                rig["frames"][0]["overlay"][0]["color"] = json!("#gg0000");
                ("command", rig)
            },
        ];
        let mut mcp = RigMcpSession::new(&fixture.root);
        mcp.initialize_legacy();

        for (index, (case, mut rig)) in cases.into_iter().enumerate() {
            rig["name"] = json!(format!("invalid_v1_{case}"));
            let rig_name = format!("invalid-v1-{case}");
            let (rig_path, rig_bytes) = fixture.write_rig(&rig_name, &rig);
            let response = mcp.call_tool(
                index as u64 + 1,
                "sprite_rig_validate",
                &format!(".sprite-studio/rigs/{rig_name}.json"),
            );
            assert_eq!(
                response["result"]["isError"], true,
                "invalid v1 {case} color must fail validation: {response}"
            );
            assert!(response["result"]["structuredContent"]["error"]
                .as_str()
                .expect("color validation error should be text")
                .contains("color"));
            assert_eq!(
                std::fs::read(&rig_path).expect("invalid v1 rig should remain readable"),
                rig_bytes,
                "invalid v1 color validation must not rewrite the rig"
            );
            assert!(!fixture
                .root
                .join(format!("assets/characters/invalid_v1_{case}_01.png"))
                .exists());
        }
        assert!(!fixture
            .root
            .join(".sprite-studio/last-generation.json")
            .exists());
        assert!(!fixture.root.join(".sprite-studio/versions").exists());
    }

    #[test]
    fn bundled_rig_mcp_preflights_every_output_before_changing_active_state() {
        let fixture = RigRendererFixture::new();
        let mut mcp = RigMcpSession::new(&fixture.root);
        mcp.initialize_legacy();

        let mut active_rig = ik_biped_walk();
        active_rig["name"] = json!("prior_active_walk");
        fixture.write_rig("prior-active-walk", &active_rig);
        let activated = mcp.call_tool(
            1,
            "sprite_rig_render",
            ".sprite-studio/rigs/prior-active-walk.json",
        );
        assert_eq!(activated["result"]["isError"], false);
        let active_files = activated["result"]["structuredContent"]["files"]
            .as_array()
            .expect("initial render should list active files")
            .iter()
            .map(|value| {
                fixture
                    .root
                    .join(value.as_str().expect("active file should be text"))
            })
            .collect::<Vec<_>>();
        let active_bytes = active_files
            .iter()
            .map(|path| std::fs::read(path).expect("active frame should be readable"))
            .collect::<Vec<_>>();
        let manifest_path = fixture.root.join(".sprite-studio/last-generation.json");
        let manifest_bytes =
            std::fs::read(&manifest_path).expect("active manifest should be readable");

        let mut colliding_rig = ik_biped_walk();
        colliding_rig["name"] = json!("collision_candidate");
        fixture.write_rig("collision-candidate", &colliding_rig);
        let collision = fixture
            .root
            .join("assets/characters/collision_candidate_02.png");
        std::fs::create_dir(&collision).expect("frame-two collision directory should exist");
        let rejected = mcp.call_tool(
            2,
            "sprite_rig_render",
            ".sprite-studio/rigs/collision-candidate.json",
        );
        assert_eq!(rejected["result"]["isError"], true);
        assert!(rejected["result"]["structuredContent"]["error"]
            .as_str()
            .expect("collision error should be text")
            .contains("output frame 2 must be a regular file"));

        assert!(!fixture
            .root
            .join("assets/characters/collision_candidate_01.png")
            .exists());
        assert!(collision.is_dir());
        assert!(!fixture
            .root
            .join("assets/characters/collision_candidate_03.png")
            .exists());
        assert_eq!(
            std::fs::read(&manifest_path).expect("prior manifest should survive"),
            manifest_bytes,
            "preflight failure must preserve the prior active manifest"
        );
        for (path, expected) in active_files.iter().zip(active_bytes) {
            assert_eq!(
                std::fs::read(path).expect("prior active frame should survive"),
                expected,
                "preflight failure must preserve every prior active frame"
            );
        }
        assert!(!fixture.root.join(".sprite-studio/versions").exists());
    }

    #[test]
    fn bundled_rig_mcp_rejects_a_forged_extra_active_frame_without_mutation() {
        let fixture = RigRendererFixture::new();
        let mut mcp = RigMcpSession::new(&fixture.root);
        mcp.initialize_legacy();

        let mut rig = ik_biped_walk();
        rig["name"] = json!("forged_manifest_walk");
        let (rig_path, _) = fixture.write_rig("forged-manifest-walk", &rig);
        let first = mcp.call_tool(
            1,
            "sprite_rig_render",
            ".sprite-studio/rigs/forged-manifest-walk.json",
        );
        assert_eq!(first["result"]["isError"], false);
        let active_paths = first["result"]["structuredContent"]["files"]
            .as_array()
            .expect("initial render should list active files")
            .iter()
            .map(|value| {
                fixture
                    .root
                    .join(value.as_str().expect("active frame should be text"))
            })
            .collect::<Vec<_>>();
        let active_bytes = active_paths
            .iter()
            .map(|path| std::fs::read(path).expect("active frame should be readable"))
            .collect::<Vec<_>>();

        let manifest_path = fixture.root.join(".sprite-studio/last-generation.json");
        let mut forged_manifest: Value = serde_json::from_slice(
            &std::fs::read(&manifest_path).expect("active manifest should be readable"),
        )
        .expect("active manifest should be JSON");
        forged_manifest["files"]
            .as_array_mut()
            .expect("active manifest files should be an array")
            .push(json!("assets/characters/forged_manifest_walk_99.png"));
        let forged_manifest_bytes =
            serde_json::to_vec_pretty(&forged_manifest).expect("forged manifest should serialize");
        std::fs::write(&manifest_path, &forged_manifest_bytes)
            .expect("forged manifest should write");
        let sentinel_path = fixture
            .root
            .join("assets/characters/forged_manifest_walk_99.png");
        let sentinel_bytes = b"forged frame-list sentinel".to_vec();
        std::fs::write(&sentinel_path, &sentinel_bytes).expect("sentinel should write");
        let active_rig_bytes =
            std::fs::read(&rig_path).expect("active normalized rig should be readable");

        let rejected = mcp.call_tool(
            2,
            "sprite_rig_render",
            ".sprite-studio/rigs/forged-manifest-walk.json",
        );
        assert_eq!(rejected["result"]["isError"], true);
        let diagnostic = rejected["result"]["structuredContent"]["error"]
            .as_str()
            .expect("forged-manifest error should be text");
        assert!(
            diagnostic
                .contains("active rig manifest frames must be the exact ordered output sequence"),
            "forged manifest should fail exact sequence validation: {diagnostic}"
        );
        assert!(!diagnostic.contains("Traceback"));

        assert_eq!(
            std::fs::read(&sentinel_path).expect("sentinel should survive rejection"),
            sentinel_bytes,
            "the unowned sentinel must not be archived or replaced"
        );
        assert_eq!(
            std::fs::read(&manifest_path).expect("forged manifest should survive rejection"),
            forged_manifest_bytes,
            "manifest validation failure must not rewrite active metadata"
        );
        assert_eq!(
            std::fs::read(&rig_path).expect("active rig should survive rejection"),
            active_rig_bytes,
            "manifest validation failure must not rewrite the active rig"
        );
        for (path, expected) in active_paths.iter().zip(active_bytes) {
            assert_eq!(
                std::fs::read(path).expect("active frame should survive rejection"),
                expected,
                "manifest validation failure must preserve every active frame"
            );
        }
        assert!(!fixture.root.join(".sprite-studio/versions").exists());
        assert_eq!(
            std::fs::read_dir(fixture.root.join(".sprite-studio/transactions"))
                .expect("transaction root should remain readable")
                .count(),
            0,
            "forged manifest rejection must not retain a transaction"
        );
    }

    #[test]
    fn bundled_rig_mcp_rerender_archives_the_exact_prior_active_frames() {
        let fixture = RigRendererFixture::new();
        let mut mcp = RigMcpSession::new(&fixture.root);
        mcp.initialize_legacy();

        let mut rig = ik_biped_walk();
        rig["name"] = json!("archive_walk");
        let (rig_path, _) = fixture.write_rig("archive-walk", &rig);
        let first = mcp.call_tool(
            1,
            "sprite_rig_render",
            ".sprite-studio/rigs/archive-walk.json",
        );
        assert_eq!(first["result"]["isError"], false);
        let first_manifest = &first["result"]["structuredContent"];
        let prior_frames = first_manifest["files"]
            .as_array()
            .expect("first render should list files")
            .iter()
            .map(|value| {
                let relative = value.as_str().expect("rendered file should be text");
                let path = fixture.root.join(relative);
                let name = path
                    .file_name()
                    .and_then(|value| value.to_str())
                    .expect("rendered frame should have a UTF-8 name")
                    .to_string();
                let bytes = std::fs::read(path).expect("prior frame should be readable");
                (name, bytes)
            })
            .collect::<HashMap<_, _>>();
        assert_eq!(prior_frames.len(), 8);

        let mut revised: Value = serde_json::from_slice(
            &std::fs::read(&rig_path).expect("normalized rig should be readable"),
        )
        .expect("normalized rig should remain JSON");
        revised["fps"] = json!(10);
        std::fs::write(
            &rig_path,
            serde_json::to_vec_pretty(&revised).expect("revised rig should serialize"),
        )
        .expect("revised rig should write");
        let second = mcp.call_tool(
            2,
            "sprite_rig_render",
            ".sprite-studio/rigs/archive-walk.json",
        );
        assert_eq!(second["result"]["isError"], false);
        let second_manifest = &second["result"]["structuredContent"];
        assert_eq!(second_manifest["fps"], 10);
        assert_ne!(second_manifest["rigHash"], first_manifest["rigHash"]);

        let manifest_path = fixture.root.join(".sprite-studio/last-generation.json");
        let active_manifest: Value = serde_json::from_slice(
            &std::fs::read(&manifest_path).expect("active manifest should be readable"),
        )
        .expect("active manifest should remain JSON");
        assert_eq!(&active_manifest, second_manifest);
        let active_files = active_manifest["files"]
            .as_array()
            .expect("active manifest should list files");
        assert_eq!(active_files.len(), 8);
        assert_eq!(
            active_manifest["quality"]["renderedFrameHashes"]
                .as_array()
                .map(Vec::len),
            Some(8)
        );
        assert!(active_files.iter().all(|value| fixture
            .root
            .join(value.as_str().expect("active file should be text"))
            .is_file()));

        let archive_parent = fixture
            .root
            .join(".sprite-studio/versions/rigs/archive_walk");
        let archives = std::fs::read_dir(&archive_parent)
            .expect("rerender should create an archive directory")
            .map(|entry| entry.expect("archive entry should be readable").path())
            .collect::<Vec<_>>();
        assert_eq!(archives.len(), 1, "one rerender should create one archive");
        assert!(archives[0].is_dir());
        let archived_frames = std::fs::read_dir(&archives[0])
            .expect("archive should list prior frames")
            .map(|entry| {
                let path = entry.expect("archived frame should be readable").path();
                let name = path
                    .file_name()
                    .and_then(|value| value.to_str())
                    .expect("archived frame should have a UTF-8 name")
                    .to_string();
                let bytes = std::fs::read(path).expect("archived frame should be readable");
                (name, bytes)
            })
            .collect::<HashMap<_, _>>();
        assert_eq!(archived_frames, prior_frames);

        let transactions = fixture.root.join(".sprite-studio/transactions");
        assert_eq!(
            std::fs::read_dir(transactions)
                .expect("transaction root should be readable")
                .count(),
            0,
            "a successful rerender must clean its transaction journal"
        );
    }

    #[test]
    fn bundled_rig_renderer_rolls_back_post_replace_interrupts() {
        let fixture = RigRendererFixture::new();
        let mut rig = ik_biped_walk();
        rig["name"] = json!("rollback_walk");
        let first = fixture.render("rollback-walk", &rig);
        assert!(
            first.status.success(),
            "baseline rollback fixture should render: {}",
            String::from_utf8_lossy(&first.stderr)
        );
        let first_manifest: Value = serde_json::from_slice(&first.stdout)
            .expect("baseline render should return a manifest");
        let active_frames = first_manifest["files"]
            .as_array()
            .expect("baseline manifest should list active frames")
            .iter()
            .map(|value| {
                fixture
                    .root
                    .join(value.as_str().expect("frame should be text"))
            })
            .collect::<Vec<_>>();
        let active_bytes = active_frames
            .iter()
            .map(|path| std::fs::read(path).expect("baseline frame should be readable"))
            .collect::<Vec<_>>();
        let manifest_path = fixture.root.join(".sprite-studio/last-generation.json");
        let manifest_bytes =
            std::fs::read(&manifest_path).expect("baseline manifest should be readable");
        let rig_path = fixture.root.join(".sprite-studio/rigs/rollback-walk.json");

        let mut revised: Value = serde_json::from_slice(
            &std::fs::read(&rig_path).expect("normalized rollback rig should be readable"),
        )
        .expect("normalized rollback rig should remain JSON");
        revised["fps"] = json!(10);
        for index in 0..8 {
            let dx = revised["frames"][index]["transforms"]["torso"]["dx"]
                .as_i64()
                .expect("torso dx should be an integer");
            revised["frames"][index]["transforms"]["torso"]["dx"] = json!(dx + 1);
        }
        let revised_rig_bytes =
            serde_json::to_vec_pretty(&revised).expect("revised rollback rig should serialize");
        std::fs::write(&rig_path, &revised_rig_bytes).expect("revised rollback rig should write");

        let injection = r#"
import importlib.util
import sys
from pathlib import Path

failure_kind = sys.argv[1]
workspace = Path.cwd().resolve()
engine = workspace / ".sprite-studio" / "sprite_rig.py"
sys.path.insert(0, str(engine.parent))
spec = importlib.util.spec_from_file_location("sprite_rig_injected_rollback", engine)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)

failure_targets = {
    "frame-1": workspace / "assets" / "characters" / "rollback_walk_01.png",
    "spec": workspace / ".sprite-studio" / "rigs" / "rollback-walk.json",
    "manifest": workspace / ".sprite-studio" / "last-generation.json",
    "rollback-interrupted": workspace / "assets" / "characters" / "rollback_walk_01.png",
}
failure_target = failure_targets[failure_kind]
original_replace = module.os.replace
state = {"target_calls": 0}

def injected_replace(source, destination):
    target = Path(destination).resolve()
    if target == failure_target:
        state["target_calls"] += 1
        if state["target_calls"] == 1:
            original_replace(source, destination)
            raise OSError(f"injected post-replace {failure_kind} failure")
        if failure_kind == "rollback-interrupted" and state["target_calls"] == 2:
            raise OSError("injected rollback restore interruption")
    return original_replace(source, destination)

module.os.replace = injected_replace
sys.argv = [str(engine), ".sprite-studio/rigs/rollback-walk.json"]
module.main()
"#;
        for failure_kind in ["frame-1", "spec", "manifest"] {
            let failed = Command::new("python3")
                .current_dir(&fixture.root)
                .arg("-c")
                .arg(injection)
                .arg(failure_kind)
                .output()
                .expect("python3 should execute the injected rollback render");
            assert!(
                !failed.status.success(),
                "post-replace {failure_kind} interrupt must fail"
            );
            let diagnostic = String::from_utf8_lossy(&failed.stderr);
            assert!(
                diagnostic.contains("render commit failed before activation and was rolled back")
                    && diagnostic
                        .contains(&format!("injected post-replace {failure_kind} failure")),
                "renderer should report a clean {failure_kind} rollback: {diagnostic}"
            );
            assert!(!diagnostic.contains("Traceback"));

            for ((path, expected), index) in active_frames.iter().zip(&active_bytes).zip(1..) {
                assert_eq!(
                    std::fs::read(path).expect("rolled-back frame should remain readable"),
                    *expected,
                    "active frame {index} must be byte-identical after {failure_kind} rollback"
                );
            }
            assert_eq!(
                std::fs::read(&manifest_path).expect("rolled-back manifest should remain readable"),
                manifest_bytes,
                "{failure_kind} rollback must restore the prior active manifest"
            );
            assert_eq!(
                std::fs::read(&rig_path).expect("rolled-back rig should remain readable"),
                revised_rig_bytes,
                "{failure_kind} rollback must restore the exact pre-render rig revision"
            );
            assert!(!fixture.root.join(".sprite-studio/versions").exists());
            assert_eq!(
                std::fs::read_dir(fixture.root.join(".sprite-studio/transactions"))
                    .expect("transaction root should remain readable")
                    .count(),
                0,
                "successful {failure_kind} rollback must clean its transaction journal"
            );
            assert_eq!(
                std::fs::read_dir(fixture.root.join("assets/characters"))
                    .expect("output directory should remain readable")
                    .filter_map(Result::ok)
                    .filter(|entry| entry
                        .file_name()
                        .to_string_lossy()
                        .starts_with(".rollback_walk-stage-"))
                    .count(),
                0,
                "successful {failure_kind} rollback must remove staged frame directories"
            );
        }

        let interrupted = Command::new("python3")
            .current_dir(&fixture.root)
            .arg("-c")
            .arg(injection)
            .arg("rollback-interrupted")
            .output()
            .expect("python3 should execute the interrupted rollback render");
        assert!(!interrupted.status.success());
        let diagnostic = String::from_utf8_lossy(&interrupted.stderr);
        assert!(
            diagnostic.contains("render commit failed and rollback needs manual recovery")
                && diagnostic.contains("injected rollback restore interruption"),
            "interrupted rollback should identify retained recovery state: {diagnostic}"
        );
        assert!(!diagnostic.contains("Traceback"));

        let transactions = fixture.root.join(".sprite-studio/transactions");
        let retained = std::fs::read_dir(&transactions)
            .expect("transaction root should remain readable")
            .map(|entry| {
                entry
                    .expect("retained transaction should be readable")
                    .path()
            })
            .collect::<Vec<_>>();
        assert_eq!(
            retained.len(),
            1,
            "an interrupted rollback must retain exactly its recovery transaction"
        );
        let journal: Value = serde_json::from_slice(
            &std::fs::read(retained[0].join("journal.json"))
                .expect("recovery journal should be readable"),
        )
        .expect("recovery journal should remain JSON");
        assert_eq!(journal["state"], "committing");
        let frame_stage = journal["frameStage"]
            .as_str()
            .expect("recovery journal should identify its frame stage");
        assert!(
            !fixture.root.join(frame_stage).exists(),
            "failed activation should still remove expendable staged frames"
        );

        let retained_backups = std::fs::read_dir(retained[0].join("backups/frames"))
            .expect("recovery transaction should retain frame backups")
            .map(|entry| {
                let path = entry.expect("frame backup should be readable").path();
                let name = path
                    .file_name()
                    .and_then(|value| value.to_str())
                    .expect("frame backup should have a UTF-8 name")
                    .to_string();
                let bytes = std::fs::read(path).expect("frame backup should be readable");
                (name, bytes)
            })
            .collect::<HashMap<_, _>>();
        assert_eq!(retained_backups.len(), 8);
        for (path, expected) in active_frames.iter().zip(&active_bytes) {
            let name = path
                .file_name()
                .and_then(|value| value.to_str())
                .expect("active frame should have a UTF-8 name");
            assert_eq!(
                retained_backups.get(name),
                Some(expected),
                "manual recovery must retain the exact prior bytes for {name}"
            );
        }
        assert_eq!(
            std::fs::read(&manifest_path).expect("prior manifest should remain readable"),
            manifest_bytes,
            "rollback should restore the manifest even when one frame restore is interrupted"
        );
        assert_eq!(
            std::fs::read(&rig_path).expect("prior rig should remain readable"),
            revised_rig_bytes,
            "rollback should restore the rig even when one frame restore is interrupted"
        );
        assert_ne!(
            std::fs::read(&active_frames[0]).expect("partially activated frame should exist"),
            active_bytes[0],
            "the retained transaction must correspond to a real unresolved frame mutation"
        );
    }

    #[test]
    fn bundled_rig_mcp_rejects_an_output_that_aliases_its_source_master() {
        let fixture = RigRendererFixture::new();
        let source = fixture.root.join("assets/characters/source_alias_01.png");
        std::fs::copy(fixture.root.join("assets/characters/walker.png"), &source)
            .expect("aliased source fixture should copy");
        let source_bytes = std::fs::read(&source).expect("aliased source should be readable");

        let mut rig = ik_biped_walk();
        rig["name"] = json!("source_alias");
        rig["source"] = json!("assets/characters/source_alias_01.png");
        fixture.write_rig("source-alias", &rig);
        let mut mcp = RigMcpSession::new(&fixture.root);
        mcp.initialize_legacy();
        let rejected = mcp.call_tool(
            1,
            "sprite_rig_render",
            ".sprite-studio/rigs/source-alias.json",
        );
        assert_eq!(rejected["result"]["isError"], true);
        assert!(rejected["result"]["structuredContent"]["error"]
            .as_str()
            .expect("source alias rejection should be text")
            .contains("cannot replace the locked source master"));
        assert_eq!(
            std::fs::read(&source).expect("source should survive alias rejection"),
            source_bytes
        );
        assert!(!fixture
            .root
            .join("assets/characters/source_alias_02.png")
            .exists());
        assert!(!fixture
            .root
            .join(".sprite-studio/last-generation.json")
            .exists());
        assert!(!fixture.root.join(".sprite-studio/versions").exists());
    }

    #[test]
    fn bundled_rig_renderer_rejects_a_source_swap_after_decoding() {
        let fixture = RigRendererFixture::new();
        let source_path = fixture.root.join("assets/characters/walker.png");
        let original_source_bytes =
            std::fs::read(&source_path).expect("original race source should be readable");
        let alternate_path = fixture
            .root
            .join(".sprite-studio/alternate-race-source.png");
        let mut alternate = RgbaImage::new(32, 32);
        for y in 3..29 {
            for x in 3..29 {
                alternate.put_pixel(x, y, Rgba([28, 188, 122, 255]));
            }
        }
        alternate
            .save(&alternate_path)
            .expect("alternate race source should save");
        let alternate_bytes =
            std::fs::read(&alternate_path).expect("alternate race source should be readable");
        assert_ne!(alternate_bytes, original_source_bytes);

        let mut rig = ik_biped_walk();
        rig["name"] = json!("source_race_walk");
        let (rig_path, rig_bytes) = fixture.write_rig("source-race-walk", &rig);
        let injection = r#"
import importlib.util
import shutil
import sys
from pathlib import Path

workspace = Path.cwd().resolve()
engine = workspace / ".sprite-studio" / "sprite_rig.py"
source = workspace / "assets" / "characters" / "walker.png"
alternate = workspace / ".sprite-studio" / "alternate-race-source.png"
sys.path.insert(0, str(engine.parent))
spec = importlib.util.spec_from_file_location("sprite_rig_injected_source_race", engine)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)

original_load_png = module.load_png
state = {"swapped": False}

def raced_load_png(path):
    decoded = original_load_png(path)
    if Path(path).resolve() == source and not state["swapped"]:
        state["swapped"] = True
        shutil.copy2(alternate, source)
    return decoded

module.load_png = raced_load_png
sys.argv = [str(engine), ".sprite-studio/rigs/source-race-walk.json"]
module.main()
"#;
        let failed = Command::new("python3")
            .current_dir(&fixture.root)
            .arg("-c")
            .arg(injection)
            .output()
            .expect("python3 should execute the injected source-race render");
        assert!(!failed.status.success(), "source identity race must fail");
        let diagnostic = String::from_utf8_lossy(&failed.stderr);
        assert!(
            diagnostic.contains("source master changed while the render was being prepared; retry"),
            "source race should fail before activation: {diagnostic}"
        );
        assert!(!diagnostic.contains("Traceback"));
        assert_eq!(
            std::fs::read(&source_path).expect("swapped source should remain readable"),
            alternate_bytes,
            "the fixture must prove the on-disk source actually changed after decoding"
        );
        assert_eq!(
            std::fs::read(&rig_path).expect("race rig should remain readable"),
            rig_bytes,
            "source identity failure must not normalize or rewrite the rig"
        );
        for index in 1..=8 {
            assert!(!fixture
                .root
                .join(format!("assets/characters/source_race_walk_{index:02}.png"))
                .exists());
        }
        assert!(!fixture
            .root
            .join(".sprite-studio/last-generation.json")
            .exists());
        assert!(!fixture.root.join(".sprite-studio/versions").exists());
        let transactions = fixture.root.join(".sprite-studio/transactions");
        assert!(transactions.is_dir());
        assert_eq!(
            std::fs::read_dir(transactions)
                .expect("transaction root should be readable")
                .count(),
            0,
            "pre-activation source race must clean its transaction"
        );
        assert_eq!(
            std::fs::read_dir(fixture.root.join("assets/characters"))
                .expect("asset directory should remain readable")
                .filter_map(Result::ok)
                .filter(|entry| entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".source_race_walk-stage-"))
                .count(),
            0,
            "pre-activation source race must remove staged frames"
        );
    }

    #[cfg(unix)]
    #[test]
    fn bundled_rig_mcp_rejects_a_symlinked_external_asset_root() {
        let fixture = RigRendererFixture::new();
        let external = std::env::temp_dir().join(format!(
            "sprite-studio-external-assets-test-{}",
            Uuid::new_v4()
        ));
        std::fs::create_dir_all(external.join("characters"))
            .expect("external asset fixture should exist");
        std::fs::copy(
            fixture.root.join("assets/characters/walker.png"),
            external.join("characters/walker.png"),
        )
        .expect("external master should copy");
        std::fs::remove_dir_all(fixture.root.join("assets"))
            .expect("temporary in-workspace assets should be removable");
        std::os::unix::fs::symlink(&external, fixture.root.join("assets"))
            .expect("external asset symlink should be created");
        fixture.write_rig("external-source", &good_biped_walk());

        let mut mcp = RigMcpSession::new(&fixture.root);
        let initialized = mcp.request(&json!({
            "jsonrpc": "2.0",
            "id": 0,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-11-25",
                "capabilities": {},
                "clientInfo": {"name": "sprite-studio-test", "version": "1"},
            },
        }));
        assert_eq!(initialized["result"]["protocolVersion"], "2025-11-25");
        mcp.send(&json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
        }));
        let rejected = mcp.request(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": "sprite_rig_validate",
                "arguments": {"rig": ".sprite-studio/rigs/external-source.json"},
            },
        }));
        assert_eq!(rejected["result"]["isError"], true);
        assert!(rejected["result"]["structuredContent"]["error"]
            .as_str()
            .expect("external source rejection should be text")
            .contains("source must be an existing PNG under assets/ or .sprite-studio/"));

        drop(mcp);
        std::fs::remove_dir_all(external).expect("external asset fixture should be removable");
    }
}
