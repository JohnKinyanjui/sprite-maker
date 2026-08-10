use crate::{
    error::{CommandError, CommandResult},
    models::AssetPack,
    workspace::workspace_path,
    AppState,
};
use std::path::{Component, Path};
use tauri::State;

fn valid_relative_asset(root: &Path, relative: &str) -> bool {
    let relative_path = Path::new(relative);
    !relative_path.is_absolute()
        && relative_path.components().all(|part| {
            !matches!(
                part,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
        && relative_path.starts_with("assets")
        && root.join(relative_path).is_file()
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 80
        && value.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        })
}

#[tauri::command]
pub fn list_asset_packs(
    workspace_id: String,
    state: State<'_, AppState>,
) -> CommandResult<Vec<AssetPack>> {
    let root = workspace_path(&state, &workspace_id)?;
    let directory = root.join(".sprite-studio/packs");
    std::fs::create_dir_all(&directory)?;
    let mut packs = Vec::new();
    for entry in std::fs::read_dir(directory)? {
        let path = entry?.path();
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let pack: AssetPack = serde_json::from_str(&std::fs::read_to_string(&path)?)
            .map_err(|error| CommandError::new("invalid_asset_pack", error.to_string()))?;
        if !valid_id(&pack.id)
            || pack.name.trim().is_empty()
            || pack.style.trim().is_empty()
            || pack.kind.trim().is_empty()
            || pack.files.is_empty()
            || !pack
                .files
                .iter()
                .all(|file| valid_relative_asset(&root, file))
        {
            return Err(CommandError::new(
                "invalid_asset_pack",
                format!("Asset pack manifest {} is invalid", path.display()),
            ));
        }
        packs.push(pack);
    }
    packs.sort_by(|left, right| right.created_at.cmp(&left.created_at));
    Ok(packs)
}

#[cfg(test)]
mod tests {
    use super::{valid_id, valid_relative_asset};
    use std::fs;
    use uuid::Uuid;

    #[test]
    fn pack_ids_and_files_stay_inside_workspace_assets() {
        let root = std::env::temp_dir().join(format!("sprite-pack-test-{}", Uuid::new_v4()));
        let asset = root.join("assets/creatures/forest_fox.png");
        fs::create_dir_all(asset.parent().expect("asset parent")).expect("fixture directory");
        fs::write(&asset, b"fixture").expect("fixture asset");
        assert!(valid_id("forest-animals"));
        assert!(!valid_id("../forest"));
        assert!(valid_relative_asset(
            &root,
            "assets/creatures/forest_fox.png"
        ));
        assert!(!valid_relative_asset(&root, "../outside.png"));
        fs::remove_dir_all(root).expect("fixture cleanup");
    }
}
