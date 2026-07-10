use anyhow::Result;
use clap::{Parser, Subcommand};

mod android;
mod launch;
mod run;
mod ui;

#[derive(Parser)]
#[command(
    name = "emu",
    about = "Interactive launcher for Android Virtual Devices + gradle install/logcat helper",
    arg_required_else_help = true
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Select and launch an AVD
    Launch,
    /// Gradle install + logcat streaming
    Run {
        /// Skip Gradle build, run logcat against the already-installed APK
        #[arg(long)]
        no_install: bool,

        /// Skip automatic activity launch after install
        #[arg(long)]
        no_start: bool,

        /// Clear logcat buffer before streaming
        #[arg(long)]
        clear: bool,

        /// Device boot wait timeout in seconds (default: 180)
        #[arg(long, default_value_t = 180)]
        boot_timeout: u64,

        /// Manually specify APK path (default: auto-detect from build/outputs/apk/)
        #[arg(long)]
        apk: Option<std::path::PathBuf>,
    },
}

fn main() {
    let original = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = console::Term::stdout().show_cursor();
        original(info);
    }));

    let cli = Cli::parse();

    if let Err(e) = dispatch(&cli) {
        ui::error(&format!("{e:#}"));
        std::process::exit(1);
    }
}

fn dispatch(cli: &Cli) -> Result<()> {
    match &cli.command {
        Some(Commands::Launch) => launch::run(),
        Some(Commands::Run {
            no_install,
            no_start,
            clear,
            boot_timeout,
            apk,
        }) => run::run_with(
            *no_install,
            *no_start,
            *clear,
            *boot_timeout,
            apk.as_deref(),
        ),
        None => Ok(()),
    }
}
