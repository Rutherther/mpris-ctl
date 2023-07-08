use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};
use mpris::{Player, PlayerFinder, PlaybackStatus, MetadataValueKind};
use serde::{Serialize, Deserialize};
use tokio::{net::UnixStream, io};
use tokio_util::codec::{Framed, LengthDelimitedCodec};

use futures::{prelude::stream::StreamExt, SinkExt};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Request {
    GetLastActive,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Response {
    None,
    Players(Vec<String>),
}

#[derive(Debug, Parser)]
#[command(version = "v0.1")]
#[command(author = "Rutherther")]
#[command(about = "Manage dbus mpris2 players")]
pub struct Cli {
    #[command(flatten)]
    pub player_selector: PlayerSelector,

    #[arg(short = 's', long, default_value = r"/tmp/mpris-ctl.sock", value_hint = clap::ValueHint::FilePath)]
    pub socket: PathBuf,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Args)]
pub struct PlayerSelector {
    #[arg(long)]
    pub all_players: bool,
    #[arg(long)]
    pub player: Vec<String>,
}

#[derive(Debug, Subcommand, Eq, PartialEq)]
pub enum Commands {
    #[command(about = "Send play media command")]
    Play,
    #[command(about = "Send pause media command")]
    Pause,
    #[command(about = "Send play if paused, else send pause")]
    Toggle,
    #[command(about = "Switch to previous media/song")]
    Prev,
    #[command(about = "Switch to next media/song")]
    Next,
    #[command(about = "Obtain metadata of the currently playing media")]
    Metadata(Metadata),
    #[command(about = "Obtain status of the currently active player")]
    Status,
    #[command(about = "List all available players")]
    List
}

#[derive(Debug, Args, Eq, PartialEq)]
pub struct Metadata {
    #[arg(help = "Key of the metadata to obtain, else all information will be obtained.")]
    pub key: Option<String>
}

#[tokio::main]
async fn main() {
    let args = Cli::parse();

    let selected_players = obtain_selected_players(&args.socket, args.player_selector).await;

    if selected_players.len() == 0 {
        println!("No players matching the criteria found.");
        return;
    }

    match args.command {
        Commands::Play => {
            for player in selected_players {
                player.play().expect("Could not play.");
            }
        },
        Commands::Pause => {
            for player in selected_players {
                player.pause().expect("Could not pause.");
            }
        },
        Commands::Toggle => {
            for player in selected_players {
                player.play_pause().expect("Could not toggle.");
            }
        },
        Commands::Prev => {
            for player in selected_players {
                player.previous().expect("Could not return back to previous.");
            }
        },
        Commands::Next => {
            for player in selected_players {
                player.next().expect("Could not skip.");
            }
        },
        Commands::Status => {
            let player = selected_players.first().unwrap();
            println!("{}", match player.get_playback_status().unwrap() {
                PlaybackStatus::Playing => "Playing",
                PlaybackStatus::Paused => "Paused",
                PlaybackStatus::Stopped => "Stopped",
            });
        },
        Commands::Metadata(Metadata { key: search_key }) => {
            for player in selected_players {
                let identity = get_short_name(player.identity());
                let metadata = player
                    .get_metadata()
                    .expect("Could not obtain metadata");

                let mut keys = metadata
                    .keys()
                    .collect::<Vec<_>>();
                keys.sort();

                let metadata = keys
                    .iter()
                    .map(|key| (key, metadata.get(key).unwrap()));

                for (key, value) in metadata {
                    if let Some(skey) = &search_key {
                        if key.contains(skey) {
                            println!("{}", &value.as_str().unwrap_or("-"));
                            break;
                        }
                    } else {
                        println!(
                            "{} {} {}",
                            identity,
                            key,
                            match value.kind() {
                                MetadataValueKind::String => value.as_str().unwrap().to_string(),
                                MetadataValueKind::Array => value.as_str_array().map(|x| x.join(" ")).unwrap(),
                                MetadataValueKind::U32 |
                                MetadataValueKind::U16 |
                                MetadataValueKind::U64 => value.as_u64().unwrap().to_string(),
                                MetadataValueKind::I32 |
                                MetadataValueKind::I16 |
                                MetadataValueKind::I64 => value.as_i64().unwrap().to_string(),
                                MetadataValueKind::F64 => value.as_f64().unwrap().to_string(),
                                _ => "-".to_string()
                            }
                        );
                    }
                }
            }
        },
        Commands::List => {
            for player in selected_players {
                println!("{:?}", player.identity());
            }
        }
    };
}

fn get_short_name(name: &str) -> String {
    name.split(' ')
        .last()
        .map(|x| x.to_lowercase())
        .unwrap_or(name.to_string())
}

async fn obtain_selected_players(socket: &PathBuf, selector: PlayerSelector) -> Vec<Player> {
    let player_finder = PlayerFinder::new()
        .expect("Could not connect to the D-Bus.");
    if selector.all_players {
        return player_finder
            .find_all()
            .expect("Could not iterate the players.");
    }

    if selector.player.len() > 0 {
        return player_finder
            .iter_players()
            .expect("Could not iterate the players.")
            .map(|x| x.expect("Could not obtain player."))
            .filter(|x| selector.player.iter().any(|sel| x.identity().to_lowercase().contains(&sel.to_lowercase())))
            .collect();
    }

    let daemon_result = obtain_daemon_active_players(&player_finder, socket).await;
    if let Ok(players) = daemon_result {
        players
    } else {
        let mut players: Vec<Player> = player_finder
            .iter_players()
            .expect("Could not iterate the players.")
            .map(|x| x.expect("Could not obtain a player."))
            .filter(|x| x.get_playback_status().expect("Could not obtain playback status") == PlaybackStatus::Playing)
            .collect();

        if players.len() == 0 {
            if let Ok(player) = player_finder.find_active() {
                players.push(player);
            } else if let Ok(player) = player_finder.find_first() {
                players.push(player);
            }
        }

        players
    }
}

async fn obtain_daemon_active_players(player_finder: &PlayerFinder, socket: &PathBuf) -> io::Result<Vec<Player>> {
    let stream = UnixStream::connect(socket).await?;
    let mut framed = Framed::new(stream, LengthDelimitedCodec::new());

    let buffer = serde_json::to_vec(&Request::GetLastActive).unwrap();
    framed.send(buffer.into()).await.expect("Could not send request to the daemon");

    let response = if let Some(frame) = framed.next().await {
        match frame {
            Ok(data) => {
                let response: serde_json::Result<Response> = serde_json::from_slice(&data);
                if response.is_err() {
                    panic!("Could not parse the frame from server: {:?}", response.err());
                }
                response.unwrap()
            },
            Err(err) => {
                panic!("Could not read from server: {:?}", err);
            }
        }
    } else {
        panic!("Could not obtain data from the server.")
    };

    match response {
        Response::Players(players) => {
            player_finder
                .iter_players()
                .expect("Could not iterate the players.")
                .map(|x| x.expect("Could not obtain a player."))
                .filter(|x| players.contains(&String::from(x.identity())))
                .map(|x| Ok(x))
                .collect()
        },
        _ => panic!("Could not get active players from the daemon.")
    }
}
