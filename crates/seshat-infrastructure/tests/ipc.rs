//! D-Bus protocol tests against a private `dbus-daemon` (never the user's
//! session bus). An independent client checks discovery, snapshots,
//! signal sequencing, cancellation, errors, reconnection and restarts.
//!
//! Skipped (passing) when `dbus-daemon` is not installed.

use std::ffi::OsString;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Condvar, Mutex, mpsc};
use std::time::Duration;

use futures_util::StreamExt;
use seshat_application::operations::{
    FileSystem, FsError, FsErrorKind, FsMetadata, OperationManager, OperationRequest,
};
use seshat_infrastructure::filesystem::LocalFileSystem;
use seshat_infrastructure::ipc::{INTERFACE, IpcConfig, IpcService, OperationInfo};

const TIMEOUT: Duration = Duration::from_secs(10);

// ── Private bus ─────────────────────────────────────────────────────────────

struct Bus {
    daemon: Child,
    address: String,
    _dir: tempfile::TempDir,
}

impl Drop for Bus {
    fn drop(&mut self) {
        let _ = self.daemon.kill();
        let _ = self.daemon.wait();
    }
}

/// A minimal session-bus configuration, so tests do not depend on the
/// host's `/etc/dbus-1` (absent in build sandboxes).
const BUS_CONFIG: &str = r#"<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-BUS Bus Configuration 1.0//EN"
 "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig>
  <type>session</type>
  <listen>unix:path=SOCKET</listen>
  <auth>EXTERNAL</auth>
  <policy context="default">
    <allow send_destination="*" eavesdrop="true"/>
    <allow eavesdrop="true"/>
    <allow own="*"/>
  </policy>
</busconfig>
"#;

/// `None` when `dbus-daemon` is not installed (the tests then skip). A
/// daemon that is installed but fails to start is a test failure.
fn private_bus() -> Option<Bus> {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("bus.conf");
    let socket = dir.path().join("bus");
    std::fs::write(
        &config,
        BUS_CONFIG.replace("SOCKET", &socket.to_string_lossy()),
    )
    .unwrap();
    let mut daemon = match Command::new("dbus-daemon")
        .arg(format!("--config-file={}", config.display()))
        .args(["--nofork", "--nopidfile", "--print-address=1"])
        .stdout(Stdio::piped())
        .spawn()
    {
        Ok(daemon) => daemon,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return None,
        Err(err) => panic!("cannot run dbus-daemon: {err}"),
    };
    let mut line = String::new();
    BufReader::new(daemon.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    let address = line.trim().to_owned();
    assert!(!address.is_empty(), "dbus-daemon did not start");
    Some(Bus {
        daemon,
        address,
        _dir: dir,
    })
}

macro_rules! bus_or_skip {
    () => {
        match private_bus() {
            Some(bus) => bus,
            None => {
                eprintln!("dbus-daemon not available; skipping");
                return;
            }
        }
    };
}

// ── A filesystem whose copies wait at a gate ────────────────────────────────

#[derive(Default)]
struct Gate {
    open: Mutex<bool>,
    changed: Condvar,
}

struct GatedFs {
    inner: LocalFileSystem,
    gate: Arc<Gate>,
}

impl FileSystem for GatedFs {
    fn metadata(&self, p: &Path) -> Result<FsMetadata, FsError> {
        self.inner.metadata(p)
    }
    fn canonicalize(&self, p: &Path) -> Result<PathBuf, FsError> {
        self.inner.canonicalize(p)
    }
    fn read_dir(&self, p: &Path) -> Result<Vec<OsString>, FsError> {
        self.inner.read_dir(p)
    }
    fn create_dir(&self, p: &Path) -> Result<(), FsError> {
        self.inner.create_dir(p)
    }
    fn create_file(&self, p: &Path) -> Result<(), FsError> {
        self.inner.create_file(p)
    }
    fn copy_file(
        &self,
        from: &Path,
        to: &Path,
        progress: &mut dyn FnMut(u64) -> bool,
    ) -> Result<(), FsError> {
        loop {
            let open = self.gate.open.lock().unwrap();
            if *open {
                break;
            }
            let _ = self
                .gate
                .changed
                .wait_timeout(open, Duration::from_millis(10))
                .unwrap();
            if !progress(0) {
                return Err(FsError::new(FsErrorKind::Cancelled, "cancelled"));
            }
        }
        self.inner.copy_file(from, to, progress)
    }
    fn copy_symlink(&self, f: &Path, t: &Path) -> Result<(), FsError> {
        self.inner.copy_symlink(f, t)
    }
    fn copy_dir_attributes(&self, f: &Path, t: &Path) -> Result<(), FsError> {
        self.inner.copy_dir_attributes(f, t)
    }
    fn rename(&self, f: &Path, t: &Path, r: bool) -> Result<(), FsError> {
        self.inner.rename(f, t, r)
    }
    fn remove_file(&self, p: &Path) -> Result<(), FsError> {
        self.inner.remove_file(p)
    }
    fn remove_empty_dir(&self, p: &Path) -> Result<(), FsError> {
        self.inner.remove_empty_dir(p)
    }
    fn available_space(&self, p: &Path) -> Option<u64> {
        self.inner.available_space(p)
    }
    fn trash(&self, _: &Path) -> Result<(), FsError> {
        Err(FsError::new(FsErrorKind::NotSupported, "not in tests"))
    }
    fn trash_origin(&self, _: &Path) -> Result<PathBuf, FsError> {
        Err(FsError::new(FsErrorKind::NotSupported, "not in tests"))
    }
    fn restore(&self, _: &Path, _: &Path, _: bool) -> Result<(), FsError> {
        Err(FsError::new(FsErrorKind::NotSupported, "not in tests"))
    }
    fn delete_trashed(&self, _: &Path) -> Result<(), FsError> {
        Err(FsError::new(FsErrorKind::NotSupported, "not in tests"))
    }
}

struct Fixture {
    _bus: Bus,
    address: String,
    gate: Arc<Gate>,
    manager: OperationManager,
    service: Option<IpcService>,
    dir: tempfile::TempDir,
}

impl Fixture {
    fn new(bus: Bus, max_concurrent: usize) -> Self {
        let gate = Arc::new(Gate::default());
        let manager = OperationManager::new(
            Arc::new(GatedFs {
                inner: LocalFileSystem::new(),
                gate: gate.clone(),
            }),
            max_concurrent,
        );
        let address = bus.address.clone();
        let service = IpcService::start(manager.clone(), config(&address)).expect("service starts");
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("dst")).unwrap();
        Self {
            _bus: bus,
            address,
            gate,
            manager,
            service: Some(service),
            dir,
        }
    }

    fn open_gate(&self) {
        *self.gate.open.lock().unwrap() = true;
        self.gate.changed.notify_all();
    }

    fn submit_copy(&self, name: &str) -> String {
        let source = self.dir.path().join(name);
        std::fs::write(&source, vec![b'x'; 4096]).unwrap();
        self.manager
            .submit(OperationRequest::Copy {
                sources: vec![source],
                destination: self.dir.path().join("dst"),
            })
            .unwrap()
            .to_string()
    }
}

fn config(address: &str) -> IpcConfig {
    IpcConfig {
        address: Some(address.to_owned()),
        ..IpcConfig::default()
    }
}

// ── An independent client ───────────────────────────────────────────────────

#[zbus::proxy(
    interface = "dev.kdmorris.Seshat.Operations",
    default_service = "dev.kdmorris.Seshat.Operations",
    default_path = "/dev/kdmorris/Seshat/Operations"
)]
trait Operations {
    fn get_protocol_version(&self) -> zbus::Result<u32>;
    fn get_active_operations(&self) -> zbus::Result<(u64, Vec<OperationInfo>)>;
    fn get_operation(&self, id: &str) -> zbus::Result<OperationInfo>;
    fn cancel_operation(&self, id: &str) -> zbus::Result<()>;
}

fn connect(address: &str) -> zbus::Connection {
    zbus::block_on(zbus::connection::Builder::address(address).build()).expect("client connects")
}

fn proxy(connection: &zbus::Connection) -> OperationsProxy<'static> {
    zbus::block_on(OperationsProxy::new(connection)).unwrap()
}

#[derive(Debug, Clone)]
struct Signal {
    name: String,
    sequence: u64,
    operation: OperationInfo,
}

/// Subscribes (AddMatch) before returning, then forwards signals.
fn subscribe(address: &str) -> mpsc::Receiver<Signal> {
    let (tx, rx) = mpsc::channel();
    let (ready_tx, ready_rx) = mpsc::channel();
    let address = address.to_owned();
    std::thread::spawn(move || {
        zbus::block_on(async move {
            let connection = zbus::connection::Builder::address(address.as_str())
                .build()
                .await
                .unwrap();
            let rule = zbus::MatchRule::builder()
                .msg_type(zbus::message::Type::Signal)
                .interface(INTERFACE)
                .build()
                .unwrap();
            let mut stream = zbus::MessageStream::for_match_rule(rule, &connection, None)
                .await
                .unwrap();
            let _ = ready_tx.send(());
            while let Some(Ok(message)) = stream.next().await {
                let header = message.header();
                let name = header.member().map(|m| m.to_string()).unwrap_or_default();
                if let Ok((sequence, operation)) =
                    message.body().deserialize::<(u64, OperationInfo)>()
                    && tx
                        .send(Signal {
                            name,
                            sequence,
                            operation,
                        })
                        .is_err()
                {
                    break;
                }
            }
        });
    });
    ready_rx.recv_timeout(TIMEOUT).expect("subscribed");
    rx
}

/// Signals until `id` reaches a terminal state.
fn until_finished(rx: &mpsc::Receiver<Signal>, id: &str) -> Vec<Signal> {
    let mut seen = Vec::new();
    loop {
        let signal = rx.recv_timeout(TIMEOUT).expect("signal");
        let done = signal.operation.id == id
            && ["completed", "failed", "cancelled"].contains(&signal.operation.status.as_str());
        seen.push(signal);
        if done {
            return seen;
        }
    }
}

fn error_name(err: zbus::Error) -> String {
    match err {
        zbus::Error::MethodError(name, _, _) => name.to_string(),
        other => format!("unexpected: {other}"),
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[test]
fn discovery_snapshot_and_ordered_signals() {
    let fx = Fixture::new(bus_or_skip!(), 2);
    let signals = subscribe(&fx.address);
    let client = connect(&fx.address);
    let ops = proxy(&client);

    assert_eq!(zbus::block_on(ops.get_protocol_version()).unwrap(), 1);
    let id = fx.submit_copy("a.bin");
    // Wait until it is running, then snapshot.
    loop {
        let s = signals.recv_timeout(TIMEOUT).unwrap();
        if s.name == "OperationStarted" {
            break;
        }
    }
    let (sequence, active) = zbus::block_on(ops.get_active_operations()).unwrap();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].id, id);
    assert_eq!(active[0].kind, "copy");
    assert!(sequence >= 2, "queued + started already happened");

    fx.open_gate();
    let rest = until_finished(&signals, &id);
    let last = rest.last().unwrap();
    assert_eq!(last.name, "OperationCompleted");
    assert_eq!(last.operation.percentage, 100.0);
    assert!(last.sequence > sequence);
    assert!(
        rest.windows(2).all(|w| w[1].sequence == w[0].sequence + 1),
        "no gaps, in order"
    );

    let done = zbus::block_on(ops.get_operation(&id)).unwrap();
    assert_eq!(done.status, "completed");
    assert!(
        zbus::block_on(ops.get_active_operations())
            .unwrap()
            .1
            .is_empty()
    );
}

#[test]
fn cancellation_and_structured_errors() {
    let fx = Fixture::new(bus_or_skip!(), 1);
    let signals = subscribe(&fx.address);
    let client = connect(&fx.address);
    let ops = proxy(&client);

    let id = fx.submit_copy("b.bin");
    zbus::block_on(ops.cancel_operation(&id)).unwrap();
    let seen = until_finished(&signals, &id);
    let names: Vec<_> = seen.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"OperationCancelling"), "{names:?}");
    assert_eq!(names.last(), Some(&"OperationCancelled"));

    let prefix = "dev.kdmorris.Seshat.Operations.Error.";
    assert_eq!(
        error_name(zbus::block_on(ops.cancel_operation(&id)).unwrap_err()),
        format!("{prefix}AlreadyFinished")
    );
    let unknown = uuid::Uuid::new_v4().to_string();
    assert_eq!(
        error_name(zbus::block_on(ops.cancel_operation(&unknown)).unwrap_err()),
        format!("{prefix}NotFound")
    );
    assert_eq!(
        error_name(zbus::block_on(ops.get_operation("../../etc/passwd")).unwrap_err()),
        format!("{prefix}InvalidArgument")
    );
}

#[test]
fn multiple_operations_and_queueing_are_visible() {
    let fx = Fixture::new(bus_or_skip!(), 1);
    let client = connect(&fx.address);
    let ops = proxy(&client);
    let first = fx.submit_copy("c.bin");
    let second = fx.submit_copy("d.bin");
    let (_, active) = zbus::block_on(ops.get_active_operations()).unwrap();
    let status = |id: &str| active.iter().find(|o| o.id == id).map(|o| o.status.clone());
    assert_eq!(active.len(), 2);
    assert_ne!(status(&first).as_deref(), Some("queued"));
    assert_eq!(status(&second).as_deref(), Some("queued"));
    fx.open_gate();
}

#[test]
fn reconnecting_client_recovers_state_from_a_snapshot() {
    let fx = Fixture::new(bus_or_skip!(), 1);
    let id = fx.submit_copy("e.bin");
    {
        let client = connect(&fx.address);
        let (_, active) = zbus::block_on(proxy(&client).get_active_operations()).unwrap();
        assert_eq!(active.len(), 1);
    } // client disconnects
    let client = connect(&fx.address);
    let (_, active) = zbus::block_on(proxy(&client).get_active_operations()).unwrap();
    assert_eq!(active[0].id, id, "state survives client reconnects");
    fx.open_gate();
}

#[test]
fn service_restart_releases_and_reclaims_the_name() {
    let mut fx = Fixture::new(bus_or_skip!(), 1);
    let client = connect(&fx.address);
    let dbus = zbus::block_on(zbus::fdo::DBusProxy::new(&client)).unwrap();
    let name = zbus::names::BusName::try_from(seshat_infrastructure::ipc::BUS_NAME).unwrap();
    let has_owner = || zbus::block_on(dbus.name_has_owner(name.clone())).unwrap();
    assert!(has_owner());

    // A second instance cannot take over the name.
    assert!(IpcService::start(fx.manager.clone(), config(&fx.address)).is_err());

    fx.service = None;
    let mut released = false;
    for _ in 0..100 {
        if !has_owner() {
            released = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(released, "name released when the service stops");

    fx.service = Some(IpcService::start(fx.manager.clone(), config(&fx.address)).unwrap());
    assert!(has_owner());
    let (_, active) = zbus::block_on(proxy(&client).get_active_operations()).unwrap();
    assert!(active.is_empty());
}
