use std::{path::PathBuf, fs, sync::{Arc, Mutex}, time::Duration};

use clap::Parser;
use mpris::{PlayerFinder, PlaybackStatus};
use serde::{Serialize, Deserialize};
use tokio::{net::{UnixListener, UnixStream}, time, task};
use tokio_util::codec::{LengthDelimitedCodec, Framed};

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
    pub socket: PathBuf
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
        handle_mpris_daemon(last_active_mpris).await;
    });

    let listener = UnixListener::bind(&args.socket).expect("failed to bind socket");
    loop {
        let last_active_listener = last_active_players.clone();
        match listener.accept().await {
            Ok((stream, _addr)) => {
                tokio::spawn(async move {
                    process(stream, last_active_listener).await;
                });
            },
            Err(e) => {
                eprintln!("{:?}", e);
            }
        }
    }
}

async fn handle_mpris_daemon(shared_data: SharedData) {

    let mut interval = time::interval(Duration::from_millis(250));
    loop {
        interval.tick().await;
        let player_finder = PlayerFinder::new().expect("Could not connect to D-Bus");
        // get active players
        let active_players: Vec<String> = player_finder
            .iter_players()
            .expect("Could not iterate players")
            .map(|x| x.expect("Could not get one of the players"))
            .filter(|x| x.get_playback_status().expect("Cannot get playback status") == PlaybackStatus::Playing)
            .map(|x| String::from(x.identity()))
            .collect();

        if active_players.len() == 0 {
            continue; // there is nothing to do
        }

        // change active players
        let mut guard = shared_data.lock().expect("Could not lock the mutex");
        *guard = active_players;
    }
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
            },
            Err(err) => {
                eprintln!("Could not read from client: {:?}", err);
                break;
            }
        }
    }

    task::yield_now().await;
}

async fn handle_request(request: Request, shared_data: SharedData) -> Response {
    match request {
        Request::GetLastActive => {
            let guard = shared_data.lock().expect("Could not lock the mutex.");
            Response::Players(guard.clone())
        },
        _ => Response::None
    }
}
