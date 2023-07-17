use std::{
    error::Error,
    fs,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};

use clap::Parser;
use mpris::{PlaybackStatus, PlayerFinder};
use serde::{Deserialize, Serialize};
use tokio::{
    net::{UnixListener, UnixStream},
    time,
};
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
pub struct Cli {
    #[arg(short = 'c', long, default_value = r"config.json", value_hint = clap::ValueHint::FilePath)]
    pub config: PathBuf,
    #[arg(short = 's', long, default_value = r"/tmp/mpris-ctl.sock", value_hint = clap::ValueHint::FilePath)]
    pub socket: PathBuf,
}

type SharedData = Arc<Mutex<Vec<String>>>;

#[tokio::main]
async fn main() {
    let args = Cli::parse();

    if args.socket.exists() {
        fs::remove_file(&args.socket).unwrap();
    }

    let last_active_players = Arc::new(Mutex::new(Vec::<String>::new()));

    let last_active_mpris = last_active_players.clone();
    let _mpris_handle = tokio::spawn(async move {
        handle_mpris_daemon_loop(last_active_mpris).await;
    });

    let listener = UnixListener::bind(&args.socket).expect("failed to bind socket");
    loop {
        let last_active_listener = last_active_players.clone();
        match listener.accept().await {
            Ok((stream, _addr)) => {
                tokio::spawn(async move {
                    process(stream, last_active_listener).await;
                });
            }
            Err(e) => {
                eprintln!("{:?}", e);
            }
        }
    }
}

async fn handle_mpris_daemon_loop(shared_data: SharedData) {
    let mut interval = time::interval(Duration::from_millis(250));
    loop {
        interval.tick().await;
        match handle_mpris_daemon(&shared_data).await {
            Ok(..) => (),
            Err(err) => eprintln!("Got an error in the daemon player finder loop: {}", err),
        }
    }
}

async fn handle_mpris_daemon(shared_data: &SharedData) -> Result<(), Box<dyn Error + '_>> {
    let player_finder = PlayerFinder::new()?;

    let mut errors = vec![];
    let active_players: Vec<_> = player_finder
        .iter_players()?
        .filter_map(|x| {
            let player_result = x.map_err(|e| errors.push(e)).ok();
            let status = if let Some(player) = &player_result {
                player
                    .get_playback_status()
                    .map_err(|e| errors.push(e))
                    .ok()
            } else {
                Option::None
            };

            if status.is_some() && status.unwrap() == PlaybackStatus::Playing {
                player_result
            } else {
                Option::None
            }
        })
        .map(|x| String::from(x.identity()))
        .collect();

    if active_players.len() != 0 {
        let mut guard = shared_data.lock()?;
        *guard = active_players;
    }

    for error in errors {
        // TODO: return the errors correctly (this is not so easy as the DBusError does not contain copy trait)
        // maybe create a custom error type somehow wrapping dbus error?
        eprintln!("An errorw hen obtaining a player has occurred: {}", error);
    }

    Ok(())
}

async fn process(stream: UnixStream, shared_data: SharedData) {
    let mut framed = Framed::new(stream, LengthDelimitedCodec::new());
    while let Some(frame) = framed.next().await {
        match frame {
            Ok(data) => {
                let request: serde_json::Result<Request> = serde_json::from_slice(&data);
                if request.is_err() {
                    eprintln!("Could not parse the frame from client: {:?}", request.err());
                    break;
                }

                let request = request.unwrap();
                let response = handle_request(request, shared_data.clone()).await;
                let buffer = serde_json::to_vec(&response).unwrap();

                let send = framed.send(buffer.into()).await;
                if send.is_err() {
                    eprintln!("Could not send to the client: {:?}", send.err());
                    break;
                }
            }
            Err(err) => {
                eprintln!("Could not read from client: {:?}", err);
                break;
            }
        }
    }
}

async fn handle_request(request: Request, shared_data: SharedData) -> Response {
    match request {
        Request::GetLastActive => {
            let guard = shared_data.lock().expect("Could not lock the mutex.");
            Response::Players(guard.clone())
        }
        _ => Response::None,
    }
}
