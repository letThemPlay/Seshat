use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use seshat_domain::entities::Location;
use seshat_domain::operation::{Operation, OperationStatus, Progress};
use zbus::Type;

/// At most this many sources are listed per operation; `source_count`
/// gives the total. Keeps messages small for huge selections.
pub const MAX_SOURCES: usize = 32;

/// Operation state on the bus. D-Bus signature
/// `(sssasussttbttbdbtttsu)`; field order is part of the protocol.
///
/// Paths are sent as UTF-8 strings; names that are not valid UTF-8 are
/// shown with replacement characters (display only).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct OperationInfo {
    pub id: String,
    /// `copy`, `move`, `rename`, `delete`, `trash`, `restore`,
    /// `create_directory`, `create_file`.
    pub kind: String,
    /// `queued`, `started`, `in_progress`, `cancelling`, `completed`,
    /// `failed`, `cancelled`.
    pub status: String,
    pub sources: Vec<String>,
    pub source_count: u32,
    /// Empty when the operation has no destination.
    pub destination: String,
    /// Item being processed; empty when none.
    pub current: String,
    pub files_completed: u64,
    pub files_total: u64,
    pub files_total_known: bool,
    pub bytes_completed: u64,
    pub bytes_total: u64,
    pub bytes_total_known: bool,
    /// 0–100, or -1 when the total is unknown (never a guess).
    pub percentage: f64,
    /// Waiting for the user to resolve a conflict.
    pub awaiting_decision: bool,
    /// Microseconds since the Unix epoch; 0 when not applicable.
    pub queued_at_usec: u64,
    pub started_at_usec: u64,
    pub elapsed_ms: u64,
    /// Why the operation failed as a whole; empty otherwise.
    pub error: String,
    /// Items that could not be processed (details stay in the app).
    pub failure_count: u32,
}

pub fn status_id(status: OperationStatus) -> &'static str {
    match status {
        OperationStatus::Queued => "queued",
        OperationStatus::Started => "started",
        OperationStatus::InProgress => "in_progress",
        OperationStatus::Cancelling => "cancelling",
        OperationStatus::Completed => "completed",
        OperationStatus::Failed => "failed",
        OperationStatus::Cancelled => "cancelled",
    }
}

fn location_text(location: &Location) -> String {
    match location {
        Location::Path(path) => path.to_string_lossy().into_owned(),
        Location::Trash(rel) => format!("trash:///{}", rel.to_string_lossy()),
    }
}

fn usec(time: Option<SystemTime>) -> u64 {
    time.and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| u64::try_from(d.as_micros()).unwrap_or(u64::MAX))
}

fn split(progress: Progress) -> (u64, u64, bool) {
    match progress {
        Progress::Determinate { completed, total } => (completed, total, true),
        Progress::Indeterminate { completed } => (completed, 0, false),
    }
}

impl OperationInfo {
    pub fn from_operation(op: &Operation, now: SystemTime) -> Self {
        let (files_completed, files_total, files_total_known) = split(op.files);
        let (bytes_completed, bytes_total, bytes_total_known) = split(op.bytes);
        // Bytes are the better measure when known; otherwise items.
        let percentage = match op.bytes {
            Progress::Determinate { total, .. } if total > 0 => op.bytes.percentage(),
            _ => op.files.percentage(),
        }
        .unwrap_or(-1.0);
        Self {
            id: op.id().to_string(),
            kind: op.kind().id().to_owned(),
            status: status_id(op.status()).to_owned(),
            sources: op
                .sources
                .iter()
                .take(MAX_SOURCES)
                .map(location_text)
                .collect(),
            source_count: u32::try_from(op.sources.len()).unwrap_or(u32::MAX),
            destination: op
                .destination
                .as_ref()
                .map(|d| d.to_string_lossy().into_owned())
                .unwrap_or_default(),
            current: op
                .current
                .as_ref()
                .map(|c| c.to_string_lossy().into_owned())
                .unwrap_or_default(),
            files_completed,
            files_total,
            files_total_known,
            bytes_completed,
            bytes_total,
            bytes_total_known,
            percentage,
            awaiting_decision: op.awaiting_decision,
            queued_at_usec: usec(Some(op.queued_at)),
            started_at_usec: usec(op.started_at),
            elapsed_ms: op
                .elapsed(now)
                .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX)),
            error: op.error.clone().unwrap_or_default(),
            failure_count: u32::try_from(op.failures.len()).unwrap_or(u32::MAX),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use seshat_domain::operation::{FileFailure, OperationId, OperationKind};
    use std::time::Duration;

    fn op(sources: usize) -> Operation {
        Operation::new(
            OperationId::new(),
            OperationKind::Copy,
            (0..sources)
                .map(|i| Location::Path(format!("/src/{i}").into()))
                .collect(),
            Some("/dst".into()),
            UNIX_EPOCH + Duration::from_secs(10),
        )
    }

    /// The `kind` and `status` strings are part of the protocol, as listed on
    /// [`OperationInfo`] and in `docs/ipc/protocol.md`.
    #[test]
    fn kind_and_status_strings_are_stable() {
        use OperationKind::*;
        let kinds = [
            Copy,
            Move,
            Rename,
            Delete,
            Trash,
            Restore,
            CreateDirectory,
            CreateFile,
        ];
        // A new kind stops this compiling until it is added above.
        for kind in kinds {
            match kind {
                Copy | Move | Rename | Delete | Trash | Restore | CreateDirectory | CreateFile => {}
            }
        }
        assert_eq!(
            kinds.map(OperationKind::id),
            [
                "copy",
                "move",
                "rename",
                "delete",
                "trash",
                "restore",
                "create_directory",
                "create_file"
            ]
        );
        assert_eq!(
            OperationStatus::ALL.map(status_id),
            [
                "queued",
                "started",
                "in_progress",
                "cancelling",
                "completed",
                "failed",
                "cancelled"
            ]
        );
    }

    #[test]
    fn signature_is_stable() {
        assert_eq!(
            OperationInfo::SIGNATURE.to_string(),
            "(sssasussttbttbdbtttsu)"
        );
    }

    #[test]
    fn indeterminate_progress_has_no_percentage() {
        let info = OperationInfo::from_operation(&op(1), UNIX_EPOCH);
        assert_eq!(info.percentage, -1.0);
        assert!(!info.files_total_known);
        assert_eq!(info.status, "queued");
        assert_eq!(info.queued_at_usec, 10_000_000);
        assert_eq!(info.started_at_usec, 0);
    }

    #[test]
    fn bytes_drive_percentage_when_known() {
        let mut o = op(1);
        o.files = Progress::Determinate {
            completed: 1,
            total: 4,
        };
        o.bytes = Progress::Determinate {
            completed: 75,
            total: 100,
        };
        assert_eq!(
            OperationInfo::from_operation(&o, UNIX_EPOCH).percentage,
            75.0
        );
        o.bytes = Progress::Determinate {
            completed: 0,
            total: 0,
        };
        assert_eq!(
            OperationInfo::from_operation(&o, UNIX_EPOCH).percentage,
            25.0
        );
    }

    #[test]
    fn sources_are_capped_and_failures_counted_without_paths() {
        let mut o = op(100);
        o.failures.push(FileFailure {
            path: "/secret/a".into(),
            message: "x".into(),
        });
        let info = OperationInfo::from_operation(&o, UNIX_EPOCH);
        assert_eq!(info.sources.len(), MAX_SOURCES);
        assert_eq!(info.source_count, 100);
        assert_eq!(info.failure_count, 1);
    }

    #[test]
    fn trash_sources_use_trash_uris() {
        let mut o = op(0);
        o.sources = vec![Location::Trash("a.txt".into())];
        let info = OperationInfo::from_operation(&o, UNIX_EPOCH);
        assert_eq!(info.sources, ["trash:///a.txt"]);
    }
}
