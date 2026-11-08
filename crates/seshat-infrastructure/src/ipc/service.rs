//! The D-Bus service: methods answer from the manager's snapshot, signals
//! mirror its event stream one-to-one (same sequence numbers).

use std::time::SystemTime;

use seshat_application::operations::{
    CancelError, OperationEvent, OperationEventKind, OperationManager, SubscriptionId,
};
use seshat_domain::operation::OperationId;
use zbus::message::Header;
use zbus::object_server::SignalEmitter;
use zbus::{Connection, DBusError, interface};

use super::types::OperationInfo;
use super::{BUS_NAME, INTERFACE, OBJECT_PATH, PROTOCOL_VERSION};

/// Longest operation id accepted (a hyphenated UUID is 36 characters).
const MAX_ID_LEN: usize = 64;

/// Where to serve.
#[derive(Debug, Clone)]
pub struct IpcConfig {
    /// Bus address; `None` means the user's session bus.
    pub address: Option<String>,
    pub bus_name: String,
    pub object_path: String,
}

impl Default for IpcConfig {
    fn default() -> Self {
        Self {
            address: None,
            bus_name: BUS_NAME.to_owned(),
            object_path: OBJECT_PATH.to_owned(),
        }
    }
}

/// Errors returned to D-Bus callers. Names are
/// `dev.kdmorris.Seshat.Operations.Error.<Variant>`.
#[derive(Debug, DBusError)]
#[zbus(prefix = "dev.kdmorris.Seshat.Operations.Error")]
pub enum IpcError {
    #[zbus(error)]
    ZBus(zbus::Error),
    /// No operation with this id (it may have finished long ago).
    NotFound(String),
    /// The operation already reached a terminal state.
    AlreadyFinished(String),
    /// Malformed argument (e.g. not an operation id).
    InvalidArgument(String),
    /// The caller is not the user running Seshat.
    AccessDenied(String),
}

struct Operations {
    manager: OperationManager,
}

/// Session buses are per user, but a bus can be shared or misconfigured:
/// only the user running Seshat may observe or control its operations.
async fn authorize(connection: &Connection, header: &Header<'_>) -> Result<(), IpcError> {
    let Some(sender) = header.sender() else {
        return Err(IpcError::AccessDenied("anonymous caller".to_owned()));
    };
    let bus = zbus::fdo::DBusProxy::new(connection).await?;
    let caller = bus
        .get_connection_unix_user(sender.clone().into())
        .await
        .map_err(|e| IpcError::AccessDenied(format!("cannot identify caller: {e}")))?;
    if caller == rustix::process::getuid().as_raw() {
        Ok(())
    } else {
        tracing::warn!(caller, sender = %sender, "rejected D-Bus call from another user");
        Err(IpcError::AccessDenied(
            "caller is a different user".to_owned(),
        ))
    }
}

fn parse_id(id: &str) -> Result<OperationId, IpcError> {
    if id.len() > MAX_ID_LEN {
        return Err(IpcError::InvalidArgument(
            "operation id too long".to_owned(),
        ));
    }
    id.parse::<uuid::Uuid>()
        .map(OperationId::from)
        .map_err(|_| IpcError::InvalidArgument(format!("not an operation id: {id:?}")))
}

#[interface(name = "dev.kdmorris.Seshat.Operations")]
impl Operations {
    /// Version of this protocol (see `docs/ipc/protocol.md`).
    fn get_protocol_version(&self) -> u32 {
        PROTOCOL_VERSION
    }

    /// Unfinished operations and the event sequence number they reflect.
    /// Events with a higher sequence happened after this snapshot.
    async fn get_active_operations(
        &self,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
    ) -> Result<(u64, Vec<OperationInfo>), IpcError> {
        authorize(connection, &header).await?;
        let snapshot = self.manager.snapshot();
        let now = SystemTime::now();
        Ok((
            snapshot.sequence,
            snapshot
                .operations
                .iter()
                .map(|op| OperationInfo::from_operation(op, now))
                .collect(),
        ))
    }

    /// One operation, active or recently finished.
    async fn get_operation(
        &self,
        id: &str,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
    ) -> Result<OperationInfo, IpcError> {
        authorize(connection, &header).await?;
        let id = parse_id(id)?;
        self.manager
            .get(id)
            .map(|op| OperationInfo::from_operation(&op, SystemTime::now()))
            .ok_or_else(|| IpcError::NotFound(id.to_string()))
    }

    /// Requests cancellation; progress continues until the operation
    /// reports `OperationCancelled` (or completes first).
    async fn cancel_operation(
        &self,
        id: &str,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
    ) -> Result<(), IpcError> {
        authorize(connection, &header).await?;
        let id = parse_id(id)?;
        tracing::info!(operation = %id, sender = ?header.sender(), "cancellation requested over D-Bus");
        self.manager.cancel(id).map_err(|e| match e {
            CancelError::NotFound => IpcError::NotFound(id.to_string()),
            CancelError::AlreadyFinished => IpcError::AlreadyFinished(id.to_string()),
        })
    }

    #[zbus(signal)]
    async fn operation_queued(
        e: &SignalEmitter<'_>,
        sequence: u64,
        operation: OperationInfo,
    ) -> zbus::Result<()>;
    #[zbus(signal)]
    async fn operation_started(
        e: &SignalEmitter<'_>,
        sequence: u64,
        operation: OperationInfo,
    ) -> zbus::Result<()>;
    #[zbus(signal)]
    async fn operation_progress(
        e: &SignalEmitter<'_>,
        sequence: u64,
        operation: OperationInfo,
    ) -> zbus::Result<()>;
    #[zbus(signal)]
    async fn operation_cancelling(
        e: &SignalEmitter<'_>,
        sequence: u64,
        operation: OperationInfo,
    ) -> zbus::Result<()>;
    #[zbus(signal)]
    async fn operation_completed(
        e: &SignalEmitter<'_>,
        sequence: u64,
        operation: OperationInfo,
    ) -> zbus::Result<()>;
    #[zbus(signal)]
    async fn operation_failed(
        e: &SignalEmitter<'_>,
        sequence: u64,
        operation: OperationInfo,
    ) -> zbus::Result<()>;
    #[zbus(signal)]
    async fn operation_cancelled(
        e: &SignalEmitter<'_>,
        sequence: u64,
        operation: OperationInfo,
    ) -> zbus::Result<()>;
}

async fn emit(emitter: &SignalEmitter<'_>, event: &OperationEvent) -> zbus::Result<()> {
    let info = OperationInfo::from_operation(&event.operation, SystemTime::now());
    let seq = event.sequence;
    match event.kind {
        OperationEventKind::Queued => Operations::operation_queued(emitter, seq, info).await,
        OperationEventKind::Started => Operations::operation_started(emitter, seq, info).await,
        OperationEventKind::Progress => Operations::operation_progress(emitter, seq, info).await,
        OperationEventKind::Cancelling => {
            Operations::operation_cancelling(emitter, seq, info).await
        }
        OperationEventKind::Completed => Operations::operation_completed(emitter, seq, info).await,
        OperationEventKind::Failed => Operations::operation_failed(emitter, seq, info).await,
        OperationEventKind::Cancelled => Operations::operation_cancelled(emitter, seq, info).await,
    }
}

/// A running D-Bus service. Dropping it stops emitting and releases the
/// bus name.
#[derive(Debug)]
pub struct IpcService {
    manager: OperationManager,
    subscription: SubscriptionId,
}

impl IpcService {
    /// Connects, claims the bus name, and starts mirroring manager events
    /// as signals on a dedicated thread. Fails if the bus is unreachable or
    /// the name is taken (e.g. another Seshat instance).
    pub fn start(manager: OperationManager, config: IpcConfig) -> Result<Self, zbus::Error> {
        // Manager subscribers run under its locks: only enqueue here. The
        // queue is unbounded so lifecycle events are never dropped; progress
        // is already throttled by the manager.
        let (events_tx, events_rx) = async_channel::unbounded::<OperationEvent>();
        let subscription = manager.subscribe(Box::new(move |event| {
            let _ = events_tx.try_send(event.clone());
        }));

        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let service = Operations {
            manager: manager.clone(),
        };
        let thread_config = config.clone();
        let spawned = std::thread::Builder::new()
            .name("seshat-ipc".to_owned())
            .spawn(move || {
                zbus::block_on(async move {
                    let connection = match connect(&thread_config, service).await {
                        Ok(connection) => {
                            let _ = ready_tx.send(Ok(()));
                            connection
                        }
                        Err(err) => {
                            let _ = ready_tx.send(Err(err));
                            return;
                        }
                    };
                    let emitter = match SignalEmitter::new(&connection, thread_config.object_path.as_str()) {
                        Ok(emitter) => emitter,
                        Err(err) => {
                            tracing::error!(error = %err, "cannot create signal emitter");
                            return;
                        }
                    };
                    while let Ok(event) = events_rx.recv().await {
                        if let Err(err) = emit(&emitter, &event).await {
                            tracing::warn!(error = %err, sequence = event.sequence, "signal not sent");
                        }
                    }
                    tracing::debug!("D-Bus service stopped");
                });
            });

        let result = match spawned {
            Ok(_) => ready_rx
                .recv()
                .unwrap_or_else(|_| Err(zbus::Error::Failure("IPC thread exited".to_owned()))),
            Err(err) => Err(zbus::Error::Failure(err.to_string())),
        };
        match result {
            Ok(()) => {
                tracing::info!(bus_name = %config.bus_name, path = %config.object_path, interface = INTERFACE, "D-Bus service started");
                Ok(Self {
                    manager,
                    subscription,
                })
            }
            Err(err) => {
                manager.unsubscribe(subscription);
                Err(err)
            }
        }
    }
}

async fn connect(config: &IpcConfig, service: Operations) -> zbus::Result<Connection> {
    let builder = match &config.address {
        Some(address) => zbus::connection::Builder::address(address.as_str()),
        None => zbus::connection::Builder::session(),
    };
    builder
        // zbus defaults to replacing an existing owner and allowing itself
        // to be replaced: a second instance would silently hijack the name.
        .replace_existing_names(false)
        .allow_name_replacements(false)
        .name(config.bus_name.as_str())
        .serve_at(config.object_path.as_str(), service)
        .build()
        .await
}

impl Drop for IpcService {
    fn drop(&mut self) {
        // Dropping the subscription closes the channel; the thread then
        // ends and drops the connection, releasing the bus name.
        self.manager.unsubscribe(self.subscription);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_validated() {
        assert!(parse_id("not-a-uuid").is_err());
        assert!(parse_id(&"a".repeat(100)).is_err());
        let id = OperationId::new();
        assert_eq!(parse_id(&id.to_string()).unwrap(), id);
    }
}
