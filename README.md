# emu

A CLI tool for Android developers to streamline the emulator workflow — AVD selection/launch, Gradle install, and logcat streaming — all from the terminal, without Android Studio.

## Features

- **`emu launch`** — Interactively select an AVD and launch it in detached mode
- **`emu run`** — Build, install, start the app, and stream logcat in one command

## Installation

```bash
cargo install --path .
```

This places the `emu` binary in `~/.cargo/bin/` (must be on your `PATH`).

## Prerequisites

- [Rust](https://rustup.rs/) (stable, edition 2024)
- Android SDK with:
  - `emulator` (`emulator/`)
  - `adb` (`platform-tools/`)
  - `aapt2` (`build-tools/<version>/`)
- At least one AVD (create with `avdmanager create avd -n <name> -k <package>`)
- A Gradle project with `gradlew` in the root

Set `ANDROID_HOME` or `ANDROID_SDK_ROOT` to your SDK path, or ensure the SDK binaries are on your `PATH`.

## Usage

### Launch an AVD

```bash
emu launch
```

Lists available AVDs, prompts you to select one, and asks whether to cold boot (skip snapshot load). The emulator starts detached so it survives terminal close.

### Build, install, and stream logs

```bash
emu run
```

Executes the full pipeline:

1. Finds `gradlew` by walking up from the current directory
2. Waits for an online device (or boot to complete)
3. Runs `./gradlew installDebug`
4. Locates the built APK (searches all `build/outputs/apk/` directories)
5. Extracts the `applicationId` via `aapt2 dump badging`
6. Launches the main activity via `adb shell monkey`
7. Streams `adb logcat` filtered by the app's PID, with colorized output

Press `Ctrl-C` to stop logcat streaming (the `adb` child process is cleaned up).

### Options

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--no-install` | bool | `false` | Skip Gradle build; use the already-installed APK |
| `--no-start` | bool | `false` | Skip launching the main activity |
| `--clear` | bool | `false` | Clear logcat buffer before streaming |
| `--boot-timeout` | u64 | `180` | Device boot wait timeout in seconds |
| `--apk` | path | auto-detect | Manually specify APK path |

## Examples

```bash
# Launch an AVD
emu launch

# Build, install, and stream logs
emu run

# Skip build, just stream logs from installed app
emu run --no-install

# Cold boot with a 5-minute timeout
emu launch   # select AVD, choose cold boot
emu run --boot-timeout 300

# Use a specific APK
emu run --apk path/to/app-debug.apk
```

## Logcat Colors

| Priority | Color |
|----------|-------|
| V (Verbose) | dim |
| D (Debug) | blue |
| I (Info) | green |
| W (Warn) | yellow |
| E (Error) | red bold |
| F (Fatal) | red bold on white |

## Known Limitations

- **PID filtering is one-shot**: if the app is restarted, logcat keeps the old PID and shows nothing. Restart `emu run` to pick up the new PID.
- **Single device**: if multiple devices are connected, the first online device is used.
- **Windows colors**: older Windows Terminal versions may not render ANSI colors correctly.

## License

MIT
