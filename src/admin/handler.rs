use std::collections::BTreeMap;

use anyhow::Result;
use base64::Engine as _;
use iroh::{EndpointAddr, EndpointId, PublicKey};

use crate::{
    admin::{authz::ClientRegistry, protocol::*},
    class_network::model::{
        ClassMember, ClassNetwork, Group, JoinTicket, MemberStatus, PendingInvite,
    },
    domain::{
        client::{ClientInfo, ClientPermissions, SERVICE_MARKER_CLIENT, SERVICE_VALUE_CLIENT},
        node::{Node, NodeRole},
        ticket::Ticket,
    },
    net::Context,
    util,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdminAction {
    Shutdown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthMode {
    RequireTicket,
    AllowLocalTicketless,
}

#[derive(Debug)]
pub struct CommandOutcome {
    pub response: ClientResponse,
    pub action: Option<AdminAction>,
}

pub fn authenticate_and_register(
    ctx: &Context,
    clients: &ClientRegistry,
    auth_req: &AuthRequest,
    mode: AuthMode,
) -> Result<(AuthResponse, Option<EndpointId>)> {
    let ticket_valid = match auth_req.ticket.parse::<Ticket>() {
        Ok(ticket) => ticket.validate(ctx.ticket.topic(), ctx.ticket.rnum()),
        Err(_) => false,
    };
    let auth_allowed = ticket_valid || mode == AuthMode::AllowLocalTicketless;

    if !auth_allowed {
        return Ok((
            AuthResponse {
                success: false,
                error: Some("Invalid ticket".to_string()),
                daemon_public_key: None,
            },
            None,
        ));
    }

    let client_pk = match auth_req.client_public_key.as_deref() {
        Some(pk_str) => match base64::engine::general_purpose::STANDARD_NO_PAD.decode(pk_str) {
            Ok(bytes) => match <[u8; 32]>::try_from(bytes.as_slice()) {
                Ok(bytes) => PublicKey::from_bytes(&bytes).ok(),
                Err(_) => None,
            },
            Err(_) => None,
        },
        None => None,
    };

    if let Some(pk) = client_pk {
        if clients.is_client_node(&pk) {
            return Ok((
                AuthResponse {
                    success: true,
                    error: None,
                    daemon_public_key: Some(
                        base64::engine::general_purpose::STANDARD_NO_PAD
                            .encode(ctx.me.node_id.as_bytes()),
                    ),
                },
                Some(pk as EndpointId),
            ));
        }

        let client_node = Node {
            node_id: pk,
            invitor: ctx.me.node_id,
            addr: EndpointAddr::new(pk),
            domain: auth_req
                .client_name
                .clone()
                .unwrap_or_else(|| format!("client-{}", pk)),
            services: {
                let mut map = BTreeMap::new();
                map.insert(SERVICE_MARKER_CLIENT.to_string(), SERVICE_VALUE_CLIENT);
                map
            },
            last_heartbeat: util::time_now(),
            role: None,
        };

        ctx.nodes.insert(pk, client_node);

        let client_info = ClientInfo {
            connected_at: util::time_now(),
            ticket_used: auth_req.ticket.clone(),
            client_name: auth_req.client_name.clone(),
            permissions: ClientPermissions {
                can_query: true,
                can_add_node: true,
                can_remove_node: true,
                can_control: true,
            },
        };
        clients.add_client(pk, client_info);
    }

    Ok((
        AuthResponse {
            success: true,
            error: None,
            daemon_public_key: Some(
                base64::engine::general_purpose::STANDARD_NO_PAD.encode(ctx.me.node_id.as_bytes()),
            ),
        },
        client_pk.map(|pk| pk as EndpointId),
    ))
}

pub fn required_permission(cmd: &ClientCommand) -> &'static str {
    match cmd {
        ClientCommand::Query => "query",
        ClientCommand::ResolveNode { .. } => "query",
        ClientCommand::AddNode { .. } => "add_node",
        ClientCommand::RemoveNode { .. } => "remove_node",
        ClientCommand::SetService { .. } => "control",
        ClientCommand::Status => "query",
        ClientCommand::GetTicket => "query",
        ClientCommand::Pause => "control",
        ClientCommand::Resume => "control",
        ClientCommand::Shutdown => "control",
        ClientCommand::ClassCreate { .. } => "control",
        ClientCommand::ClassInfo => "control",
        ClientCommand::ClassMembersList => "control",
        ClientCommand::ClassGroupsList => "control",
        ClientCommand::ClassInviteCreate { .. } => "control",
        ClientCommand::ClassJoin { .. } => "control",
        ClientCommand::ClassJoinRequestsList => "query",
        ClientCommand::ClassJoinRequestApprove { .. } => "control",
        ClientCommand::ClassJoinRequestDeny { .. } => "control",
        ClientCommand::ClassGroupCreate { .. } => "control",
        ClientCommand::ClassGroupAddMember { .. } => "control",
        ClientCommand::ClassGroupRemoveMember { .. } => "control",
        ClientCommand::ClassGroupMembersList { .. } => "query",
        ClientCommand::ClassGroupDelete { .. } => "control",
        ClientCommand::ClassBroadcast { .. } => "control",
        ClientCommand::ClassMulticast { .. } => "control",
        ClientCommand::ClassDirectSend { .. } => "control",
        ClientCommand::ClassMessageStatus { .. } => "query",
        ClientCommand::ClassMessageRetry { .. } => "control",
        ClientCommand::ClassAuditList => "query",
    }
}

fn resolve_active_daemon_node(
    id_or_domain: &str,
    ctx: &Context,
    clients: &ClientRegistry,
) -> std::result::Result<Option<Node>, String> {
    let query = id_or_domain.trim();
    if query.is_empty() {
        return Err("Node id or domain is required".to_string());
    }

    let nodes = ctx
        .nodes
        .iter()
        .filter(|e| !clients.is_client_node(e.key()))
        .map(|e| e.value().clone())
        .collect::<Vec<_>>();

    if let Ok(node_id) = query.parse::<EndpointId>() {
        return Ok(nodes.into_iter().find(|node| node.node_id == node_id));
    }

    let domain_matches = nodes
        .iter()
        .filter(|node| node.domain.eq_ignore_ascii_case(query))
        .cloned()
        .collect::<Vec<_>>();
    match domain_matches.len() {
        0 => {}
        1 => return Ok(domain_matches.into_iter().next()),
        n => {
            return Err(format!(
                "Ambiguous domain '{}': matches {} nodes. Use node id instead.",
                query, n
            ));
        }
    }

    let prefix = query.to_lowercase();
    let prefix_matches = nodes
        .into_iter()
        .filter(|node| node.node_id.to_string().to_lowercase().starts_with(&prefix))
        .collect::<Vec<_>>();
    match prefix_matches.len() {
        0 => Ok(None),
        1 => Ok(prefix_matches.into_iter().next()),
        n => Err(format!(
            "Ambiguous prefix '{}': matches {} nodes. Provide more characters.",
            query, n
        )),
    }
}

fn uuid_v4() -> String {
    let bytes: [u8; 16] = rand::random();
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0],
        bytes[1],
        bytes[2],
        bytes[3],
        bytes[4],
        bytes[5],
        bytes[6],
        bytes[7],
        bytes[8],
        bytes[9],
        bytes[10],
        bytes[11],
        bytes[12],
        bytes[13],
        bytes[14],
        bytes[15],
    )
}

fn require_teacher(
    ctx: &Context,
    client_id: Option<EndpointId>,
) -> std::result::Result<(), String> {
    let id = client_id.ok_or("Authentication required")?;
    let member = ctx
        .class_store
        .load_member(&id)
        .map_err(|e| format!("{e}"))?
        .ok_or("Not a class member")?;
    if member.role != NodeRole::Teacher {
        return Err("Only teachers can perform this action".into());
    }
    Ok(())
}

pub async fn handle_command(
    cmd: &ClientCommand,
    ctx: &Context,
    clients: &ClientRegistry,
    client_id: Option<EndpointId>,
) -> CommandOutcome {
    match cmd {
        ClientCommand::Query => {
            let nodes = ctx
                .nodes
                .iter()
                .filter(|e| !clients.is_client_node(e.key()))
                .map(|e| e.value().clone())
                .collect();
            CommandOutcome {
                response: ClientResponse::Nodes(nodes),
                action: None,
            }
        }
        ClientCommand::ResolveNode { id_or_domain } => {
            let response = match resolve_active_daemon_node(id_or_domain, ctx, clients) {
                Ok(node) => ClientResponse::Node(node),
                Err(e) => ClientResponse::Error(e),
            };
            CommandOutcome {
                response,
                action: None,
            }
        }
        ClientCommand::AddNode { ticket } => {
            let parsed: Ticket = match ticket.parse() {
                Ok(t) => t,
                Err(e) => {
                    return CommandOutcome {
                        response: ClientResponse::Error(format!("Invalid ticket: {e}")),
                        action: None,
                    };
                }
            };

            let (topic, _rnum, node) = parsed.flatten();
            if topic != ctx.ticket.topic() {
                return CommandOutcome {
                    response: ClientResponse::Error(
                        "Ticket topic does not match this daemon's network".to_string(),
                    ),
                    action: None,
                };
            }

            let domain = node.domain.clone();
            ctx.trust_daemon_node(&node);
            ctx.upsert_active_node(node.clone());
            if let Err(e) = ctx.storage.save_node(&node) {
                return CommandOutcome {
                    response: ClientResponse::Error(format!("Failed to persist node: {e}")),
                    action: None,
                };
            }

            CommandOutcome {
                response: ClientResponse::Ack(format!("Node added: {}", domain)),
                action: None,
            }
        }
        ClientCommand::RemoveNode { id } => {
            let node_id: EndpointId = match id.parse() {
                Ok(id) => id,
                Err(_) => {
                    // Prefix matching: find nodes whose ID string starts with the given prefix
                    let prefix = id.to_lowercase();
                    let mut matches: Vec<EndpointId> = ctx
                        .nodes
                        .iter()
                        .filter(|e| !clients.is_client_node(e.key()))
                        .filter(|e| e.key().to_string().to_lowercase().starts_with(&prefix))
                        .map(|e| *e.key())
                        .collect();
                    for trusted_id in ctx.trusted_node_ids_with_prefix(&prefix) {
                        if !matches.contains(&trusted_id) {
                            matches.push(trusted_id);
                        }
                    }

                    match matches.len() {
                        0 => {
                            return CommandOutcome {
                                response: ClientResponse::Error(format!(
                                    "No node found matching prefix '{}'",
                                    id
                                )),
                                action: None,
                            };
                        }
                        1 => matches[0],
                        n => {
                            return CommandOutcome {
                                response: ClientResponse::Error(format!(
                                    "Ambiguous prefix '{}': matches {} nodes. Provide more characters.",
                                    id, n
                                )),
                                action: None,
                            };
                        }
                    }
                }
            };

            if node_id == ctx.me.node_id {
                return CommandOutcome {
                    response: ClientResponse::Error("Refusing to remove self".to_string()),
                    action: None,
                };
            }

            let domain = ctx
                .nodes
                .get(&node_id)
                .map(|n| n.value().domain.clone())
                .or_else(|| ctx.trusted_node_domain(&node_id));
            clients.remove_client(&node_id);
            ctx.nodes.remove(&node_id);
            ctx.untrust_node(&node_id);
            if let Err(e) = ctx.storage.remove_node(&node_id) {
                return CommandOutcome {
                    response: ClientResponse::Error(format!("Failed to remove from storage: {e}")),
                    action: None,
                };
            }

            let msg = match domain {
                Some(d) => format!("Node removed: {} ({})", d, node_id),
                None => format!("Node removed: {}", node_id),
            };
            CommandOutcome {
                response: ClientResponse::Ack(msg),
                action: None,
            }
        }
        ClientCommand::SetService { name, port } => {
            ctx.local_services.write().insert(name.clone(), *port);
            ctx.broadcast_state_update().await;
            ctx.send_state_update_to_known_nodes().await;
            ctx.request_neighbor_sync().await;
            CommandOutcome {
                response: ClientResponse::Ack(format!(
                    "Service '{}' registered on port {}",
                    name, port
                )),
                action: None,
            }
        }
        ClientCommand::Status => {
            let my_addr = if crate::net::should_filter_advertised_addrs(&ctx.args) {
                util::best_local_ip_for_display(&ctx.handle.addr())
            } else {
                util::best_ip_for_display(&ctx.handle.addr())
            }
            .map(|ip| ip.to_string())
            .unwrap_or_else(|| format!("{:?}", ctx.handle.addr()));
            let status = crate::domain::client::DaemonStatus {
                running: true,
                paused: ctx.is_paused(),
                node_count: ctx.nodes.len(),
                client_count: clients.count(),
                uptime_seconds: ctx.uptime_seconds(),
                my_domain: ctx.me.domain.clone(),
                my_addr,
                hosts_sync: ctx.hosts_sync_status(),
            };
            CommandOutcome {
                response: ClientResponse::Status(status),
                action: None,
            }
        }
        ClientCommand::GetTicket => CommandOutcome {
            response: ClientResponse::Ticket(ctx.ticket.to_string()),
            action: None,
        },
        ClientCommand::Pause => {
            ctx.set_paused(true);
            CommandOutcome {
                response: ClientResponse::Ack("Daemon paused".to_string()),
                action: None,
            }
        }
        ClientCommand::Resume => {
            ctx.set_paused(false);
            CommandOutcome {
                response: ClientResponse::Ack("Daemon resumed".to_string()),
                action: None,
            }
        }
        ClientCommand::Shutdown => CommandOutcome {
            response: ClientResponse::Ack("Daemon shutting down".to_string()),
            action: Some(AdminAction::Shutdown),
        },
        ClientCommand::ClassCreate { class_name } => {
            if let Err(e) = require_teacher(ctx, client_id) {
                return CommandOutcome {
                    response: ClientResponse::Error(e),
                    action: None,
                };
            }
            if !ctx.args.primary {
                return CommandOutcome {
                    response: ClientResponse::Error(
                        "Only the primary node can create a class network. Use --primary flag."
                            .to_string(),
                    ),
                    action: None,
                };
            }

            if ctx
                .class_store
                .load_class_network()
                .unwrap_or(None)
                .is_some()
            {
                return CommandOutcome {
                    response: ClientResponse::Error(
                        "A class network already exists. Delete it first or use a fresh config."
                            .to_string(),
                    ),
                    action: None,
                };
            }

            let now = util::time_now();
            let network = ClassNetwork {
                class_id: ctx.ticket.topic(),
                class_name: class_name.clone(),
                teacher_node_id: ctx.me.node_id,
                teacher_domain: ctx.me.domain.clone(),
                created_at: now,
            };

            let teacher_member = ClassMember {
                node_id: ctx.me.node_id,
                display_name: ctx.me.domain.clone(),
                role: NodeRole::Teacher,
                status: MemberStatus::Approved,
                groups: vec![],
                joined_at: now,
            };

            if let Err(e) = ctx.class_store.save_class_network(&network) {
                return CommandOutcome {
                    response: ClientResponse::Error(format!("Failed to save class network: {e}")),
                    action: None,
                };
            }

            if let Err(e) = ctx.class_store.save_member(&teacher_member) {
                return CommandOutcome {
                    response: ClientResponse::Error(format!("Failed to save teacher member: {e}")),
                    action: None,
                };
            }

            ctx.class_store.audit(
                &ctx.me.domain,
                "class.create",
                &network.class_name,
                "ok",
            );

            CommandOutcome {
                response: ClientResponse::ClassCreated(network),
                action: None,
            }
        }
        ClientCommand::ClassInfo => match ctx.class_store.load_class_network() {
            Ok(Some(network)) => CommandOutcome {
                response: ClientResponse::ClassInfo(network),
                action: None,
            },
            Ok(None) => CommandOutcome {
                response: ClientResponse::Error("No class network found".to_string()),
                action: None,
            },
            Err(e) => CommandOutcome {
                response: ClientResponse::Error(format!("Failed to load class network: {e}")),
                action: None,
            },
        },
        ClientCommand::ClassMembersList => match ctx.class_store.load_all_members() {
            Ok(members) => CommandOutcome {
                response: ClientResponse::ClassMembers(members),
                action: None,
            },
            Err(e) => CommandOutcome {
                response: ClientResponse::Error(format!("Failed to load members: {e}")),
                action: None,
            },
        },
        ClientCommand::ClassGroupsList => match ctx.class_store.load_all_groups() {
            Ok(groups) => CommandOutcome {
                response: ClientResponse::ClassGroups(groups),
                action: None,
            },
            Err(e) => CommandOutcome {
                response: ClientResponse::Error(format!("Failed to load groups: {e}")),
                action: None,
            },
        },
        ClientCommand::ClassInviteCreate {
            display_name,
            expires_secs,
        } => {
            if let Err(e) = require_teacher(ctx, client_id) {
                return CommandOutcome {
                    response: ClientResponse::Error(e),
                    action: None,
                };
            }
            let network = match ctx.class_store.load_class_network() {
                Ok(Some(n)) => n,
                Ok(None) => {
                    return CommandOutcome {
                        response: ClientResponse::Error(
                            "No class network found. Create one first with 'class create'."
                                .to_string(),
                        ),
                        action: None,
                    };
                }
                Err(e) => {
                    return CommandOutcome {
                        response: ClientResponse::Error(format!(
                            "Failed to load class network: {e}"
                        )),
                        action: None,
                    };
                }
            };

            let now = util::time_now();
            let expires = expires_secs.unwrap_or(86400);
            let invite_id = uuid_v4();

            // Generate per-member shared secret: 32 random bytes → base64
            let secret_bytes: [u8; 32] = rand::random();
            let shared_secret =
                base64::engine::general_purpose::STANDARD_NO_PAD.encode(secret_bytes);

            let join_ticket = JoinTicket {
                invite_id: invite_id.clone(),
                class_id: network.class_id,
                class_name: network.class_name.clone(),
                teacher_node_id: network.teacher_node_id,
                teacher_domain: network.teacher_domain.clone(),
                p2p_ticket: ctx.ticket.to_string(),
                transport_port: 39091,
                shared_secret: shared_secret.clone(),
                expires_at: now + expires,
            };

            let json = match serde_json::to_string_pretty(&join_ticket) {
                Ok(j) => j,
                Err(e) => {
                    return CommandOutcome {
                        response: ClientResponse::Error(format!("Failed to serialize invite: {e}")),
                        action: None,
                    };
                }
            };

            let pending = PendingInvite {
                invite_id,
                display_name: display_name.clone(),
                shared_secret,
                created_at: now,
                expires_at: now + expires,
            };

            if let Err(e) = ctx.class_store.save_pending_invite(&pending) {
                return CommandOutcome {
                    response: ClientResponse::Error(format!("Failed to save invite: {e}")),
                    action: None,
                };
            }

            CommandOutcome {
                response: ClientResponse::InviteCreated(json),
                action: None,
            }
        }
        ClientCommand::ClassJoin { invite_json } => {
            let join_ticket: JoinTicket = match serde_json::from_str(invite_json) {
                Ok(jt) => jt,
                Err(e) => {
                    return CommandOutcome {
                        response: ClientResponse::Error(format!("Invalid invite JSON: {e}")),
                        action: None,
                    };
                }
            };

            // Validate invite exists and is not expired
            let now = util::time_now();
            let pending = match ctx.class_store.load_pending_invite(&join_ticket.invite_id) {
                Ok(Some(p)) => {
                    if now > p.expires_at {
                        return CommandOutcome {
                            response: ClientResponse::Error("Invite has expired".to_string()),
                            action: None,
                        };
                    }
                    p
                }
                Ok(None) => {
                    return CommandOutcome {
                        response: ClientResponse::Error("Unknown invite ID".to_string()),
                        action: None,
                    };
                }
                Err(e) => {
                    return CommandOutcome {
                        response: ClientResponse::Error(format!("Failed to load invite: {e}")),
                        action: None,
                    };
                }
            };

            // Parse P2P ticket and add node to network
            let ticket: Ticket = match join_ticket.p2p_ticket.parse() {
                Ok(t) => t,
                Err(e) => {
                    return CommandOutcome {
                        response: ClientResponse::Error(format!("Invalid P2P ticket: {e}")),
                        action: None,
                    };
                }
            };

            let (topic, _rnum, node) = ticket.flatten();
            if topic != ctx.ticket.topic() {
                return CommandOutcome {
                    response: ClientResponse::Error(
                        "P2P ticket topic does not match this network".to_string(),
                    ),
                    action: None,
                };
            }

            let student_node_id = node.node_id;
            ctx.trust_daemon_node(&node);
            ctx.upsert_active_node(node.clone());
            if let Err(e) = ctx.storage.save_node(&node) {
                return CommandOutcome {
                    response: ClientResponse::Error(format!("Failed to persist node: {e}")),
                    action: None,
                };
            }

            // Store shared_secret in config
            if let Err(e) = ctx.storage.save_config::<Vec<u8>, Vec<u8>>(
                &format!("shared_secret_{}", student_node_id),
                pending.shared_secret.as_bytes().to_vec(),
            ) {
                return CommandOutcome {
                    response: ClientResponse::Error(format!("Failed to save secret: {e}")),
                    action: None,
                };
            }

            // Create student ClassMember (status=Pending)
            let class_member = ClassMember {
                node_id: student_node_id,
                display_name: pending.display_name.clone(),
                role: NodeRole::Student,
                status: MemberStatus::Pending,
                groups: vec![],
                joined_at: now,
            };

            if let Err(e) = ctx.class_store.save_member(&class_member) {
                return CommandOutcome {
                    response: ClientResponse::Error(format!("Failed to save member: {e}")),
                    action: None,
                };
            }

            // Delete consumed invite
            let _ = ctx
                .class_store
                .delete_pending_invite(&join_ticket.invite_id);

            ctx.class_store.audit(
                &pending.display_name,
                "class.join",
                &student_node_id.to_string(),
                "ok",
            );

            CommandOutcome {
                response: ClientResponse::JoinRequested(format!(
                    "Join request submitted for '{}'. Waiting for teacher approval.",
                    pending.display_name
                )),
                action: None,
            }
        }
        ClientCommand::ClassJoinRequestsList => {
            match ctx
                .class_store
                .load_members_by_status(MemberStatus::Pending)
            {
                Ok(members) => CommandOutcome {
                    response: ClientResponse::JoinRequests(members),
                    action: None,
                },
                Err(e) => CommandOutcome {
                    response: ClientResponse::Error(format!("Failed to load join requests: {e}")),
                    action: None,
                },
            }
        }
        ClientCommand::ClassJoinRequestApprove { node_id } => {
            if let Err(e) = require_teacher(ctx, client_id) {
                return CommandOutcome {
                    response: ClientResponse::Error(e),
                    action: None,
                };
            }
            let node_id_parsed: EndpointId = match node_id.parse() {
                Ok(id) => id,
                Err(e) => {
                    return CommandOutcome {
                        response: ClientResponse::Error(format!("Invalid node ID: {e}")),
                        action: None,
                    };
                }
            };

            let mut member = match ctx.class_store.load_member(&node_id_parsed) {
                Ok(Some(m)) => m,
                Ok(None) => {
                    return CommandOutcome {
                        response: ClientResponse::Error("Member not found".to_string()),
                        action: None,
                    };
                }
                Err(e) => {
                    return CommandOutcome {
                        response: ClientResponse::Error(format!("Failed to load member: {e}")),
                        action: None,
                    };
                }
            };

            if member.status != MemberStatus::Pending {
                return CommandOutcome {
                    response: ClientResponse::Error(format!(
                        "Member status is {:?}, expected Pending",
                        member.status
                    )),
                    action: None,
                };
            }

            member.status = MemberStatus::Approved;
            if let Err(e) = ctx.class_store.save_member(&member) {
                return CommandOutcome {
                    response: ClientResponse::Error(format!("Failed to update member: {e}")),
                    action: None,
                };
            }

            CommandOutcome {
                response: ClientResponse::JoinRequestApproved(format!(
                    "Approved {} ({})",
                    member.display_name, node_id_parsed
                )),
                action: None,
            }
        }
        ClientCommand::ClassJoinRequestDeny { node_id } => {
            if let Err(e) = require_teacher(ctx, client_id) {
                return CommandOutcome {
                    response: ClientResponse::Error(e),
                    action: None,
                };
            }
            let node_id_parsed: EndpointId = match node_id.parse() {
                Ok(id) => id,
                Err(e) => {
                    return CommandOutcome {
                        response: ClientResponse::Error(format!("Invalid node ID: {e}")),
                        action: None,
                    };
                }
            };

            let mut member = match ctx.class_store.load_member(&node_id_parsed) {
                Ok(Some(m)) => m,
                Ok(None) => {
                    return CommandOutcome {
                        response: ClientResponse::Error("Member not found".to_string()),
                        action: None,
                    };
                }
                Err(e) => {
                    return CommandOutcome {
                        response: ClientResponse::Error(format!("Failed to load member: {e}")),
                        action: None,
                    };
                }
            };

            if member.status != MemberStatus::Pending {
                return CommandOutcome {
                    response: ClientResponse::Error(format!(
                        "Member status is {:?}, expected Pending",
                        member.status
                    )),
                    action: None,
                };
            }

            member.status = MemberStatus::Removed;
            if let Err(e) = ctx.class_store.save_member(&member) {
                return CommandOutcome {
                    response: ClientResponse::Error(format!("Failed to update member: {e}")),
                    action: None,
                };
            }

            // Clean up shared_secret
            let _ = ctx
                .storage
                .remove_config(&format!("shared_secret_{}", node_id_parsed));

            CommandOutcome {
                response: ClientResponse::JoinRequestDenied(format!(
                    "Denied {} ({})",
                    member.display_name, node_id_parsed
                )),
                action: None,
            }
        }
        ClientCommand::ClassGroupCreate { group_name } => {
            if let Err(e) = require_teacher(ctx, client_id) {
                return CommandOutcome {
                    response: ClientResponse::Error(e),
                    action: None,
                };
            }
            let network = match ctx.class_store.load_class_network() {
                Ok(Some(n)) => n,
                _ => {
                    return CommandOutcome {
                        response: ClientResponse::Error("No class network found".to_string()),
                        action: None,
                    };
                }
            };

            let group_id = uuid_v4();
            let now = util::time_now();
            let group = Group {
                group_id: group_id.clone(),
                group_name: group_name.clone(),
                members: vec![],
                created_at: now,
            };

            if let Err(e) = ctx.class_store.save_group(&group) {
                return CommandOutcome {
                    response: ClientResponse::Error(format!("Failed to save group: {e}")),
                    action: None,
                };
            }

            log::info!(
                "Group '{}' ({}) created in class '{}'",
                group_name,
                group_id,
                network.class_name
            );

            CommandOutcome {
                response: ClientResponse::GroupCreated(group),
                action: None,
            }
        }
        ClientCommand::ClassGroupAddMember { group_id, node_id } => {
            let node_id_parsed: EndpointId = match node_id.parse() {
                Ok(id) => id,
                Err(e) => {
                    return CommandOutcome {
                        response: ClientResponse::Error(format!("Invalid node ID: {e}")),
                        action: None,
                    };
                }
            };

            let mut group = match ctx.class_store.load_group(group_id) {
                Ok(Some(g)) => g,
                Ok(None) => {
                    return CommandOutcome {
                        response: ClientResponse::Error("Group not found".to_string()),
                        action: None,
                    };
                }
                Err(e) => {
                    return CommandOutcome {
                        response: ClientResponse::Error(format!("Failed to load group: {e}")),
                        action: None,
                    };
                }
            };

            if group.members.contains(&node_id_parsed) {
                return CommandOutcome {
                    response: ClientResponse::Error("Member already in this group".to_string()),
                    action: None,
                };
            }

            // Verify member exists and is approved
            let mut member = match ctx.class_store.load_member(&node_id_parsed) {
                Ok(Some(m)) => m,
                _ => {
                    return CommandOutcome {
                        response: ClientResponse::Error("Class member not found".to_string()),
                        action: None,
                    };
                }
            };

            group.members.push(node_id_parsed);

            if !member.groups.contains(group_id) {
                member.groups.push(group_id.clone());
            }

            if let Err(e) = ctx.class_store.save_group(&group) {
                return CommandOutcome {
                    response: ClientResponse::Error(format!("Failed to update group: {e}")),
                    action: None,
                };
            }
            if let Err(e) = ctx.class_store.save_member(&member) {
                return CommandOutcome {
                    response: ClientResponse::Error(format!("Failed to update member: {e}")),
                    action: None,
                };
            }

            CommandOutcome {
                response: ClientResponse::Ack(format!(
                    "Added {} to group '{}'",
                    member.display_name, group.group_name
                )),
                action: None,
            }
        }
        ClientCommand::ClassGroupRemoveMember { group_id, node_id } => {
            let node_id_parsed: EndpointId = match node_id.parse() {
                Ok(id) => id,
                Err(e) => {
                    return CommandOutcome {
                        response: ClientResponse::Error(format!("Invalid node ID: {e}")),
                        action: None,
                    };
                }
            };

            let mut group = match ctx.class_store.load_group(group_id) {
                Ok(Some(g)) => g,
                Ok(None) => {
                    return CommandOutcome {
                        response: ClientResponse::Error("Group not found".to_string()),
                        action: None,
                    };
                }
                Err(e) => {
                    return CommandOutcome {
                        response: ClientResponse::Error(format!("Failed to load group: {e}")),
                        action: None,
                    };
                }
            };

            group.members.retain(|id| id != &node_id_parsed);

            let mut member = match ctx.class_store.load_member(&node_id_parsed) {
                Ok(Some(m)) => m,
                _ => {
                    return CommandOutcome {
                        response: ClientResponse::Error("Class member not found".to_string()),
                        action: None,
                    };
                }
            };

            member.groups.retain(|g| g != group_id);

            if let Err(e) = ctx.class_store.save_group(&group) {
                return CommandOutcome {
                    response: ClientResponse::Error(format!("Failed to update group: {e}")),
                    action: None,
                };
            }
            if let Err(e) = ctx.class_store.save_member(&member) {
                return CommandOutcome {
                    response: ClientResponse::Error(format!("Failed to update member: {e}")),
                    action: None,
                };
            }

            CommandOutcome {
                response: ClientResponse::Ack(format!(
                    "Removed {} from group '{}'",
                    member.display_name, group.group_name
                )),
                action: None,
            }
        }
        ClientCommand::ClassGroupMembersList { group_id } => {
            let group = match ctx.class_store.load_group(group_id) {
                Ok(Some(g)) => g,
                Ok(None) => {
                    return CommandOutcome {
                        response: ClientResponse::Error("Group not found".to_string()),
                        action: None,
                    };
                }
                Err(e) => {
                    return CommandOutcome {
                        response: ClientResponse::Error(format!("Failed to load group: {e}")),
                        action: None,
                    };
                }
            };

            let mut members = Vec::new();
            for node_id in &group.members {
                if let Ok(Some(m)) = ctx.class_store.load_member(node_id) {
                    members.push(m);
                }
            }

            CommandOutcome {
                response: ClientResponse::GroupMembers(members),
                action: None,
            }
        }
        ClientCommand::ClassGroupDelete { group_id } => {
            if let Err(e) = require_teacher(ctx, client_id) {
                return CommandOutcome {
                    response: ClientResponse::Error(e),
                    action: None,
                };
            }
            let group = match ctx.class_store.load_group(group_id) {
                Ok(Some(g)) => g,
                Ok(None) => {
                    return CommandOutcome {
                        response: ClientResponse::Error("Group not found".to_string()),
                        action: None,
                    };
                }
                Err(e) => {
                    return CommandOutcome {
                        response: ClientResponse::Error(format!("Failed to load group: {e}")),
                        action: None,
                    };
                }
            };

            // Remove group reference from all members
            for node_id in &group.members {
                if let Ok(Some(mut member)) = ctx.class_store.load_member(node_id) {
                    member.groups.retain(|g| g != group_id);
                    let _ = ctx.class_store.save_member(&member);
                }
            }

            if let Err(e) = ctx.class_store.delete_group(group_id) {
                return CommandOutcome {
                    response: ClientResponse::Error(format!("Failed to delete group: {e}")),
                    action: None,
                };
            }

            CommandOutcome {
                response: ClientResponse::Ack(format!("Group '{}' deleted", group.group_name)),
                action: None,
            }
        }
        ClientCommand::ClassBroadcast { message } => {
            if let Err(e) = require_teacher(ctx, client_id) {
                return CommandOutcome {
                    response: ClientResponse::Error(e),
                    action: None,
                };
            }
            let members = match ctx
                .class_store
                .load_members_by_status(MemberStatus::Approved)
            {
                Ok(m) => m,
                Err(e) => {
                    return CommandOutcome {
                        response: ClientResponse::Error(format!("Failed to load members: {e}")),
                        action: None,
                    };
                }
            };

            let msg_id = format!("bc-{}-{}", util::time_now(), rand::random::<u16>());
            let now = util::time_now();
            let mut sent = 0usize;
            let mut failed = 0usize;
            let mut failures: Vec<String> = Vec::new();

            for member in &members {
                if member.node_id == ctx.me.node_id {
                    sent += 1;
                    continue;
                }
                let class_msg = crate::domain::message::Message::ClassText {
                    id: msg_id.clone(),
                    from_node: ctx.me.node_id,
                    text: message.clone(),
                    timestamp: now,
                };
                match ctx.send_message_to(&member.node_id, class_msg).await {
                    Ok(_) => sent += 1,
                    Err(e) => {
                        failed += 1;
                        failures.push(format!("{}: {}", member.display_name, e));
                    }
                }
            }

            ctx.class_store.audit(
                &ctx.me.domain,
                "class.broadcast",
                &format!("{}/{} recipients", sent, members.len()),
                if failed == 0 { "ok" } else { "partial" },
            );

            CommandOutcome {
                response: ClientResponse::BroadcastResult {
                    total: members.len(),
                    sent,
                    failed,
                    failures,
                },
                action: None,
            }
        }
        ClientCommand::ClassMulticast { group_id, message } => {
            let group = match ctx.class_store.load_group(group_id) {
                Ok(Some(g)) => g,
                Ok(None) => {
                    return CommandOutcome {
                        response: ClientResponse::Error("Group not found".to_string()),
                        action: None,
                    };
                }
                Err(e) => {
                    return CommandOutcome {
                        response: ClientResponse::Error(format!("Failed to load group: {e}")),
                        action: None,
                    };
                }
            };

            let msg_id = format!("mc-{}-{}", util::time_now(), rand::random::<u16>());
            let now = util::time_now();
            let mut sent = 0usize;
            let mut failed = 0usize;
            let mut failures: Vec<String> = Vec::new();
            let total = group.members.len();

            for node_id in &group.members {
                if node_id == &ctx.me.node_id {
                    sent += 1;
                    continue;
                }
                let class_msg = crate::domain::message::Message::ClassText {
                    id: msg_id.clone(),
                    from_node: ctx.me.node_id,
                    text: message.clone(),
                    timestamp: now,
                };
                match ctx.send_message_to(node_id, class_msg).await {
                    Ok(_) => sent += 1,
                    Err(e) => {
                        failed += 1;
                        failures.push(format!("{}: {}", node_id, e));
                    }
                }
            }

            CommandOutcome {
                response: ClientResponse::BroadcastResult {
                    total,
                    sent,
                    failed,
                    failures,
                },
                action: None,
            }
        }
        ClientCommand::ClassMessageStatus { message_id } => {
            match ctx.class_store.load_outbox_record(message_id) {
                Ok(Some(record)) => {
                    let json = serde_json::json!({
                        "message_id": record.message_id,
                        "status": format!("{:?}", record.status),
                        "retry_count": record.retry_count,
                        "max_retries": record.max_retries,
                        "next_retry_at": record.next_retry_at,
                        "last_error": record.last_error,
                    });
                    CommandOutcome {
                        response: ClientResponse::MessageStatus(
                            serde_json::to_string_pretty(&json).unwrap_or_default(),
                        ),
                        action: None,
                    }
                }
                Ok(None) => CommandOutcome {
                    response: ClientResponse::Error("Message not found".to_string()),
                    action: None,
                },
                Err(e) => CommandOutcome {
                    response: ClientResponse::Error(format!("Failed to load message: {e}")),
                    action: None,
                },
            }
        }
        ClientCommand::ClassMessageRetry { message_id } => {
            let mut record = match ctx.class_store.load_outbox_record(message_id) {
                Ok(Some(r)) => r,
                Ok(None) => {
                    return CommandOutcome {
                        response: ClientResponse::Error("Message not found".to_string()),
                        action: None,
                    };
                }
                Err(e) => {
                    return CommandOutcome {
                        response: ClientResponse::Error(format!("Failed to load message: {e}")),
                        action: None,
                    };
                }
            };

            let now = util::time_now();
            record.next_retry_at = now;
            record.retry_count = 0;
            record.status = crate::class_network::messaging::DeliveryStatus::Created;
            if let Err(e) = ctx.class_store.save_outbox_record(&record) {
                return CommandOutcome {
                    response: ClientResponse::Error(format!("Failed to queue retry: {e}")),
                    action: None,
                };
            }

            CommandOutcome {
                response: ClientResponse::Ack(format!(
                    "Message {} queued for retry",
                    message_id
                )),
                action: None,
            }
        }
        ClientCommand::ClassAuditList => {
            match ctx.class_store.load_all_audit_events() {
                Ok(events) => {
                    let json = serde_json::json!({
                        "events": events.iter().map(|e| serde_json::json!({
                            "event_id": e.event_id,
                            "actor": e.actor,
                            "action": e.action,
                            "target": e.target,
                            "timestamp": e.timestamp,
                            "result": e.result,
                        })).collect::<Vec<_>>(),
                        "count": events.len(),
                    });
                    CommandOutcome {
                        response: ClientResponse::AuditLog(
                            serde_json::to_string_pretty(&json).unwrap_or_default(),
                        ),
                        action: None,
                    }
                }
                Err(e) => CommandOutcome {
                    response: ClientResponse::Error(format!("Failed to load audit log: {e}")),
                    action: None,
                },
            }
        }
        ClientCommand::ClassDirectSend { node_id, message } => {
            let target: EndpointId = match node_id.parse() {
                Ok(id) => id,
                Err(e) => {
                    return CommandOutcome {
                        response: ClientResponse::Error(format!("Invalid node ID: {e}")),
                        action: None,
                    };
                }
            };

            let msg_id = format!("dm-{}-{}", util::time_now(), rand::random::<u16>());
            let now = util::time_now();
            let class_msg = crate::domain::message::Message::ClassText {
                id: msg_id,
                from_node: ctx.me.node_id,
                text: message.clone(),
                timestamp: now,
            };

            match ctx.send_message_to(&target, class_msg).await {
                Ok(_) => CommandOutcome {
                    response: ClientResponse::Ack(format!("Message sent to {}", target)),
                    action: None,
                },
                Err(e) => CommandOutcome {
                    response: ClientResponse::Error(format!("Send failed: {e}")),
                    action: None,
                },
            }
        }
    }
}
