//! `seshat-progress`: prints Seshat's file operations as they happen.
//!
//! Demonstrates the IPC protocol from an independent process:
//! connect, subscribe, snapshot, apply signals in sequence order, detect
//! gaps and resynchronise, report completion, and survive service
//! restarts and lost connections.
//!
//! Usage: `seshat-progress [--address <dbus-address>] [--exit-when-idle]`
//! (default: the session bus).

use std::collections::BTreeMap;
use std::time::Duration;

use futures_util::StreamExt;
use futures_util::stream::{self, Stream};
use serde::{Deserialize, Serialize};
use zbus::{Connection, MatchRule, MessageStream, Type, proxy};

const BUS_NAME: &str = "dev.kdmorris.Seshat.Operations";
const INTERFACE: &str = "dev.kdmorris.Seshat.Operations";
const PROTOCOL_VERSION: u32 = 1;
const RETRY: Duration = Duration::from_secs(2);

/// Mirrors the protocol's `OperationInfo`, signature
/// `(sssasussttbttbdbtttsu)`. Field order is what matters on the wire.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
struct OperationInfo {
    id: String,
    kind: String,
    status: String,
    sources: Vec<String>,
    source_count: u32,
    destination: String,
    current: String,
    files_completed: u64,
    files_total: u64,
    files_total_known: bool,
    bytes_completed: u64,
    bytes_total: u64,
    bytes_total_known: bool,
    percentage: f64,
    awaiting_decision: bool,
    queued_at_usec: u64,
    started_at_usec: u64,
    elapsed_ms: u64,
    error: String,
    failure_count: u32,
}

impl OperationInfo {
    fn is_finished(&self) -> bool {
        matches!(self.status.as_str(), "completed" | "failed" | "cancelled")
    }
}

#[proxy(
    interface = "dev.kdmorris.Seshat.Operations",
    default_service = "dev.kdmorris.Seshat.Operations",
    default_path = "/dev/kdmorris/Seshat/Operations"
)]
trait Operations {
    fn get_protocol_version(&self) -> zbus::Result<u32>;
    fn get_active_operations(&self) -> zbus::Result<(u64, Vec<OperationInfo>)>;
}

struct Options {
    address: Option<String>,
    exit_when_idle: bool,
}

fn parse_args() -> Result<Options, String> {
    let mut options = Options {
        address: None,
        exit_when_idle: false,
    };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--address" => options.address = Some(args.next().ok_or("--address needs a value")?),
            "--exit-when-idle" => options.exit_when_idle = true,
            "-h" | "--help" => {
                return Err(
                    "usage: seshat-progress [--address <dbus-address>] [--exit-when-idle]".into(),
                );
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(options)
}

fn main() {
    let options = match parse_args() {
        Ok(options) => options,
        Err(message) => {
            eprintln!("{message}");
            std::process::exit(2);
        }
    };
    zbus::block_on(run(options));
}

/// Reconnects forever (or until idle with `--exit-when-idle`).
async fn run(options: Options) {
    loop {
        match follow(&options).await {
            Ok(Outcome::Idle) => return,
            Ok(Outcome::ServiceGone) => println!("· Seshat stopped; waiting for it to return"),
            Err(err) => println!("· connection problem ({err}); retrying"),
        }
        // No connection is alive between attempts, so blocking is fine.
        std::thread::sleep(RETRY);
    }
}

enum Outcome {
    Idle,
    ServiceGone,
}

enum Event {
    Signal(String, u64, Box<OperationInfo>),
    OwnerChanged { new_owner: String },
}

async fn connect(options: &Options) -> zbus::Result<Connection> {
    match &options.address {
        Some(address) => {
            zbus::connection::Builder::address(address.as_str())
                .build()
                .await
        }
        None => Connection::session().await,
    }
}

/// One connection's lifetime: subscribe first, then snapshot, then apply
/// signals newer than the snapshot. Any gap triggers a fresh snapshot.
async fn follow(options: &Options) -> zbus::Result<Outcome> {
    let connection = connect(options).await?;
    let mut events = subscribe(&connection).await?;

    // Wait for the service.
    let dbus = zbus::fdo::DBusProxy::new(&connection).await?;
    while !dbus.name_has_owner(BUS_NAME.try_into()?).await? {
        if options.exit_when_idle {
            println!("· Seshat is not running");
            return Ok(Outcome::Idle);
        }
        match events.next().await {
            Some(Event::OwnerChanged { new_owner }) if !new_owner.is_empty() => break,
            Some(_) => {}
            None => return Err(zbus::Error::Failure("bus connection closed".into())),
        }
    }

    let operations = OperationsProxy::new(&connection).await?;
    let version = operations.get_protocol_version().await?;
    if version != PROTOCOL_VERSION {
        return Err(zbus::Error::Failure(format!(
            "unsupported protocol version {version} (expected {PROTOCOL_VERSION})"
        )));
    }

    let mut state = BTreeMap::new();
    let mut last = resync(&operations, &mut state).await?;
    if options.exit_when_idle && state.is_empty() {
        return Ok(Outcome::Idle);
    }

    while let Some(event) = events.next().await {
        match event {
            Event::OwnerChanged { new_owner } if new_owner.is_empty() => {
                return Ok(Outcome::ServiceGone);
            }
            Event::OwnerChanged { .. } => {
                // A new instance: its sequence numbers start afresh.
                last = resync(&operations, &mut state).await?;
            }
            Event::Signal(_, sequence, _) if sequence <= last => {} // already in the snapshot
            Event::Signal(name, sequence, operation) => {
                if sequence > last + 1 {
                    println!("· missed {} event(s); resynchronising", sequence - last - 1);
                    last = resync(&operations, &mut state).await?;
                    if sequence <= last {
                        continue;
                    }
                }
                last = sequence;
                let operation = *operation;
                report(&name, &operation);
                if operation.is_finished() {
                    state.remove(&operation.id);
                } else {
                    state.insert(operation.id.clone(), operation);
                }
                if options.exit_when_idle && state.is_empty() {
                    return Ok(Outcome::Idle);
                }
            }
        }
    }
    Err(zbus::Error::Failure("bus connection closed".into()))
}

/// Subscribes to the interface's signals and to ownership changes of the
/// service name — before taking any snapshot, so nothing falls in between.
async fn subscribe(connection: &Connection) -> zbus::Result<impl Stream<Item = Event> + Unpin> {
    let signals = MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .interface(INTERFACE)
        .build()?;
    let owners = MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender("org.freedesktop.DBus")
        .member("NameOwnerChanged")
        .arg(0, BUS_NAME)
        .build()?;
    let signals = MessageStream::for_match_rule(signals, connection, None)
        .await?
        .filter_map(|message| async move {
            let message = message.ok()?;
            let name = message.header().member()?.to_string();
            let (sequence, operation) =
                message.body().deserialize::<(u64, OperationInfo)>().ok()?;
            Some(Event::Signal(name, sequence, Box::new(operation)))
        });
    let owners = MessageStream::for_match_rule(owners, connection, None)
        .await?
        .filter_map(|message| async move {
            let message = message.ok()?;
            let (_, _, new_owner) = message
                .body()
                .deserialize::<(String, String, String)>()
                .ok()?;
            Some(Event::OwnerChanged { new_owner })
        });
    Ok(Box::pin(stream::select(signals, owners)))
}

async fn resync(
    operations: &OperationsProxy<'_>,
    state: &mut BTreeMap<String, OperationInfo>,
) -> zbus::Result<u64> {
    let (sequence, active) = operations.get_active_operations().await?;
    state.clear();
    println!("· {} active operation(s)", active.len());
    for operation in active {
        report("Snapshot", &operation);
        state.insert(operation.id.clone(), operation);
    }
    Ok(sequence)
}

fn describe(op: &OperationInfo) -> String {
    let what = match op.sources.as_slice() {
        [single] => single.rsplit('/').next().unwrap_or(single).to_owned(),
        _ => format!("{} items", op.source_count),
    };
    let to = op
        .destination
        .rsplit('/')
        .next()
        .filter(|d| !d.is_empty())
        .map(|d| format!(" → {d}"))
        .unwrap_or_default();
    format!("{} {what}{to}", op.kind)
}

fn report(signal: &str, op: &OperationInfo) {
    let short_id = &op.id[..op.id.len().min(8)];
    let line = match signal {
        "OperationCompleted" if op.failure_count > 0 => {
            format!("✔ completed with {} problem(s)", op.failure_count)
        }
        "OperationCompleted" => "✔ completed".to_owned(),
        "OperationFailed" => format!("✖ failed: {}", op.error),
        "OperationCancelled" => "⨯ cancelled".to_owned(),
        "OperationCancelling" => "… cancelling".to_owned(),
        _ if op.awaiting_decision => "? waiting for a decision in Seshat".to_owned(),
        _ if op.percentage >= 0.0 => format!(
            "{:5.1}%  {} of {} items",
            op.percentage, op.files_completed, op.files_total
        ),
        _ => format!("{}  {} items so far", op.status, op.files_completed),
    };
    println!("[{short_id}] {:<40} {line}", describe(op));
}
