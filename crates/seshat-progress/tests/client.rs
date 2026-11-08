//! Runs the example client against Seshat's real D-Bus service on a
//! private bus: discovery, progress to completion, service stop and
//! restart. Skipped (passing) when `dbus-daemon` is not installed.

use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, mpsc};
use std::time::Duration;

use seshat_application::operations::{OperationManager, OperationRequest};
use seshat_infrastructure::filesystem::LocalFileSystem;
use seshat_infrastructure::ipc::{IpcConfig, IpcService};

const TIMEOUT: Duration = Duration::from_secs(15);

struct Killed(Child);

impl Drop for Killed {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
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

/// `None` when `dbus-daemon` is not installed (the tests then skip). The
/// daemon is dropped before its directory.
fn private_bus() -> Option<((Killed, tempfile::TempDir), String)> {
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
    Some(((Killed(daemon), dir), address))
}

/// Lines the client prints, readable with a timeout.
fn spawn_client(address: &str) -> (Killed, mpsc::Receiver<String>) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_seshat-progress"))
        .args(["--address", address])
        .stdout(Stdio::piped())
        .spawn()
        .expect("client starts");
    let stdout = child.stdout.take().unwrap();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    (Killed(child), rx)
}

fn expect_line(rx: &mpsc::Receiver<String>, needle: &str) -> String {
    let deadline = std::time::Instant::now() + TIMEOUT;
    let mut seen = Vec::new();
    while let Some(left) = deadline.checked_duration_since(std::time::Instant::now()) {
        match rx.recv_timeout(left) {
            Ok(line) if line.contains(needle) => return line,
            Ok(line) => seen.push(line),
            Err(_) => break,
        }
    }
    panic!("client never printed {needle:?}; got:\n{}", seen.join("\n"));
}

fn copy_request(dir: &std::path::Path, name: &str) -> OperationRequest {
    let source = dir.join(name);
    std::fs::create_dir(&source).unwrap();
    for i in 0..200 {
        std::fs::write(source.join(format!("f{i}")), b"data").unwrap();
    }
    std::fs::create_dir_all(dir.join("dst")).unwrap();
    OperationRequest::Copy {
        sources: vec![source],
        destination: dir.join("dst"),
    }
}

#[test]
fn client_follows_operations_across_service_restarts() {
    let Some((_bus, address)) = private_bus() else {
        eprintln!("dbus-daemon not available; skipping");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let manager = OperationManager::new(Arc::new(LocalFileSystem::new()), 2);
    let config = IpcConfig {
        address: Some(address.clone()),
        ..IpcConfig::default()
    };

    // The client starts first and waits for the service.
    let (_client, lines) = spawn_client(&address);
    std::thread::sleep(Duration::from_millis(300));
    let service = IpcService::start(manager.clone(), config.clone()).unwrap();
    expect_line(&lines, "0 active operation(s)");

    manager.submit(copy_request(dir.path(), "first")).unwrap();
    let done = expect_line(&lines, "✔ completed");
    assert!(done.contains("copy first → dst"), "{done}");

    drop(service);
    expect_line(&lines, "Seshat stopped");

    let _service = IpcService::start(manager.clone(), config).unwrap();
    expect_line(&lines, "0 active operation(s)");
    manager.submit(copy_request(dir.path(), "second")).unwrap();
    let done = expect_line(&lines, "✔ completed");
    assert!(done.contains("copy second → dst"), "{done}");
}

#[test]
fn exit_when_idle_without_a_service() {
    let Some((_bus, address)) = private_bus() else {
        eprintln!("dbus-daemon not available; skipping");
        return;
    };
    let output = Command::new(env!("CARGO_BIN_EXE_seshat-progress"))
        .args(["--address", &address, "--exit-when-idle"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("Seshat is not running"));
}
