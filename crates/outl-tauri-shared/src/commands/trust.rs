//! The commands behind "trust you can see": who wrote what, and taking
//! your notes with you.
//!
//! Both are thin wrappers. `outl_actions::activity` owns the per-device
//! summary and `outl_actions::export_md` owns what an export copies.

use std::path::PathBuf;

use serde::Serialize;

use crate::helpers::with_ws;
use crate::host::AppHost;

/// One device's share of the op log.
#[derive(Debug, Clone, Serialize)]
pub struct ActorActivityDto {
    /// The actor id (ULID). Not a peer node id: the two are different
    /// identities, so the client labels these by recency, not by alias.
    pub actor: String,
    /// Ops from this actor in the resident log.
    pub ops: usize,
    /// Wall-clock ms of its newest op, `0` when it has none.
    pub last_at_ms: u64,
    /// Whether this is the device asking.
    pub is_local: bool,
}

/// The whole summary.
#[derive(Debug, Clone, Serialize)]
pub struct DeviceActivityDto {
    /// Most recently active first.
    pub actors: Vec<ActorActivityDto>,
    /// `false` when the counts only cover the ops since the last snapshot.
    pub complete: bool,
}

/// What the op log holds from each device.
pub fn device_activity<S: AppHost>(state: &S) -> Result<DeviceActivityDto, String> {
    let local = state.hlc().actor();
    with_ws(state, |ws| {
        let activity = outl_actions::device_activity(ws, local);
        Ok(DeviceActivityDto {
            complete: activity.complete,
            actors: activity
                .actors
                .into_iter()
                .map(|a| ActorActivityDto {
                    actor: a.actor.0.to_string(),
                    ops: a.ops,
                    last_at_ms: a.last_at_ms,
                    is_local: a.is_local,
                })
                .collect(),
        })
    })
}

/// Where an export landed.
#[derive(Debug, Clone, Serialize)]
pub struct ExportReportDto {
    /// Absolute path of the new folder.
    pub dir: String,
    /// `.md` files copied.
    pub pages: usize,
    /// Asset files copied.
    pub assets: usize,
}

/// Copy the workspace's Markdown and assets into a new folder under
/// `dest_dir`, or next to the workspace folder when `None`. Next to the
/// workspace is the default on every client because it is the one place
/// the user already knows how to find: `~/Documents` on a desktop, and
/// the outl folder in iCloud Drive on iOS, which the Files app shows.
pub fn export_markdown<S: AppHost>(
    state: &S,
    dest_dir: Option<String>,
) -> Result<ExportReportDto, String> {
    let root = state.storage_root()?;
    let dest = match dest_dir {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => root
            .parent()
            .map(PathBuf::from)
            .ok_or_else(|| format!("{} has no parent folder", root.display()))?,
    };
    let stamp = chrono::Local::now().format("%Y-%m-%d-%H%M%S").to_string();
    let report = outl_actions::export_markdown(&root, &dest, &stamp)
        .map_err(|e| format!("could not export to {}: {e}", dest.display()))?;
    Ok(ExportReportDto {
        dir: report.dir.to_string_lossy().into_owned(),
        pages: report.pages,
        assets: report.assets,
    })
}
