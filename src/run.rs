use anyhow::{Result, bail};
use indicatif::ProgressBar;
use std::{
    env,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

#[allow(clippy::too_many_arguments)]
pub fn run_with(
    no_install: bool,
    no_start: bool,
    clear: bool,
    boot_timeout: u64,
    apk: Option<&Path>,
) -> Result<()> {
    let project_root = find_gradle_root(&env::current_dir()?)?;
    crate::ui::info("project", &project_root.display().to_string());

    // Wait for device to be online
    let serial = crate::android::wait_for_boot(boot_timeout)?;
    crate::ui::info("device", &serial);

    // Set ANDROID_SERIAL for gradle
    // SAFETY: This is single-threaded before spawning any child process.
    unsafe {
        env::set_var("ANDROID_SERIAL", &serial);
    }

    // Find or build APK
    let apk_path = if no_install {
        find_apk(&project_root, apk)?
    } else {
        gradle_install(&project_root)?;
        find_apk(&project_root, apk)?
    };

    // Get package name from APK
    let package = crate::android::package_name_from_apk(&apk_path)?;
    crate::ui::info("package", &package);

    // Start main activity
    if !no_start {
        crate::ui::info("launching", "starting activity");
        let adb = crate::android::locate_adb_binary()?;
        Command::new(&adb)
            .args(["-s", &serial])
            .args([
                "shell",
                "monkey",
                "-p",
                &package,
                "-c",
                "android.intent.category.LAUNCHER",
                "1",
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?;
    }

    // Clear logcat buffer if requested
    if clear {
        let adb = crate::android::locate_adb_binary()?;
        Command::new(&adb)
            .args(["-s", &serial])
            .args(["logcat", "-c"])
            .status()?;
    }

    // Stream logcat
    stream_logcat(&serial, &package)?;

    Ok(())
}

fn find_gradle_root(cwd: &Path) -> Result<PathBuf> {
    let gradlew_name = if cfg!(windows) {
        "gradlew.bat"
    } else {
        "gradlew"
    };

    let mut current = cwd.to_path_buf();
    loop {
        let candidate = current.join(gradlew_name);
        if candidate.is_file() {
            return Ok(current);
        }

        match current.parent() {
            Some(parent) => {
                // Check if we've reached the filesystem root
                if parent == current {
                    break;
                }
                current = parent.to_path_buf();
            }
            None => break,
        }
    }

    bail!(
        "Could not find `gradlew` in any parent directory of {}",
        cwd.display()
    )
}

fn find_apk(project_root: &Path, manual: Option<&Path>) -> Result<PathBuf> {
    if let Some(path) = manual {
        if path.is_file() {
            return Ok(path.to_path_buf());
        }
        bail!("Specified APK not found: {}", path.display());
    }

    // Search for .apk files under all build/outputs/apk/ directories
    // (handles both single-module and multi-module project layouts)
    let mut apks: Vec<PathBuf> = Vec::new();

    collect_apks(project_root, &mut apks);

    if apks.is_empty() {
        bail!(
            "No APK found under {}. Run `gradlew installDebug` first.",
            project_root.display()
        );
    }

    if apks.len() == 1 {
        return Ok(apks.into_iter().next().unwrap());
    }

    // Multiple APKs: prompt user
    let display_items: Vec<String> = apks
        .iter()
        .map(|p| {
            p.strip_prefix(project_root)
                .unwrap_or(p)
                .display()
                .to_string()
        })
        .collect();

    let selection = dialoguer::Select::new()
        .with_prompt("Multiple APKs found, select one")
        .items(&display_items)
        .default(0)
        .interact()?;

    Ok(apks.into_iter().nth(selection).unwrap())
}

fn collect_apks(dir: &Path, results: &mut Vec<PathBuf>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_apks(&path, results);
            } else if path.extension().is_some_and(|ext| ext == "apk") {
                results.push(path);
            }
        }
    }
}

fn gradle_install(project_root: &Path) -> Result<()> {
    let gradlew = if cfg!(windows) {
        "gradlew.bat"
    } else {
        "./gradlew"
    };

    let mut cmd = Command::new(gradlew);
    cmd.current_dir(project_root)
        .arg("installDebug")
        .arg("--console=plain")
        .env(
            "ANDROID_SERIAL",
            env::var("ANDROID_SERIAL").unwrap_or_default(),
        )
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd.spawn()?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let pb = ProgressBar::new_spinner();
    pb.set_style(crate::ui::spinner_style());
    pb.set_prefix("gradle");
    pb.enable_steady_tick(Duration::from_millis(100));

    // stdout thread: update spinner with task name
    if let Some(stdout) = stdout {
        let pb_clone = pb.clone();
        std::thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines().map_while(|l| l.ok()) {
                if let Some(task) = line.strip_prefix("> Task ") {
                    pb_clone.set_message(task.to_owned());
                }
            }
        });
    }

    // stderr thread: collect all lines
    let stderr_thread = if let Some(stderr) = stderr {
        let handle = std::thread::spawn(move || {
            let reader = BufReader::new(stderr);
            reader
                .lines()
                .map_while(|l| l.ok())
                .collect::<Vec<String>>()
        });
        Some(handle)
    } else {
        None
    };

    let status = child.wait()?;

    pb.finish_and_clear();

    if !status.success() {
        crate::ui::build_failed("BUILD FAILED");
        if let Some(handle) = stderr_thread
            && let Ok(lines) = handle.join()
        {
            for line in lines {
                eprintln!("  {line}");
            }
        }
        bail!("Gradle build failed with status: {status}");
    }

    crate::ui::success("BUILD SUCCESSFUL");
    Ok(())
}

fn stream_logcat(serial: &str, package: &str) -> Result<()> {
    let adb = crate::android::locate_adb_binary()?;

    // Poll for PID (max 10 seconds, 300ms interval)
    let deadline = Instant::now() + Duration::from_secs(10);
    let pid = loop {
        let output = Command::new(&adb)
            .args(["-s", serial])
            .args(["shell", "pidof", package])
            .output()?;

        let value = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        if !value.is_empty() {
            break value;
        }

        if Instant::now() >= deadline {
            bail!("Could not find PID for `{package}` within 10s. Is the app running?");
        }

        std::thread::sleep(Duration::from_millis(300));
    };

    crate::ui::info("pid", &pid);

    // Set up Ctrl-C handler
    let running = Arc::new(AtomicBool::new(true));
    let running_clone = Arc::clone(&running);
    ctrlc::set_handler(move || {
        running_clone.store(false, Ordering::SeqCst);
    })?;

    let mut child = Command::new(&adb)
        .args(["-s", serial])
        .args(["logcat", "-v", "brief", &format!("--pid={pid}")])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;

    let stdout = child.stdout.take().unwrap();
    let reader = BufReader::new(stdout);

    for line in reader.lines().map_while(|l| l.ok()) {
        if !running.load(Ordering::SeqCst) {
            break;
        }
        println!("{}", crate::ui::colorize_logcat(&line));
    }

    let _ = child.kill();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn find_gradle_root_finds_in_current_dir() {
        let dir = tempdir().unwrap();
        let gradlew = dir.path().join("gradlew");
        fs::write(&gradlew, "#!/bin/sh").unwrap();

        let result = find_gradle_root(dir.path());
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), dir.path());
    }

    #[test]
    fn find_gradle_root_finds_in_parent() {
        let dir = tempdir().unwrap();
        let gradlew = dir.path().join("gradlew");
        fs::write(&gradlew, "#!/bin/sh").unwrap();

        let subdir = dir.path().join("subdir");
        fs::create_dir(&subdir).unwrap();

        let result = find_gradle_root(&subdir);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), dir.path());
    }

    #[test]
    fn find_gradle_root_not_found() {
        let dir = tempdir().unwrap();
        let result = find_gradle_root(dir.path());
        assert!(result.is_err());
    }
}
