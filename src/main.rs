mod tui;

use std::path::{Path, PathBuf};

use clap::{ArgAction, Parser};
use color_eyre::Result;
use gix::{date::time::format::ISO8601, revision::walk::Sorting};
#[allow(unused)]
use tracing::debug;
use tui::LogEntryInfo;

#[derive(Debug, clap::Parser)]
#[clap(name = "log", about = "git log example", version = option_env!("GIX_VERSION"))]
struct Args {
    /// Directory to use (git directory)
    #[clap(name = "dir")]
    dir: Option<PathBuf>,
    /// Reverse the commit sort order.
    #[clap(short, long)]
    reverse: bool,
    /// Whether to include submodules (default to true)
    #[clap(default_value_t = true, long = "no-submodules", action = ArgAction::SetFalse)]
    submodules: bool,
}

fn main() -> Result<()> {
    color_eyre::install()?;
    tracing_subscriber::fmt::init();

    let args = Args::parse_from(gix::env::args_os());
    run(args)
}

fn run(args: Args) -> Result<()> {
    let mut entries = Vec::new();
    let git_dir = args.dir.as_deref().unwrap_or(Path::new("."));
    let repo = gix::discover(git_dir)?;

    let submodules;
    if args.submodules
        && let Some(sub) = repo.submodules()?
    {
        submodules = sub.collect::<Vec<_>>();
        for submodule in &submodules {
            if let Some(repo) = submodule.open()? {
                let log_iter = get_log_iter(&repo, "HEAD")?;
                for entry in log_iter {
                    entries.push((entry?, Some(submodule)));
                }
            }
        }
    }

    let log_iter = get_log_iter(&repo, "HEAD")?;
    for entry in log_iter {
        entries.push((entry?, None));
    }
    if args.reverse {
        entries.sort_by_key(|(entry, _)| entry.author_time);
    } else {
        entries.sort_by_key(|(entry, _)| std::cmp::Reverse(entry.author_time));
    }

    tui::run(git_dir.to_path_buf(), entries)
}

fn get_log_iter<'a>(
    repo: &'a gix::Repository,
    spec: &str,
) -> Result<Box<dyn Iterator<Item = Result<LogEntryInfo>> + 'a>> {
    Ok(Box::new(
        repo.rev_walk([repo
            .rev_parse_single(spec)?
            .object()?
            .try_into_commit()?
            .id()])
            .sorting(Sorting::ByCommitTime(Default::default()))
            .all()?
            .map(|info| -> Result<_> {
                let info = info?;
                let commit = info.object()?;
                let commit_ref = commit.decode()?;

                let commit_id = commit.id().to_hex().to_string();
                let author = commit_ref.author().name.into();
                let author_time = commit_ref.author.time()?;
                //let time = commit_ref.author.time.to_string();
                let time = author_time.format(ISO8601);
                let message = commit_ref.message.to_owned();
                Ok(LogEntryInfo {
                    commit_id,
                    author,
                    time,
                    message,
                    author_time,
                })
            }),
    ))
}
