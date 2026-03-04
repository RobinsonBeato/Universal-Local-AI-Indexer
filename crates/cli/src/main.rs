use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use clap::{Args, Parser, Subcommand};
use lupa_core::{LupaConfig, LupaEngine, SearchOptions};
use notify::{recommended_watcher, RecursiveMode, Watcher};

#[derive(Parser, Debug)]
#[command(
    name = "lupa",
    version,
    about = "Local file indexer/searcher (offline-first)"
)]
struct Cli {
    #[arg(long, default_value = ".")]
    root: PathBuf,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Index(IndexCmd),
    Search(SearchCmd),
    Doctor(DoctorCmd),
}

#[derive(Subcommand, Debug)]
enum IndexSubcommand {
    Build(IndexBuildArgs),
    Watch(IndexWatchArgs),
}

#[derive(Args, Debug)]
struct IndexCmd {
    #[command(subcommand)]
    command: IndexSubcommand,
}

#[derive(Args, Debug)]
struct IndexBuildArgs {
    #[arg(long)]
    json: bool,
}

#[derive(Args, Debug)]
struct IndexWatchArgs {
    #[arg(long, default_value_t = 2)]
    interval_secs: u64,

    #[arg(long)]
    json: bool,
}

#[derive(Args, Debug)]
struct SearchCmd {
    query: String,

    #[arg(long, default_value_t = 20)]
    limit: usize,

    #[arg(long)]
    json: bool,

    #[arg(long)]
    path_prefix: Option<String>,

    #[arg(long)]
    regex: Option<String>,

    #[arg(long)]
    highlight: bool,

    #[arg(long)]
    stats: bool,
}

#[derive(Args, Debug)]
struct DoctorCmd {
    #[arg(long)]
    json: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let root = cli.root.canonicalize().unwrap_or(cli.root);
    let watch_root = root.clone();

    let mut cfg = LupaConfig::load(&root)?;
    if cfg.threads > 0 {
        let _ = rayon::ThreadPoolBuilder::new()
            .num_threads(cfg.threads)
            .build_global();
    } else {
        cfg.threads = num_cpus::get();
    }

    let engine = LupaEngine::new(root, cfg)?;

    match cli.command {
        Commands::Index(index_cmd) => match index_cmd.command {
            IndexSubcommand::Build(args) => {
                let stats = engine.build_incremental()?;
                if args.json {
                    println!("{}", serde_json::to_string_pretty(&stats)?);
                } else {
                    println!(
                        "index build: scanned={} new={} updated={} skipped={} removed={} took={}ms",
                        stats.scanned,
                        stats.indexed_new,
                        stats.indexed_updated,
                        stats.skipped_unchanged,
                        stats.removed,
                        stats.duration_ms
                    );
                }
            }
            IndexSubcommand::Watch(args) => {
                let running = Arc::new(AtomicBool::new(true));
                let running_signal = Arc::clone(&running);
                ctrlc::set_handler(move || {
                    running_signal.store(false, Ordering::SeqCst);
                })?;

                // Initial sync to avoid missing pre-existing files.
                let initial = engine.build_incremental()?;
                if args.json {
                    println!("{}", serde_json::to_string(&initial)?);
                } else {
                    println!(
                        "watch init: scanned={} new={} updated={} skipped={} removed={} took={}ms",
                        initial.scanned,
                        initial.indexed_new,
                        initial.indexed_updated,
                        initial.skipped_unchanged,
                        initial.removed,
                        initial.duration_ms
                    );
                }

                let (tx, rx) = mpsc::channel();
                let mut watcher = recommended_watcher(move |res| {
                    let _ = tx.send(res);
                })?;
                watcher.watch(&watch_root, RecursiveMode::Recursive)?;

                let mut dirty = HashSet::<PathBuf>::new();
                while running.load(Ordering::SeqCst) {
                    match rx.recv_timeout(Duration::from_secs(args.interval_secs)) {
                        Ok(Ok(event)) => {
                            for p in event.paths {
                                dirty.insert(p);
                            }
                        }
                        Ok(Err(err)) => {
                            eprintln!("watch error: {err}");
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    }

                    if dirty.is_empty() {
                        continue;
                    }

                    let dirty_batch = dirty.drain().collect::<Vec<_>>();
                    let stats = if dirty_batch.len() > 5000 {
                        // Overflow safety valve: rebuild if event burst is too large.
                        engine.build_incremental()?
                    } else {
                        engine.apply_dirty_paths(&dirty_batch)?
                    };

                    if args.json {
                        println!("{}", serde_json::to_string(&stats)?);
                    } else {
                        println!(
                            "watch tick: scanned={} new={} updated={} skipped={} removed={} errors={} took={}ms",
                            stats.scanned,
                            stats.indexed_new,
                            stats.indexed_updated,
                            stats.skipped_unchanged,
                            stats.removed,
                            stats.errors,
                            stats.duration_ms
                        );
                    }
                }
            }
        },
        Commands::Search(args) => {
            let opts = SearchOptions {
                limit: args.limit,
                path_prefix: args.path_prefix,
                regex: args.regex,
                highlight: args.highlight,
            };
            let result = engine.search(&args.query, &opts)?;
            if args.json {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                for (i, hit) in result.hits.iter().enumerate() {
                    println!("{}. {} (score {:.3})", i + 1, hit.path, hit.score);
                    if let Some(snippet) = &hit.snippet {
                        println!("   {}", snippet);
                    }
                }
                if args.stats {
                    println!(
                        "stats: hits={} took={}ms",
                        result.total_hits, result.took_ms
                    );
                }
            }
        }
        Commands::Doctor(args) => {
            let report = engine.doctor()?;
            if args.json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                println!("doctor");
                println!("- project_root: {}", report.project_root);
                println!("- data_dir: {}", report.data_dir);
                println!("- index_dir: {}", report.index_dir);
                println!("- db_path: {}", report.db_path);
                println!("- threads: {}", report.threads);
                for c in report.checks {
                    println!("- check: {}", c);
                }
            }
        }
    }

    Ok(())
}
