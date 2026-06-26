use std::{env, net::SocketAddr, path::PathBuf, sync::Arc, time::Duration};

use anyhow::Result;
use base64::Engine as _;
use clap::{Parser, Subcommand, builder::TypedValueParser};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tracing_subscriber::EnvFilter;

use crate::{
    admin::{authz::ClientRegistry, protocol::*},
    cli::{
        args::{DaemonArgs, LogLevel},
        output, paths,
    },
    net::init_network,
    storage, util,
};

#[derive(Parser, Debug, Clone)]
#[command(
    name = "p2p-ddnsctl",
    version,
    about = "p2p-ddns local management client",
    long_about = None
)]
pub struct ClientArgs {
    /// Path to daemon's Unix socket; defaults to XDG_RUNTIME_DIR or /run.
    #[arg(long, value_name = "SOCKET_PATH", conflicts_with = "admin_http")]
    pub socket_path: Option<PathBuf>,

    /// Connect to the daemon over HTTP instead of the local socket (for example `127.0.0.1:8080`).
    #[arg(long, value_name = "ADMIN_HTTP", conflicts_with = "socket_path")]
    pub admin_http: Option<String>,

    /// Request timeout in seconds
    #[arg(long, default_value_t = 5)]
    pub timeout: u64,

    /// Optional network ticket. Current local daemons allow ticketless Unix socket access.
    #[arg(short, long, value_name = "TICKET")]
    pub ticket: Option<String>,

    /// Log level, default is info
    #[arg(
        long,
        short = 'L',
        default_value_t = LogLevel::Info,
        value_parser = clap::builder::PossibleValuesParser::new(["trace", "debug", "info", "warn", "error", "off"])
            .map(|s: String| s.parse::<LogLevel>().unwrap()),
    )]
    pub log: LogLevel,

    /// Output results as JSON (useful for scripting and piping)
    #[arg(long, default_value_t = false)]
    pub json: bool,

    #[command(subcommand)]
    pub command: ClientCommandArgs,
}

#[derive(Subcommand, Debug, Clone)]
pub enum ClientCommandArgs {
    /// List all network nodes
    List,

    /// Resolve one daemon node by domain, full ID, or ID prefix
    Resolve {
        /// Domain, full node ID, or node ID prefix to resolve
        id_or_domain: String,
    },

    /// Get daemon status
    Status,

    /// Manage nodes
    Node {
        #[command(subcommand)]
        command: NodeCommand,
    },

    /// Get current P2P ticket from daemon
    Ticket,

    /// Pause daemon
    Pause,

    /// Resume daemon
    Resume,

    /// Gracefully shutdown daemon
    Stop,

    /// Manage class network
    Class {
        #[command(subcommand)]
        command: ClassCommand,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum ClassCommand {
    /// Create a new class network (requires --primary)
    Create {
        /// Human-readable class name (e.g. "CS101 Fall 2025")
        class_name: String,
    },
    /// Show current class network info
    Info,
    /// Manage class members
    Members {
        #[command(subcommand)]
        command: ClassMemberCommand,
    },
    /// Manage class groups
    Groups {
        #[command(subcommand)]
        command: ClassGroupCommand,
    },
    /// Manage invites
    Invite {
        #[command(subcommand)]
        command: ClassInviteCommand,
    },
    /// Join a class using an invite file
    Join {
        /// Path to invite JSON file
        invite_file: String,
    },
    /// Manage join requests
    JoinRequests {
        #[command(subcommand)]
        command: ClassJoinRequestsCommand,
    },
    /// Broadcast a message to all approved class members
    Broadcast {
        /// Message text
        #[arg(long)]
        message: String,
    },
    /// Send a multicast message to a group
    Multicast {
        /// Group ID
        #[arg(long)]
        group: String,
        /// Message text
        #[arg(long)]
        message: String,
    },
    /// Send a direct message to a class member
    Send {
        /// Target node ID
        #[arg(long)]
        node_id: String,
        /// Message text
        #[arg(long)]
        message: String,
    },
    /// Manage messages and audit
    Message {
        #[command(subcommand)]
        command: ClassMessageCommand,
    },
    /// Show audit log
    Audit {
        #[command(subcommand)]
        command: ClassAuditCommand,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum ClassMessageCommand {
    /// Show message delivery status
    Status {
        /// Message ID
        #[arg(long)]
        message_id: String,
    },
    /// Retry a failed message
    Retry {
        /// Message ID
        #[arg(long)]
        message_id: String,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum ClassAuditCommand {
    /// List all audit events
    List,
}

#[derive(Subcommand, Debug, Clone)]
pub enum ClassMemberCommand {
    /// List all class members
    List,
}

#[derive(Subcommand, Debug, Clone)]
pub enum ClassGroupCommand {
    /// List all groups
    List,
    /// Create a new group
    Create {
        /// Group name
        #[arg(long)]
        name: String,
    },
    /// Add a member to a group
    #[command(name = "add-member")]
    AddMember {
        /// Group ID
        #[arg(long)]
        group: String,
        /// Node ID of the member
        #[arg(long)]
        node_id: String,
    },
    /// Remove a member from a group
    #[command(name = "remove-member")]
    RemoveMember {
        /// Group ID
        #[arg(long)]
        group: String,
        /// Node ID of the member
        #[arg(long)]
        node_id: String,
    },
    /// List members of a group
    Members {
        /// Group ID
        #[arg(long)]
        group: String,
    },
    /// Delete a group
    Delete {
        /// Group ID
        #[arg(long)]
        group: String,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum ClassInviteCommand {
    /// Create a new invite for a student
    Create {
        /// Student display name (e.g. "Alice")
        #[arg(long)]
        display_name: String,
        /// Expiry in seconds (default 86400 = 24h)
        #[arg(long, default_value = "86400")]
        expires: u64,
        /// Output file path (prints to stdout if not set)
        #[arg(long)]
        output: Option<String>,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum ClassJoinRequestsCommand {
    /// List pending join requests
    List,
    /// Approve a join request
    Approve {
        /// Node ID of the student to approve
        node_id: String,
    },
    /// Deny a join request
    Deny {
        /// Node ID of the student to deny
        node_id: String,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum NodeCommand {
    /// Add a node using a ticket
    Add {
        /// The ticket string from the other node
        ticket: String,
    },

    /// Remove a node by ID (supports prefix matching)
    Remove {
        /// Full or prefix of the node ID to remove
        id: String,
    },
}

pub fn init_logging(level: LogLevel) {
    let default_filter = format!(
        "warn,p2p_ddns={},iroh::discovery::pkarr=error,tracing::span=warn",
        level
    );
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_filter));

    let _ = tracing_log::LogTracer::init();
    log::set_max_level(log::LevelFilter::Trace);
    let _ = tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_target(true)
        .with_level(true)
        .try_init();
}

pub async fn run_daemon(mut args: DaemonArgs) -> Result<()> {
    args.daemon = true;
    DaemonArgs::validate(&args)?;
    DaemonArgs::apply(&args);

    let db_path = paths::storage_db_path(&args);
    let storage = storage::open_or_create(db_path)?;
    let (ctx, gos_recv, sp_recv) = init_network(args.clone(), storage).await?;
    let ctx = Arc::new(ctx);
    let clients = Arc::new(ClientRegistry::new());

    output::print_bootstrap_ticket(&ctx);
    output::log_startup(&ctx);
    log::info!("Daemon started");

    if args.hosts_sync {
        sync_hosts_file(&ctx)?;
    }

    tokio::spawn({
        let ctx = ctx.clone();
        let clients = clients.clone();
        async move {
            crate::admin::server::run_default_management_server(ctx, clients).await;
        }
    });

    if let Some(bind) = args.admin_http.as_deref() {
        let bind = util::parse_bind_addr(bind)?;
        tokio::spawn({
            let ctx = ctx.clone();
            let clients = clients.clone();
            async move {
                if let Err(e) = crate::admin::http::run_http_server(bind, ctx, clients).await {
                    log::error!("Admin HTTP server stopped: {}", e);
                }
            }
        });
    }

    spawn_daemon_background_tasks(ctx.clone(), args.hosts_sync);
    spawn_outbox_retry_task(ctx.clone());

    ctx.run(gos_recv, sp_recv).await;
    Ok(())
}

fn spawn_outbox_retry_task(ctx: Arc<crate::net::Context>) {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(10));
        loop {
            tick.tick().await;
            let now = crate::util::time_now();
            let pending = match ctx.class_store.load_pending_outbox(now) {
                Ok(p) => p,
                Err(e) => {
                    log::error!("Outbox retry: failed to load pending: {e}");
                    continue;
                }
            };

            for mut record in pending {
                log::debug!(
                    "Outbox retry: message={} retry={}/{}",
                    record.message_id,
                    record.retry_count,
                    record.max_retries
                );

                // Phase 7: retry sends would go here.
                // Full per-recipient retry needs Phase 8 transport integration.

                record.retry_count += 1;
                record.next_retry_at = record.compute_next_retry(now);

                if record.retry_count >= record.max_retries {
                    record.status = crate::class_network::messaging::DeliveryStatus::Expired;
                    record.last_error = Some("Max retries exceeded".to_string());
                } else {
                    record.status = crate::class_network::messaging::DeliveryStatus::Sent;
                }

                if let Err(e) = ctx.class_store.save_outbox_record(&record) {
                    log::error!("Outbox retry: failed to save record: {e}");
                }
            }
        }
    });
}

pub async fn run_client(args: ClientArgs) -> Result<()> {
    let ticket = args.ticket.clone().or_else(|| get_ticket().ok());
    let auth = build_auth_request(ticket.as_deref()).await?;

    // Extract invite output path before consuming args.command
    let invite_output = match &args.command {
        ClientCommandArgs::Class {
            command:
                ClassCommand::Invite {
                    command: ClassInviteCommand::Create { output, .. },
                },
        } => output.clone(),
        _ => None,
    };

    let response = if let Some(bind) = args.admin_http.as_deref() {
        let bind = util::parse_bind_addr(bind)?;
        run_http_client(bind, args.timeout, auth, args.command).await?
    } else {
        let socket_path = get_socket_path(&args.socket_path);
        run_socket_client(socket_path, args.timeout, auth, args.command).await?
    };

    // Write invite to file if --output was specified
    if let ClientResponse::InviteCreated(ref json) = response
        && let Some(path) = &invite_output
    {
        if let Err(e) = std::fs::write(path, json) {
            output::display_error(&format!("Failed to write invite file: {e}"));
        } else {
            output::display_info(&format!("Invite written to {}", path));
        }
    }

    display_response(response, args.json)?;
    Ok(())
}

fn spawn_daemon_background_tasks(ctx: Arc<crate::net::Context>, hosts_sync: bool) {
    tokio::spawn(async move {
        let mut print_tick = tokio::time::interval(Duration::from_secs(30));
        let mut hosts_tick = tokio::time::interval(Duration::from_secs(2));

        loop {
            tokio::select! {
                _ = print_tick.tick() => {
                    output::print(&ctx);
                }
                _ = hosts_tick.tick(), if hosts_sync => {
                    if let Err(e) = sync_hosts_file(&ctx) {
                        log::error!("Failed to synchronize hosts file: {e:#}");
                    }
                }
            }
        }
    });
}

fn sync_hosts_file(ctx: &crate::net::Context) -> Result<()> {
    let changed = ctx.sync_hosts_file()?;
    let status = ctx.hosts_sync_status();
    let hosts_path = status.path.unwrap_or_else(|| "/etc/hosts".to_string());
    if changed {
        log::info!("Synchronized hosts records to {hosts_path}");
    } else {
        log::debug!("Hosts records already up to date: {hosts_path}");
    }
    Ok(())
}

fn get_ticket() -> Result<String> {
    if let Ok(ticket) = env::var("P2P_DDNS_CLI_ARGS") {
        let args: Vec<String> = ticket.split_whitespace().map(|s| s.to_string()).collect();
        for (i, arg) in args.iter().enumerate() {
            if (arg == "--ticket" || arg == "-t")
                && let Some(ticket) = args.get(i + 1)
            {
                return Ok(ticket.clone());
            }
        }
    }

    if let Ok(ticket) = env::var("P2P_DDNS_TICKET") {
        return Ok(ticket);
    }

    let ticket_path = get_client_config_path().join("ticket.txt");
    if ticket_path.exists() {
        let ticket = std::fs::read_to_string(&ticket_path)?;
        return Ok(ticket.trim().to_string());
    }

    Err(anyhow::anyhow!(
        "No ticket found. Provide via --ticket, P2P_DDNS_TICKET env, or save to {}/ticket.txt",
        get_client_config_path().display()
    ))
}

fn get_socket_path(override_path: &Option<PathBuf>) -> PathBuf {
    if let Some(path) = override_path {
        return path.clone();
    }

    if let Ok(path) = env::var("P2P_DDNS_SOCKET") {
        return PathBuf::from(path);
    }

    let mut candidates = crate::admin::server::socket_path_candidates();
    for legacy_path in [
        PathBuf::from("/run/p2p-ddns/p2p-ddns.sock"),
        PathBuf::from("/var/run/p2p-ddns/p2p-ddns.sock"),
        PathBuf::from("/var/run/p2p-ddns.sock"),
    ] {
        if !candidates.iter().any(|path| path == &legacy_path) {
            candidates.push(legacy_path);
        }
    }

    for path in &candidates {
        if path.exists() {
            return path.clone();
        }
    }

    candidates
        .into_iter()
        .next()
        .unwrap_or_else(crate::admin::server::default_socket_path)
}

#[cfg(unix)]
async fn connect_to_daemon(
    socket_path: &PathBuf,
    timeout_sec: u64,
) -> Result<tokio::net::UnixStream> {
    let timeout = tokio::time::Duration::from_secs(timeout_sec);
    Ok(
        tokio::time::timeout(timeout, tokio::net::UnixStream::connect(socket_path))
            .await
            .map_err(|_| {
                anyhow::anyhow!(
                    "Failed to connect to daemon at {} (timeout: {}s).\n\
                     Is the daemon running? Try: sudo systemctl start p2p-ddns",
                    socket_path.display(),
                    timeout_sec
                )
            })??,
    )
}

#[cfg(not(unix))]
#[allow(dead_code)]
async fn connect_to_daemon(_socket_path: &PathBuf, _timeout_sec: u64) -> Result<()> {
    anyhow::bail!("Local socket admin is unavailable on this platform; use --admin-http instead")
}

async fn build_auth_request(ticket: Option<&str>) -> Result<AuthRequest> {
    let storage = init_client_storage().await?;
    let (pk, _sk) = storage.load_secret()?.unwrap_or_else(|| {
        let mut rng = rand::rng();
        let sk = iroh::SecretKey::generate(&mut rng);
        let pk = sk.public();
        storage.save_secret(sk.clone()).ok();
        (pk, sk)
    });

    Ok(AuthRequest {
        ticket: ticket.unwrap_or_default().to_string(),
        client_public_key: Some(
            base64::engine::general_purpose::STANDARD_NO_PAD.encode(pk.as_bytes()),
        ),
        client_name: Some("cli".to_string()),
    })
}

#[cfg(unix)]
async fn run_socket_client(
    socket_path: PathBuf,
    timeout_sec: u64,
    auth: AuthRequest,
    command: ClientCommandArgs,
) -> Result<ClientResponse> {
    let mut stream = connect_to_daemon(&socket_path, timeout_sec).await?;
    authenticate_socket(&mut stream, &auth).await?;
    execute_socket_command(&mut stream, command).await
}

#[cfg(not(unix))]
async fn run_socket_client(
    _socket_path: PathBuf,
    _timeout_sec: u64,
    _auth: AuthRequest,
    _command: ClientCommandArgs,
) -> Result<ClientResponse> {
    anyhow::bail!("Local socket admin is unavailable on this platform; use --admin-http instead")
}

#[cfg(unix)]
async fn authenticate_socket(
    stream: &mut tokio::net::UnixStream,
    auth: &AuthRequest,
) -> Result<()> {
    send_message(stream, auth).await?;

    let auth_resp: AuthResponse = read_message(stream).await?;
    if !auth_resp.success {
        Err(anyhow::anyhow!(
            "Authentication failed: {}",
            auth_resp
                .error
                .unwrap_or_else(|| "Unknown error".to_string())
        ))
    } else {
        Ok(())
    }
}

#[cfg(unix)]
async fn execute_socket_command(
    stream: &mut tokio::net::UnixStream,
    command: ClientCommandArgs,
) -> Result<ClientResponse> {
    let cmd = to_client_command(command);
    send_message(stream, &cmd).await?;
    read_message(stream).await
}

async fn run_http_client(
    bind: SocketAddr,
    timeout_sec: u64,
    auth: AuthRequest,
    command: ClientCommandArgs,
) -> Result<ClientResponse> {
    authenticate_http(bind, timeout_sec, &auth).await?;
    let request = AdminCommandRequest {
        auth,
        command: to_client_command(command),
    };
    let (status, resp_body) = send_http_post(bind, timeout_sec, "/command", &request).await?;
    match status {
        200 | 403 => Ok(postcard::from_bytes(&resp_body)?),
        401 => {
            let auth_resp: AuthResponse = postcard::from_bytes(&resp_body)?;
            Err(anyhow::anyhow!(
                "Authentication failed: {}",
                auth_resp
                    .error
                    .unwrap_or_else(|| "Unknown error".to_string())
            ))
        }
        _ => Err(anyhow::anyhow!(
            "HTTP command failed with status {}",
            status
        )),
    }
}

async fn authenticate_http(bind: SocketAddr, timeout_sec: u64, auth: &AuthRequest) -> Result<()> {
    let (status, resp_body) = send_http_post(bind, timeout_sec, "/auth", auth).await?;
    if status != 200 {
        return Err(anyhow::anyhow!("HTTP auth failed with status {}", status));
    }

    let auth_resp: AuthResponse = postcard::from_bytes(&resp_body)?;
    if !auth_resp.success {
        Err(anyhow::anyhow!(
            "Authentication failed: {}",
            auth_resp
                .error
                .unwrap_or_else(|| "Unknown error".to_string())
        ))
    } else {
        Ok(())
    }
}

async fn send_http_post<T: serde::Serialize>(
    bind: SocketAddr,
    timeout_sec: u64,
    path: &str,
    body: &T,
) -> Result<(u16, Vec<u8>)> {
    let timeout = tokio::time::Duration::from_secs(timeout_sec);
    let request_body = postcard::to_stdvec(body)?;
    let mut stream = tokio::time::timeout(timeout, tokio::net::TcpStream::connect(bind))
        .await
        .map_err(|_| anyhow::anyhow!("Failed to connect to HTTP admin endpoint at {}", bind))??;

    let request = format!(
        "POST {path} HTTP/1.1\r\nHost: {bind}\r\nContent-Type: application/postcard\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        request_body.len()
    );
    stream.write_all(request.as_bytes()).await?;
    stream.write_all(&request_body).await?;

    let mut resp = Vec::new();
    stream.read_to_end(&mut resp).await?;
    parse_http_response(&resp)
}

fn parse_http_response(resp: &[u8]) -> Result<(u16, Vec<u8>)> {
    let header_end = resp
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or_else(|| anyhow::anyhow!("missing response header terminator"))?;
    let header = std::str::from_utf8(&resp[..header_end])?;
    let status = header
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .ok_or_else(|| anyhow::anyhow!("bad status line"))?
        .parse::<u16>()?;
    Ok((status, resp[header_end + 4..].to_vec()))
}

fn to_client_command(command: ClientCommandArgs) -> ClientCommand {
    match command {
        ClientCommandArgs::List => ClientCommand::Query,
        ClientCommandArgs::Resolve { id_or_domain } => ClientCommand::ResolveNode { id_or_domain },
        ClientCommandArgs::Status => ClientCommand::Status,
        ClientCommandArgs::Node {
            command: NodeCommand::Add { ticket },
        } => ClientCommand::AddNode { ticket },
        ClientCommandArgs::Node {
            command: NodeCommand::Remove { id },
        } => ClientCommand::RemoveNode { id },
        ClientCommandArgs::Ticket => ClientCommand::GetTicket,
        ClientCommandArgs::Pause => ClientCommand::Pause,
        ClientCommandArgs::Resume => ClientCommand::Resume,
        ClientCommandArgs::Stop => ClientCommand::Shutdown,
        ClientCommandArgs::Class {
            command: ClassCommand::Create { class_name },
        } => ClientCommand::ClassCreate { class_name },
        ClientCommandArgs::Class {
            command: ClassCommand::Info,
        } => ClientCommand::ClassInfo,
        ClientCommandArgs::Class {
            command:
                ClassCommand::Members {
                    command: ClassMemberCommand::List,
                },
        } => ClientCommand::ClassMembersList,
        ClientCommandArgs::Class {
            command:
                ClassCommand::Groups {
                    command: ClassGroupCommand::List,
                },
        } => ClientCommand::ClassGroupsList,
        ClientCommandArgs::Class {
            command:
                ClassCommand::Groups {
                    command: ClassGroupCommand::Create { name },
                },
        } => ClientCommand::ClassGroupCreate { group_name: name },
        ClientCommandArgs::Class {
            command:
                ClassCommand::Groups {
                    command: ClassGroupCommand::AddMember { group, node_id },
                },
        } => ClientCommand::ClassGroupAddMember {
            group_id: group,
            node_id,
        },
        ClientCommandArgs::Class {
            command:
                ClassCommand::Groups {
                    command: ClassGroupCommand::RemoveMember { group, node_id },
                },
        } => ClientCommand::ClassGroupRemoveMember {
            group_id: group,
            node_id,
        },
        ClientCommandArgs::Class {
            command:
                ClassCommand::Groups {
                    command: ClassGroupCommand::Members { group },
                },
        } => ClientCommand::ClassGroupMembersList { group_id: group },
        ClientCommandArgs::Class {
            command:
                ClassCommand::Groups {
                    command: ClassGroupCommand::Delete { group },
                },
        } => ClientCommand::ClassGroupDelete { group_id: group },
        ClientCommandArgs::Class {
            command: ClassCommand::Broadcast { message },
        } => ClientCommand::ClassBroadcast { message },
        ClientCommandArgs::Class {
            command: ClassCommand::Multicast { group, message },
        } => ClientCommand::ClassMulticast {
            group_id: group,
            message,
        },
        ClientCommandArgs::Class {
            command: ClassCommand::Send { node_id, message },
        } => ClientCommand::ClassDirectSend { node_id, message },
        ClientCommandArgs::Class {
            command:
                ClassCommand::Message {
                    command: ClassMessageCommand::Status { message_id },
                },
        } => ClientCommand::ClassMessageStatus { message_id },
        ClientCommandArgs::Class {
            command:
                ClassCommand::Message {
                    command: ClassMessageCommand::Retry { message_id },
                },
        } => ClientCommand::ClassMessageRetry { message_id },
        ClientCommandArgs::Class {
            command:
                ClassCommand::Audit {
                    command: ClassAuditCommand::List,
                },
        } => ClientCommand::ClassAuditList,
        ClientCommandArgs::Class {
            command:
                ClassCommand::Invite {
                    command:
                        ClassInviteCommand::Create {
                            display_name,
                            expires,
                            output: _,
                        },
                },
        } => ClientCommand::ClassInviteCreate {
            display_name,
            expires_secs: Some(expires),
        },
        ClientCommandArgs::Class {
            command: ClassCommand::Join { invite_file },
        } => {
            let json =
                std::fs::read_to_string(&invite_file).unwrap_or_else(|_| invite_file.clone());
            ClientCommand::ClassJoin { invite_json: json }
        }
        ClientCommandArgs::Class {
            command:
                ClassCommand::JoinRequests {
                    command: ClassJoinRequestsCommand::List,
                },
        } => ClientCommand::ClassJoinRequestsList,
        ClientCommandArgs::Class {
            command:
                ClassCommand::JoinRequests {
                    command: ClassJoinRequestsCommand::Approve { node_id },
                },
        } => ClientCommand::ClassJoinRequestApprove { node_id },
        ClientCommandArgs::Class {
            command:
                ClassCommand::JoinRequests {
                    command: ClassJoinRequestsCommand::Deny { node_id },
                },
        } => ClientCommand::ClassJoinRequestDeny { node_id },
    }
}

fn display_response(response: ClientResponse, json: bool) -> Result<()> {
    match response {
        ClientResponse::Nodes(nodes) => {
            if json {
                output::display_nodes_json(&nodes);
            } else {
                output::display_nodes(&nodes);
            }
        }
        ClientResponse::Node(node) => {
            if json {
                output::display_node_json(node.as_ref());
            } else {
                output::display_node(node.as_ref());
            }
        }
        ClientResponse::Status(status) => {
            if json {
                output::display_status_json(&status);
            } else {
                output::display_status(&status);
            }
        }
        ClientResponse::Ticket(ticket) => {
            if json {
                output::display_ticket_json(&ticket);
            } else {
                output::display_ticket(&ticket);
            }
            let ticket_path = get_client_config_path().join("ticket.txt");
            if std::fs::write(&ticket_path, &ticket).is_ok() {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    if let Ok(meta) = std::fs::metadata(&ticket_path) {
                        let mut perms = meta.permissions();
                        perms.set_mode(0o600);
                        let _ = std::fs::set_permissions(&ticket_path, perms);
                    }
                }
                if !json {
                    output::display_info(&format!("Ticket saved to {}", ticket_path.display()));
                }
            }
        }
        ClientResponse::Ack(message) => {
            if json {
                output::display_ack_json(&message);
            } else {
                output::display_ack(&message);
            }
        }
        ClientResponse::Error(e) => {
            if json {
                output::display_error_json(&e);
            } else {
                output::display_error(&e);
            }
            std::process::exit(1);
        }
        ClientResponse::ClassCreated(network) => {
            if json {
                output::display_class_created_json(&network);
            } else {
                output::display_class_created(&network);
            }
        }
        ClientResponse::ClassInfo(network) => {
            if json {
                output::display_class_info_json(&network);
            } else {
                output::display_class_info(&network);
            }
        }
        ClientResponse::ClassMembers(members) => {
            if json {
                output::display_class_members_json(&members);
            } else {
                output::display_class_members(&members);
            }
        }
        ClientResponse::ClassGroups(groups) => {
            if json {
                output::display_class_groups_json(&groups);
            } else {
                output::display_class_groups(&groups);
            }
        }
        ClientResponse::InviteCreated(invite_json) => {
            if json {
                output::display_invite_created_json(&invite_json);
            } else {
                output::display_invite_created(&invite_json);
            }
        }
        ClientResponse::JoinRequested(msg) => {
            if json {
                output::display_ack_json(&msg);
            } else {
                output::display_ack(&msg);
            }
        }
        ClientResponse::JoinRequests(members) => {
            if json {
                output::display_join_requests_json(&members);
            } else {
                output::display_join_requests(&members);
            }
        }
        ClientResponse::JoinRequestApproved(msg) => {
            if json {
                output::display_ack_json(&msg);
            } else {
                output::display_ack(&msg);
            }
        }
        ClientResponse::JoinRequestDenied(msg) => {
            if json {
                output::display_ack_json(&msg);
            } else {
                output::display_ack(&msg);
            }
        }
        ClientResponse::GroupCreated(group) => {
            if json {
                output::display_group_created_json(&group);
            } else {
                output::display_group_created(&group);
            }
        }
        ClientResponse::GroupMembers(members) => {
            if json {
                output::display_group_members_json(&members);
            } else {
                output::display_group_members(&members);
            }
        }
        ClientResponse::MessageStatus(msg) | ClientResponse::AuditLog(msg) => {
            if json {
                println!("{msg}");
            } else {
                output::display_info(&msg);
            }
        }
        ClientResponse::BroadcastResult {
            message_id,
            total,
            sent,
            failed,
            failures,
        } => {
            if json {
                output::display_broadcast_result_json(&message_id, total, sent, failed, &failures);
            } else {
                output::display_broadcast_result(&message_id, total, sent, failed, &failures);
            }
        }
    }

    Ok(())
}

async fn init_client_storage() -> Result<crate::storage::Storage> {
    let config_path = get_client_config_path();
    std::fs::create_dir_all(&config_path)?;

    let db_path = config_path.join("client.db");
    crate::storage::Storage::new(db_path)
}

fn get_client_config_path() -> PathBuf {
    dirs::config_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("p2p-ddns")
}

#[cfg(unix)]
async fn send_message<T: serde::Serialize>(
    stream: &mut tokio::net::UnixStream,
    msg: &T,
) -> Result<()> {
    let msg_bytes = postcard::to_stdvec(msg)?;
    let len = msg_bytes.len() as u32;
    stream.write_all(&len.to_be_bytes()).await?;
    stream.write_all(&msg_bytes).await?;
    Ok(())
}

#[cfg(unix)]
async fn read_message<T: for<'a> serde::Deserialize<'a>>(
    stream: &mut tokio::net::UnixStream,
) -> Result<T> {
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > 4 * 1024 * 1024 {
        anyhow::bail!("admin frame too large: {}", len);
    }
    let mut msg_buf = vec![0u8; len];
    stream.read_exact(&mut msg_buf).await?;
    Ok(postcard::from_bytes(&msg_buf)?)
}
