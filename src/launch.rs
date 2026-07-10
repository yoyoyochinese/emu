use anyhow::Result;
use dialoguer::{Confirm, Select};
use std::process::{Command, Stdio};

use crate::android;
use crate::ui;

pub fn run() -> Result<()> {
    let emulator = android::locate_emulator_binary()?;

    let avds = android::list_avds()?;

    let selection = Select::new()
        .with_prompt("Select AVD")
        .items(&avds)
        .default(0)
        .interact()?;

    let avd_name = &avds[selection];

    let cold_boot = Confirm::new()
        .with_prompt("Cold boot (no snapshot load)?")
        .default(false)
        .interact()?;

    println!();

    let mut cmd = Command::new(&emulator);
    cmd.arg("-avd")
        .arg(avd_name)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .current_dir(
            emulator
                .parent()
                .ok_or_else(|| anyhow::anyhow!("emulator binary has no parent directory"))?,
        );

    if cold_boot {
        cmd.arg("-no-snapshot-load");
    }

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        unsafe {
            cmd.pre_exec(|| {
                // setsid() is async-signal-safe per POSIX, so calling it
                // inside pre_exec (between fork and exec) is well-defined.
                // This creates a new session so the emulator survives
                // terminal disconnect (no SIGHUP).
                let ret = libc::setsid();
                if ret == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP
        cmd.creation_flags(0x0000_0208);
    }

    let child = cmd.spawn()?;

    ui::info("launched", avd_name);
    ui::info("pid", &child.id().to_string());

    Ok(())
}
