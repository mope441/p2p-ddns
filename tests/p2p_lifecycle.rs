use std::{collections::BTreeMap, net::SocketAddr, sync::Arc, time::Duration};

use anyhow::Result;
use iroh::{EndpointAddr, SecretKey, TransportAddr};
use p2p_ddns::{
    cli::args::DaemonArgs,
    domain::{message::Message, node::Node},
    net::init_network,
    storage::Storage,
    util::time_now,
};

async fn make_context(
    domain: &str,
    dir: &tempfile::TempDir,
    bind: &str,
    primary: bool,
    ticket: Option<String>,
) -> Result<(
    p2p_ddns::net::Context,
    iroh_gossip::api::GossipReceiver,
    futures::channel::mpsc::Receiver<bytes::Bytes>,
)> {
    let args = DaemonArgs {
        daemon: false,
        primary,
        domain: Some(domain.to_string()),
        ticket,
        config: Some(dir.path().to_path_buf()),
        bind: Some(bind.to_string()),
        no_mdns: true,
        dht: false,
        ..DaemonArgs::default()
    };
    DaemonArgs::validate(&args)?;

    let storage = Storage::new(dir.path().join("storage.db"))?;
    init_network(args, storage).await
}

#[tokio::test]
async fn cleanup_removes_stale_nodes() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let (ctx, _gos, _sp) = make_context("a", &dir, "127.0.0.1:0", true, None).await?;

    let mut rng = rand::rng();
    let pk = SecretKey::generate(&mut rng).public();
    ctx.nodes.insert(
        pk,
        Node {
            node_id: pk,
            invitor: pk,
            addr: iroh::EndpointAddr::new(pk),
            domain: "stale".to_string(),
            services: BTreeMap::new(),
            last_heartbeat: time_now().saturating_sub(24 * 60 * 60),
            role: None,
        },
    );

    ctx.cleanup().await;
    assert!(!ctx.nodes.contains_key(&pk));
    Ok(())
}

#[tokio::test]
async fn left_message_removes_joined_node() -> Result<()> {
    let dir_a = tempfile::tempdir()?;
    let dir_b = tempfile::tempdir()?;

    let (ctx_a, gos_a, sp_a) = make_context("a", &dir_a, "127.0.0.1:0", true, None).await?;
    let ticket = ctx_a.ticket.to_string();
    let (ctx_b, gos_b, sp_b) =
        make_context("b", &dir_b, "127.0.0.1:0", false, Some(ticket)).await?;

    let id_a = ctx_a.me.node_id;
    let id_b = ctx_b.me.node_id;
    let rnum_b = ctx_b.ticket.rnum();

    let ctx_a = Arc::new(ctx_a);
    let ctx_b = Arc::new(ctx_b);

    let task_a = {
        let ctx_a = ctx_a.clone();
        tokio::spawn(async move { ctx_a.run_for(gos_a, sp_a, Duration::from_secs(3)).await })
    };
    let task_b = {
        let ctx_b = ctx_b.clone();
        tokio::spawn(async move { ctx_b.run_for(gos_b, sp_b, Duration::from_secs(3)).await })
    };

    ctx_b
        .send_message_to(
            &id_a,
            Message::Invited {
                topic: ctx_a.ticket.topic(),
                rnum: rnum_b,
                addr: ctx_b.handle.addr(),
                alias: ctx_b.me.domain.clone(),
                services: BTreeMap::new(),
                invitor: ctx_b.me.invitor,
            },
        )
        .await?;

    tokio::time::timeout(Duration::from_secs(2), async {
        while !ctx_a.nodes.contains_key(&id_b) {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await?;

    ctx_b.send_message_to(&id_a, Message::Left).await?;

    tokio::time::timeout(Duration::from_secs(2), async {
        while ctx_a.nodes.contains_key(&id_b) {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await?;

    let _ = task_a.await;
    let _ = task_b.await;

    assert!(!ctx_a.nodes.contains_key(&id_b));
    Ok(())
}

#[tokio::test]
async fn stale_trusted_daemon_can_resume_with_fresh_about_me() -> Result<()> {
    let dir_a = tempfile::tempdir()?;
    let dir_b = tempfile::tempdir()?;

    let (ctx_a, gos_a, sp_a) = make_context("a", &dir_a, "127.0.0.1:0", true, None).await?;
    let ticket = ctx_a.ticket.to_string();
    let (ctx_b, gos_b, sp_b) =
        make_context("b", &dir_b, "127.0.0.1:0", false, Some(ticket)).await?;

    let id_a = ctx_a.me.node_id;
    let id_b = ctx_b.me.node_id;
    let rnum_b = ctx_b.ticket.rnum();

    let ctx_a = Arc::new(ctx_a);
    let ctx_b = Arc::new(ctx_b);

    let task_a = {
        let ctx_a = ctx_a.clone();
        tokio::spawn(async move { ctx_a.run_for(gos_a, sp_a, Duration::from_secs(5)).await })
    };
    let task_b = {
        let ctx_b = ctx_b.clone();
        tokio::spawn(async move { ctx_b.run_for(gos_b, sp_b, Duration::from_secs(5)).await })
    };

    ctx_b
        .send_message_to(
            &id_a,
            Message::Invited {
                topic: ctx_a.ticket.topic(),
                rnum: rnum_b,
                addr: ctx_b.handle.addr(),
                alias: ctx_b.me.domain.clone(),
                services: BTreeMap::new(),
                invitor: ctx_b.me.invitor,
            },
        )
        .await?;

    tokio::time::timeout(Duration::from_secs(2), async {
        while !ctx_a.nodes.contains_key(&id_b) {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await?;

    {
        let mut joined = ctx_a.nodes.get_mut(&id_b).expect("joined node");
        joined.last_heartbeat = time_now().saturating_sub(24 * 60 * 60);
    }
    ctx_a.cleanup().await;
    assert!(!ctx_a.nodes.contains_key(&id_b));
    assert!(ctx_a.is_node_trusted(&id_b));
    ctx_a.save().await?;
    let persisted = ctx_a.storage.load_nodes::<Vec<_>>()?;
    assert!(persisted.iter().any(|node| node.node_id == id_b));

    ctx_b
        .send_message_to(
            &id_a,
            Message::AboutMe {
                addr: ctx_b.handle.addr(),
                alias: ctx_b.me.domain.clone(),
                services: BTreeMap::new(),
                invitor: ctx_b.me.invitor,
            },
        )
        .await?;

    let resumed = tokio::time::timeout(Duration::from_secs(2), async {
        while !ctx_a.nodes.contains_key(&id_b) {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await;

    let _ = task_a.await;
    let _ = task_b.await;

    assert!(
        resumed.is_ok(),
        "previously trusted daemon should resume via fresh AboutMe after stale cleanup"
    );
    Ok(())
}

#[tokio::test]
async fn trusted_sync_update_replaces_stale_dhcp_addr() -> Result<()> {
    let dir_a = tempfile::tempdir()?;
    let dir_b = tempfile::tempdir()?;

    let (ctx_a, gos_a, sp_a) = make_context("a", &dir_a, "127.0.0.1:0", true, None).await?;
    let ticket = ctx_a.ticket.to_string();
    let (ctx_b, gos_b, sp_b) =
        make_context("b", &dir_b, "127.0.0.1:0", false, Some(ticket)).await?;

    let id_a = ctx_a.me.node_id;
    let id_b = ctx_b.me.node_id;
    let rnum_b = ctx_b.ticket.rnum();

    let ctx_a = Arc::new(ctx_a);
    let ctx_b = Arc::new(ctx_b);

    let task_a = {
        let ctx_a = ctx_a.clone();
        tokio::spawn(async move { ctx_a.run_for(gos_a, sp_a, Duration::from_secs(5)).await })
    };
    let task_b = {
        let ctx_b = ctx_b.clone();
        tokio::spawn(async move { ctx_b.run_for(gos_b, sp_b, Duration::from_secs(5)).await })
    };

    ctx_b
        .send_message_to(
            &id_a,
            Message::Invited {
                topic: ctx_a.ticket.topic(),
                rnum: rnum_b,
                addr: ctx_b.handle.addr(),
                alias: ctx_b.me.domain.clone(),
                services: BTreeMap::new(),
                invitor: ctx_b.me.invitor,
            },
        )
        .await?;

    tokio::time::timeout(Duration::from_secs(2), async {
        while !ctx_a.nodes.contains_key(&id_b) {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await?;

    let mut rng = rand::rng();
    let node_id = SecretKey::generate(&mut rng).public();
    let old_ip: SocketAddr = "10.0.0.20:7777".parse().unwrap();
    let new_ip: SocketAddr = "10.0.0.37:7777".parse().unwrap();

    let old_node = Node {
        node_id,
        invitor: id_b,
        addr: EndpointAddr::from_parts(node_id, [TransportAddr::Ip(old_ip)]),
        domain: "dhcp-node".to_string(),
        services: BTreeMap::new(),
        last_heartbeat: 10,
        role: None,
    };
    ctx_b
        .send_message_to(
            &id_a,
            Message::SyncRequest {
                nodes: vec![old_node],
            },
        )
        .await?;

    tokio::time::timeout(Duration::from_secs(2), async {
        while !ctx_a.nodes.contains_key(&node_id) {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await?;

    let new_node = Node {
        node_id,
        invitor: id_b,
        addr: EndpointAddr::from_parts(node_id, [TransportAddr::Ip(new_ip)]),
        domain: "dhcp-node".to_string(),
        services: BTreeMap::new(),
        last_heartbeat: 20,
        role: None,
    };
    ctx_b
        .send_message_to(
            &id_a,
            Message::SyncRequest {
                nodes: vec![new_node],
            },
        )
        .await?;

    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if let Some(node) = ctx_a.nodes.get(&node_id)
                && node.addr.ip_addrs().any(|addr| addr == &new_ip)
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await?;

    let node = ctx_a.nodes.get(&node_id).expect("synced DHCP node");
    let ips = node.addr.ip_addrs().copied().collect::<Vec<_>>();

    let _ = task_a.await;
    let _ = task_b.await;

    assert!(
        !ips.contains(&old_ip),
        "trusted sync update should replace stale DHCP IP instead of keeping it"
    );
    assert!(ips.contains(&new_ip));
    Ok(())
}
