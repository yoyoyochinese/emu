use anyhow::{Result, anyhow};
use dirs;
use std::{env, path::PathBuf};

pub fn locate_binary_in_sdk(sub_dir: &str, binary_target: &str) -> Option<PathBuf> {
    let sdk_root = env::var_os("ANDROID_HOME").or_else(|| env::var_os("ANDROID_SDK_ROOT"))?;
    let candidate = PathBuf::from(sdk_root).join(sub_dir).join(binary_target);

    if candidate.is_file() {
        return Some(candidate);
    }

    None
}

pub fn locate_binary_in_path(binary_target: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;

    for p in env::split_paths(&path) {
        let candidate = p.join(binary_target);

        if candidate.is_file() {
            return Some(candidate);
        }
    }

    None
}

pub fn locate_binary_in_select_paths(
    paths: &[PathBuf],
    sub_dir: &str,
    binary_target: &str,
) -> Option<PathBuf> {
    paths.iter().find_map(|path| {
        let candidate = path.join(sub_dir).join(binary_target);
        candidate.is_file().then_some(candidate)
    })
}

fn fallback_sdk_locations() -> Vec<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    let home_dir = dirs::home_dir();

    cfg_select! {
            target_os = "windows" => {

            if let Some(h) = home_dir {
                candidates.push(h.join("AppData").join("Local").join("Android").join("Sdk"));
            }
            candidates.push(PathBuf::from(r"C:\Program Files (x86)\Android\android-sdk"));
            candidates.push(PathBuf::from(r"C:\Program Files\Android\android-sdk"));
            candidates.push(PathBuf::from(r"C:\Android\android-sdk"));
            }

            target_os = "macos" => {
                if let Some(h) = home_dir {
                    candidates.push(h.join("Library").join("Android").join("sdk"));
                }
            }

            target_os = "linux" => {
                if let Some(h) = home_dir {
                    candidates.push(h.join("Android").join("Sdk"));
                }
            }
    }

    return candidates;
}

pub fn locate_emulator_binary() -> Result<PathBuf> {
    let emulator_target = format!("emulator{}", env::consts::EXE_SUFFIX);

    locate_binary_in_sdk("emulator", &emulator_target)
        .or_else(|| locate_binary_in_path(&emulator_target))
        .or_else(|| {
            locate_binary_in_select_paths(&fallback_sdk_locations(), "emulator", &emulator_target)
        })
        .ok_or_else(|| anyhow!("Failed to locate Android emulator binary"))
}
