//! THROWAWAY: measures how long a user-initiated `open_today_journal`
//! waits on the workspace mutex while the background reconcile sweep
//! runs. Delete after reporting.
//!
//! Usage: cargo run --release -p outl-tauri-shared --example
//!        lock_wait_bench -- <workspace-root> <mode>
//! modes: before-union | before-desktop | after | after-nosettle

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use outl_core::hlc::HlcGenerator;
use outl_core::id::ActorId;
use outl_core::workspace::Workspace;
use outl_tauri_shared::helpers::{build_page_view, reproject_stale_md};
use outl_tauri_shared::workspace_open::{open_workspace_at, yield_to_user, WorkspaceGuards};
use parking_lot::Mutex;

type Slot = Arc<Mutex<Option<Workspace>>>;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let root = PathBuf::from(&args[1]);
    let mode = args[2].clone();

    let actor = ActorId::new();
    let hlc = HlcGenerator::new(actor);
    let guards: Mutex<Option<WorkspaceGuards>> = Mutex::new(None);
    let open_started = Instant::now();
    let ws = open_workspace_at(actor, &hlc, &root, 0, &guards).expect("open workspace");
    println!("mode={mode}");
    println!(
        "boot: {} pages, {} ops, open took {:?}",
        outl_actions::list_pages(&ws).len(),
        ws.log().len(),
        open_started.elapsed()
    );
    let slot: Slot = Arc::new(Mutex::new(Some(ws)));

    // The frontend issues its first open the instant the workspace is
    // published, which is also when the sweep is spawned.
    let stop = Arc::new(AtomicBool::new(false));
    let user = {
        let slot = Arc::clone(&slot);
        let hlc = hlc.clone();
        let root = root.clone();
        let stop = Arc::clone(&stop);
        thread::spawn(move || user_loop(&slot, &root, &hlc, &stop))
    };
    // The metric that actually answers "could a command have got in":
    // the longest run of consecutive `try_lock` failures. The user loop
    // above re-locks immediately, which lets it win `parking_lot`'s unfair
    // handoff and therefore *under*-reports back-to-back holds.
    let probe = {
        let slot = Arc::clone(&slot);
        let stop = Arc::clone(&stop);
        thread::spawn(move || unavailable_probe(&slot, &stop))
    };

    let swept = Instant::now();
    match mode.as_str() {
        "before-union" => sweep_before(&slot, &root, &hlc, Before::Union),
        "before-desktop" => sweep_before(&slot, &root, &hlc, Before::Desktop),
        "before-mobile" => sweep_before(&slot, &root, &hlc, Before::Mobile),
        "after" => {
            outl_tauri_shared::reconcile::reconcile_sweep(
                &slot,
                &root,
                &hlc,
                outl_tauri_shared::reconcile::SETTLE_BEFORE_WHOLE_WORKSPACE,
            );
        }
        "after-nosettle" => {
            outl_tauri_shared::reconcile::reconcile_sweep(&slot, &root, &hlc, Duration::ZERO);
        }
        other => panic!("unknown mode {other}"),
    }
    let sweep_took = swept.elapsed();
    stop.store(true, Ordering::Relaxed);
    let stats = user.join().expect("user thread");
    let long_runs = probe.join().expect("probe thread");

    println!("sweep total: {sweep_took:?}");
    println!("opens: {}", stats.waits.len());
    println!("FIRST open lock wait (the launch case): {:?}", stats.first);
    let mut sorted = stats.waits.clone();
    sorted.sort();
    println!("WORST open lock wait: {:?}", sorted.last().copied());
    println!(
        "p99 {:?}  p50 {:?}",
        sorted.get(sorted.len() * 99 / 100),
        sorted.get(sorted.len() / 2)
    );
    println!(
        "WORST unavailable window: {:?}",
        long_runs.iter().map(|(_, d)| *d).max()
    );
    println!("unavailable windows over {LONG_HOLD:?}, as (t+ from sweep start, length):");
    for (at, len) in &long_runs {
        println!("    t+{at:?}  {len:?}");
    }
}

/// A hold long enough that it can only be one of the whole-workspace
/// phases, not a single page's reconcile.
const LONG_HOLD: Duration = Duration::from_millis(150);

/// Every run of consecutive failed `try_lock`s at or over [`LONG_HOLD`] —
/// how long a command that is *not* already racing for the mutex would
/// have had to wait, and when. The user loop above re-locks immediately,
/// which lets it win `parking_lot`'s unfair handoff and therefore
/// under-reports back-to-back holds.
fn unavailable_probe(slot: &Slot, stop: &AtomicBool) -> Vec<(Duration, Duration)> {
    let started = Instant::now();
    let mut runs = Vec::new();
    let mut run_start: Option<Instant> = None;
    let mut record = |begin: Instant, runs: &mut Vec<(Duration, Duration)>| {
        let held = begin.elapsed();
        if held >= LONG_HOLD {
            runs.push((begin - started, held));
        }
    };
    while !stop.load(Ordering::Relaxed) {
        if slot.try_lock().is_some() {
            if let Some(begin) = run_start.take() {
                record(begin, &mut runs);
            }
        } else {
            run_start.get_or_insert_with(Instant::now);
        }
        thread::sleep(Duration::from_millis(1));
    }
    // A run still in flight when the sweep ends is the one that matters
    // most — it is the last phase's hold.
    if let Some(begin) = run_start.take() {
        record(begin, &mut runs);
    }
    runs
}

struct Stats {
    first: Duration,
    /// Max of the two lock acquisitions one open makes.
    waits: Vec<Duration>,
}

/// One `open_today_journal`: `open_today` under the mutation lock, then
/// `reproject_stale_md` + `build_page_view` under a second one. Times only
/// the two acquisitions.
fn one_open(slot: &Slot, root: &Path, hlc: &HlcGenerator) -> Duration {
    let t = Instant::now();
    let mut guard = slot.lock();
    let wait_a = t.elapsed();
    let ws = guard.as_mut().expect("published");
    let id = outl_actions::open_today(ws, hlc).expect("open today");
    drop(guard);

    let t = Instant::now();
    let guard = slot.lock();
    let wait_b = t.elapsed();
    let ws = guard.as_ref().expect("published");
    let _ = reproject_stale_md(ws, root, id, "bench");
    let _ = build_page_view(ws, root, id);
    drop(guard);
    wait_a.max(wait_b)
}

fn user_loop(slot: &Slot, root: &Path, hlc: &HlcGenerator, stop: &AtomicBool) -> Stats {
    let first = one_open(slot, root, hlc);
    let mut waits = vec![first];
    while !stop.load(Ordering::Relaxed) {
        waits.push(one_open(slot, root, hlc));
        thread::sleep(Duration::from_millis(5));
    }
    Stats { first, waits }
}

/// Which pre-change client to reproduce.
#[derive(PartialEq, Eq)]
enum Before {
    /// Desktop: paced orphan loop, no desynced-projection pass.
    Desktop,
    /// Mobile: blocking `lock()` per orphan with no yield at all, plus
    /// the desynced-projection pass the desktop never had.
    Mobile,
    /// The phase list the measurement pass reported: paced orphans plus
    /// every locked phase either client runs.
    Union,
}

/// Faithful copy of the pre-change clients.
fn sweep_before(slot: &Slot, root: &Path, hlc: &HlcGenerator, which: Before) {
    let engine = outl_actions::SyncEngine::new(root.to_path_buf(), hlc.actor());
    let orphans_log = engine.orphans_log();
    let t = Instant::now();
    let orphans = engine.scan_for_orphans();
    println!(
        "  scan_for_orphans: {:?} ({} hit)",
        t.elapsed(),
        orphans.len()
    );
    let t = Instant::now();
    for path in &orphans {
        if which == Before::Mobile {
            // Mobile's shape: blocking acquire, no yield, next page.
            let mut guard = slot.lock();
            let Some(ws) = guard.as_mut() else { return };
            let _ = outl_md::reconcile::reconcile_md(ws, hlc, path, Some(&orphans_log));
            continue;
        }
        let took = loop {
            let Some(mut guard) = slot.try_lock() else {
                thread::sleep(Duration::from_millis(20));
                continue;
            };
            let Some(ws) = guard.as_mut() else { return };
            let started = Instant::now();
            let _ = outl_md::reconcile::reconcile_md(ws, hlc, path, Some(&orphans_log));
            break started.elapsed();
        };
        yield_to_user(took);
    }
    println!("  orphan reconcile loop: {:?}", t.elapsed());

    if which != Before::Desktop {
        let t = Instant::now();
        let desynced = {
            let mut guard = slot.lock();
            let Some(ws) = guard.as_mut() else { return };
            engine.scan_for_desynced_projections(ws)
        };
        println!(
            "  scan_for_desynced_projections: {:?} [LOCKED] ({} hit)",
            t.elapsed(),
            desynced.len()
        );
        for path in &desynced {
            let mut guard = slot.lock();
            let Some(ws) = guard.as_mut() else { return };
            let _ = outl_actions::recover_desynced_projection(ws, hlc, root, path);
        }
    }

    let t = Instant::now();
    {
        let mut guard = slot.lock();
        let Some(ws) = guard.as_mut() else { return };
        let _ = outl_actions::repair_doubled_journal_titles(ws, hlc);
    }
    println!(
        "  repair_doubled_journal_titles: {:?} [LOCKED]",
        t.elapsed()
    );

    let t = Instant::now();
    {
        let mut guard = slot.lock();
        let Some(ws) = guard.as_mut() else { return };
        let _ = outl_actions::repair_namespaced_titles(ws, hlc);
    }
    println!("  repair_namespaced_titles: {:?} [LOCKED]", t.elapsed());
}
