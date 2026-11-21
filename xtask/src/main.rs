mod build_time;
mod checks_chart;

use std::{path::PathBuf, sync::LazyLock};

use anyhow::Result;
use clap::{Parser, Subcommand};
use xshell::{Shell, cmd};

#[derive(Parser)]
#[command(name = "xtask", about = "Puns project automation")]
#[command(styles = clap_cargo::style::CLAP_STYLING)]
#[clap(disable_version_flag = true, bin_name = "cargo xtask")]
struct Cli {
  #[command(subcommand)]
  command: Command,
}

#[derive(Subcommand)]
enum Command {
  /// Format code
  Fmt {
    /// Check formatting without making changes
    #[arg(long)]
    check: bool,
  },
  /// Lint code and check formatting
  Lint {
    /// Fix lint issues automatically
    #[arg(long)]
    fix: bool,
  },
  /// Time the builds of the section "Build time" of `docs/performance.md`
  BuildTime,
  /// Write the table and the chart of the section "Time" of `docs/performance.md` from the record
  /// of the benchmark
  ChecksChart {
    /// Fail if the table or the chart is not what this writes, rather than write them
    #[arg(long)]
    check: bool,
  },
}

/// The root directory of the workspace.
static ROOT: LazyLock<PathBuf> = LazyLock::new(|| {
  std::path::Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().nth(1).unwrap().to_path_buf()
});

fn main() -> Result<()> {
  let cli = Cli::parse();

  let mut sh = Shell::new()?;
  sh.set_current_dir(ROOT.as_path());

  match cli.command {
    Command::Fmt { check } => fmt(&sh, check),
    Command::Lint { fix } => lint(&sh, fix),
    Command::BuildTime => build_time::run(&ROOT),
    Command::ChecksChart { check } => checks_chart::run(&ROOT, check),
  }
}

/// The nightly toolchain that the unstable options of `rustfmt.toml` need. The crates build on
/// stable.
const FMT_TOOLCHAIN: &str = "+nightly-2026-11-17";

fn fmt(sh: &Shell, check: bool) -> Result<()> {
  let args =
    std::iter::once("--all").chain(check.then_some(["--", "--check"]).into_iter().flatten());
  cmd!(sh, "cargo {FMT_TOOLCHAIN} fmt {args...}").run_echo()?;
  Ok(())
}

fn lint(sh: &Shell, fix: bool) -> Result<()> {
  let args =
    if fix { ["--fix", "--allow-dirty", "--allow-staged"] } else { ["--", "-D", "warnings"] };
  cmd!(sh, "cargo clippy --workspace --tests --examples --benches {args...}").run_echo()?;
  // With the `unstable` feature: the targets that require it, the code under it.
  let unstable = "puns/unstable,puns-model/unstable";
  cmd!(sh, "cargo clippy --workspace --tests --examples --benches --features {unstable} {args...}")
    .run_echo()?;
  fmt(sh, !fix)
}
