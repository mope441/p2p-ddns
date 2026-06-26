use anyhow::Result;
use clap::Parser;

use p2p_ddns::{app, cli::args::DaemonArgs};

#[tokio::main]
async fn main() -> Result<()> {
    let args = DaemonArgs::parse();
    app::init_logging(args.log);
    app::run_daemon(args).await
}
