use anyhow::{Result, anyhow, bail};
use std::{
    env,
    path::PathBuf,
    process::Command,
    time::{Duration, Instant},
};

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

    candidates
}

pub fn locate_emulator_binary() -> Result<PathBuf> {
    let emulator_target = format!("emulator{}", env::consts::EXE_SUFFIX);

    locate_binary_in_sdk("emulator", &emulator_target)
        .or_else(|| locate_binary_in_path(&emulator_target))
        .or_else(|| {
            locate_binary_in_select_paths(&fallback_sdk_locations(), "emulator", &emulator_target)
        })
        .ok_or_else(|| {
            anyhow!(
                "Failed to locate Android emulator binary.\n\
             Set $ANDROID_HOME or $ANDROID_SDK_ROOT, or add the emulator to $PATH."
            )
        })
}

pub fn locate_adb_binary() -> Result<PathBuf> {
    let adb_target = format!("adb{}", env::consts::EXE_SUFFIX);

    locate_binary_in_sdk("platform-tools", &adb_target)
        .or_else(|| locate_binary_in_path(&adb_target))
        .or_else(|| {
            locate_binary_in_select_paths(&fallback_sdk_locations(), "platform-tools", &adb_target)
        })
        .ok_or_else(|| {
            anyhow!(
                "Failed to locate adb binary.\n\
             Set $ANDROID_HOME or $ANDROID_SDK_ROOT, or add adb to $PATH."
            )
        })
}

pub fn locate_aapt2_binary() -> Result<PathBuf> {
    let aapt2_target = format!("aapt2{}", env::consts::EXE_SUFFIX);

    let sdk_roots: Vec<PathBuf> = env::var_os("ANDROID_HOME")
        .or_else(|| env::var_os("ANDROID_SDK_ROOT"))
        .map(|p| vec![PathBuf::from(p)])
        .unwrap_or_else(fallback_sdk_locations);

    for sdk_root in &sdk_roots {
        let build_tools_dir = sdk_root.join("build-tools");
        if let Ok(entries) = std::fs::read_dir(&build_tools_dir) {
            let mut versions: Vec<PathBuf> = entries
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .collect();
            versions.sort();
            if let Some(latest) = versions.last() {
                let candidate = latest.join(&aapt2_target);
                if candidate.is_file() {
                    return Ok(candidate);
                }
            }
        }
    }

    locate_binary_in_path(&aapt2_target).ok_or_else(|| {
        anyhow!(
            "Failed to locate aapt2 binary.\n\
             Ensure Android SDK build-tools are installed."
        )
    })
}

pub fn list_avds() -> Result<Vec<String>> {
    let emulator = locate_emulator_binary()?;
    let output = Command::new(&emulator)
        .arg("-list-avds")
        .output()
        .map_err(|e| anyhow!("running `emulator -list-avds`: {e}"))?;

    if !output.status.success() {
        bail!("`emulator -list-avds` exited with non-zero status");
    }

    let avds: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .collect();

    if avds.is_empty() {
        bail!("No AVDs found. Create one with: avdmanager create avd -n <name> -k <package>");
    }

    Ok(avds)
}

pub fn list_online_devices() -> Result<Vec<String>> {
    let adb = locate_adb_binary()?;
    let output = Command::new(&adb)
        .arg("devices")
        .output()
        .map_err(|e| anyhow!("running `adb devices`: {e}"))?;

    let mut devices: Vec<String> = Vec::new();
    for line in String::from_utf8_lossy(&output.stdout).lines().skip(1) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 && parts[1] == "device" {
            devices.push(parts[0].to_owned());
        }
    }

    Ok(devices)
}

pub fn wait_for_boot(timeout_secs: u64) -> Result<String> {
    let adb = locate_adb_binary()?;
    let deadline = Instant::now() + Duration::from_secs(timeout_secs);

    // Phase 1: wait for device to appear
    let mut serial = None;
    while Instant::now() < deadline {
        let devices = list_online_devices()?;
        if let Some(first) = devices.into_iter().next() {
            serial = Some(first);
            break;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    let serial =
        serial.ok_or_else(|| anyhow!("Timed out waiting for device after {timeout_secs}s"))?;

    // Phase 2: wait for sys.boot_completed == 1
    while Instant::now() < deadline {
        let output = Command::new(&adb)
            .args(["-s", &serial])
            .args(["shell", "getprop", "sys.boot_completed"])
            .output()
            .map_err(|e| anyhow!("running `adb shell getprop`: {e}"))?;

        if String::from_utf8_lossy(&output.stdout).trim() == "1" {
            return Ok(serial);
        }
        std::thread::sleep(Duration::from_millis(500));
    }

    bail!("Timed out waiting for `sys.boot_completed` after {timeout_secs}s")
}

pub fn package_name_from_apk(apk_path: &std::path::Path) -> Result<String> {
    let aapt2 = locate_aapt2_binary()?;
    let output = Command::new(&aapt2)
        .args(["dump", "badging"])
        .arg(apk_path)
        .output()
        .map_err(|e| anyhow!("running `aapt2 dump badging`: {e}"))?;

    if !output.status.success() {
        bail!("`aapt2 dump badging` exited with non-zero status");
    }

    parse_package_name(&String::from_utf8_lossy(&output.stdout))
}

fn parse_package_name(aapt_output: &str) -> Result<String> {
    for line in aapt_output.lines() {
        if let Some(rest) = line.strip_prefix("package:") {
            for part in rest.split_whitespace() {
                if let Some(value) = part.strip_prefix("name='")
                    && let Some(name) = value.strip_suffix('\'')
                {
                    return Ok(name.to_owned());
                }
            }
        }
    }
    bail!("Could not parse package name from aapt2 output")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_package_name_basic() {
        let output =
            "package: name='com.example.app' versionCode='1' versionName='1.0'\nsdkVersion:'21'";
        assert_eq!(parse_package_name(output).unwrap(), "com.example.app");
    }

    #[test]
    fn parse_package_name_no_name() {
        assert!(parse_package_name("sdkVersion:'21'").is_err());
    }

    #[test]
    fn parse_package_name_empty() {
        assert!(parse_package_name("").is_err());
    }

    #[test]
    fn parse_package_name_name_not_first_field() {
        let output = "package: versionCode='1' name='com.test.app' versionName='1.0'";
        assert_eq!(parse_package_name(output).unwrap(), "com.test.app");
    }
}
