//! FaintSignal CLI — link, sync, send, receive, measure RSS.

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use faint_signal_core::{
    paths, print_rss, Session, SessionConfig, Servers,
};
use tracing_subscriber::EnvFilter;
use uuid::Uuid;

#[derive(Parser, Debug)]
#[command(name = "faintsignal", about = "Memory-efficient Signal desktop client (Linux)")]
struct Args {
    #[arg(long, env = "FAINTSIGNAL_PASSPHRASE")]
    passphrase: Option<String>,

    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Link as a secondary device (scan QR with your phone).
    LinkDevice {
        #[arg(long, default_value = "FaintSignal")]
        device_name: String,
        #[arg(long, default_value = "production")]
        servers: String,
    },
    /// Show whoami for the linked account.
    Whoami,
    /// Receive pending messages (pass --watch to stay connected).
    Receive {
        #[arg(long)]
        watch: bool,
    },
    /// Send a text message to a contact ACI UUID.
    Send {
        recipient: Uuid,
        message: String,
    },
    /// Send a text message to a group (32-byte master key as hex).
    SendGroup {
        master_key_hex: String,
        message: String,
    },
    /// List cached conversations from the app store.
    Conversations {
        #[arg(long, default_value_t = 50)]
        limit: u32,
    },
    /// Print process RSS (for memory budget checks).
    Rss,
    /// Show data paths.
    Paths,
    /// Start an outgoing 1:1 voice call (needs RingRTC; see docs/calling.md).
    Call {
        peer: Uuid,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("info".parse()?))
        .init();

    let args = Args::parse();
    let faint_paths = paths().context("resolve FaintSignal paths")?;
    let config = SessionConfig {
        passphrase: args.passphrase.clone(),
        ..SessionConfig::default()
    };

    match args.cmd {
        Cmd::Paths => {
            println!("data_dir:      {}", faint_paths.data_dir.display());
            println!("signal_db:    {}", faint_paths.signal_db.display());
            println!("app_db:       {}", faint_paths.app_db.display());
            println!("attachments:  {}", faint_paths.attachments_dir.display());
            print_rss("cli");
        }
        Cmd::Rss => {
            print_rss("cli");
        }
        Cmd::LinkDevice {
            device_name,
            servers,
        } => {
            let servers = parse_servers(&servers)?;
            let config = SessionConfig {
                device_name,
                servers,
                passphrase: args.passphrase.clone(),
            };
            print_rss("before-link");
            let session = Session::link_device(faint_paths, config).await?;
            println!("{}", session.whoami_string().await?);
            print_rss("after-link");
        }
        Cmd::Whoami => {
            let session = Session::load(faint_paths, &config).await?;
            println!("{}", session.whoami_string().await?);
            print_rss("whoami");
        }
        Cmd::Receive { watch } => {
            let mut session = Session::load(faint_paths, &config).await?;
            print_rss("before-receive");
            session.receive_loop(watch).await?;
            print_rss("after-receive");
        }
        Cmd::Send { recipient, message } => {
            let mut session = Session::load(faint_paths, &config).await?;
            session.send_text(recipient, message).await?;
            println!("sent");
            print_rss("after-send");
        }
        Cmd::SendGroup {
            master_key_hex,
            message,
        } => {
            let raw = hex_decode_32(&master_key_hex)?;
            let mut session = Session::load(faint_paths, &config).await?;
            session.send_text_to_group(&raw, message).await?;
            println!("sent to group");
            print_rss("after-send-group");
        }
        Cmd::Conversations { limit } => {
            let session = Session::load(faint_paths, &config).await?;
            for c in session.app.list_conversations(limit)? {
                println!(
                    "{}  [{}]  unread={}  last={:?}  {}",
                    c.id, c.kind, c.unread, c.last_message_at, c.title
                );
            }
        }
        Cmd::Call { peer } => {
            let session = Session::load(faint_paths, &config).await?;
            match session.start_voice_call(peer).await {
                Ok(()) => println!("call started (media via RingRTC when linked)"),
                Err(e) => {
                    eprintln!("call failed: {e}");
                    eprintln!("See docs/calling.md to build RingRTC for Linux.");
                    std::process::exit(1);
                }
            }
        }
    }

    Ok(())
}

fn parse_servers(s: &str) -> Result<Servers> {
    match s.to_ascii_lowercase().as_str() {
        "production" | "prod" => Ok(Servers::Production),
        "staging" => Ok(Servers::Staging),
        other => anyhow::bail!("unknown servers '{other}' (production|staging)"),
    }
}

fn hex_decode_32(s: &str) -> Result<[u8; 32]> {
    let s = s.trim().trim_start_matches("0x");
    if s.len() != 64 {
        anyhow::bail!("group master key must be 64 hex chars");
    }
    let mut out = [0u8; 32];
    for i in 0..32 {
        out[i] = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16)
            .map_err(|e| anyhow::anyhow!("bad hex: {e}"))?;
    }
    Ok(out)
}
