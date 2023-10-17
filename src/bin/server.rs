//! The binary that will run the server for the asynchronous database

use async_std::{
    prelude::*,
    task,
    net::{TcpListener, ToSocketAddrs, TcpStream},
};

use futures::channel::mpsc::{self, Receiver, Sender};
// use futures::io::AsyncReadExt;
use futures::sink::SinkExt;

use std::fmt::Debug;
use std::sync::Arc;
use std::convert::TryFrom;
use std::collections::HashMap;

mod models;
use models::prelude::*;

mod events;
mod interface;

enum Event {
    NewClient {
        peer_addr: String,
        stream: Arc<TcpStream>,
    },
    AddEpic {
        epic_name: String,
        epic_description: String,
    },
    DeleteEpic {
        epic_id: u32,
    },
    GetEpic {
        epic_id: u32,
    },
    UpdateEpicStatus {
        epic_id: u32,
        status: Status,
    },
    GetStory {
        epic_id: u32,
        story_id: u32,
    },
    AddStory {
        epic_id: u32,
        story_name: String,
        story_description: String,
    },
    DeleteStory {
        epic_id: u32,
        story_id: u32,
    },
    UpdateStoryStatus {
        epic_id: u32,
        story_id: u32,
        status: Status,
    },
    ClientDisconnected
}

impl Event {
    async fn try_from(tag: &[u8; 13], stream: &TcpStream) -> Result<Event, DbError> {
        let event_byte = tag[0];
        let mut stream = stream;
        if event_byte & 1 != 0 {
            let epic_name_len = parse_4_bytes(&tag[1..5]);
            let epic_description_len = parse_4_bytes(&tag[5..9]);
            let mut epic_name_bytes = vec![0u8; epic_name_len as usize];
            let mut epic_description_bytes = vec![0u8; epic_description_len as usize];
            stream.read_exact(&mut epic_name_bytes)
                .await
                .map_err(|_| DbError::ParseError(format!("unable to read epic name bytes from stream")))?;
            stream.read_exact(&mut epic_description_bytes)
                .await
                .map_err(|_| DbError::ParseError(format!("unable to parse epic description bytes from stream")))?;
            let epic_name = String::from_utf8(epic_name_bytes)
                .map_err(|_| DbError::ParseError(format!("unable to parse epic name as well formed utf8")))?;
            let epic_description = String::from_utf8(epic_description_bytes)
                .map_err(|_| DbError::ParseError(format!("unable to parse epic description as well formed utf8")))?;
            Ok(Event::AddEpic { epic_name, epic_description })
        } else if event_byte & 2 != 0 {
            let epic_id = parse_4_bytes(&tag[1..5]);
            Ok(Event::DeleteEpic { epic_id })
        } else if event_byte & 4 != 0 {
            let epic_id = parse_4_bytes(&tag[1..5]);
            Ok(Event::GetEpic { epic_id })
        } else if event_byte & 8 != 0 {
            let epic_id = parse_4_bytes(&tag[1..5]);
            let epic_status = tag[5];
            let status = Status::try_from(epic_status)?;
            Ok(Event::UpdateEpicStatus { epic_id, status })
        } else if event_byte & 16 != 0 {
            let epic_id = parse_4_bytes(&tag[1..5]);
            let story_id = parse_4_bytes(&tag[5..9]);
            Ok(Event::GetStory { epic_id, story_id })
        } else if event_byte & 32 != 0 {
            let epic_id = parse_4_bytes(&tag[1..5]);
            let story_name_len = parse_4_bytes(&tag[5..9]) as usize;
            let story_description_len = parse_4_bytes(&tag[9..]) as usize;
            let mut story_name_bytes = vec![0u8; story_name_len];
            let mut story_description_bytes = vec![0u8; story_description_len];
            stream.read_exact(&mut story_name_bytes)
                .await
                .map_err(|_| DbError::ParseError(format!("unable to read story name bytes from stream")))?;
            stream.read_exact(&mut story_description_bytes)
                .await
                .map_err(|_| DbError::ParseError(format!("unable to ready story description bytes from stream")))?;
            let story_name = String::from_utf8(story_name_bytes)
                .map_err(|_| DbError::ParseError(format!("unable to read story name as well formed utf8")))?;
            let story_description = String::from_utf8(story_description_bytes)
                .map_err(|_| DbError::ParseError(format!("unable to read story description bytes as well formed utf8")))?;
            Ok(Event::AddStory { epic_id, story_name, story_description })
        } else if event_byte & 64 != 0 {
            let epic_id = parse_4_bytes(&tag[1..5]);
            let story_id = parse_4_bytes(&tag[5..9]);
            Ok(Event::DeleteStory { epic_id, story_id })
        } else if event_byte & 128 != 0 {
            let epic_id = parse_4_bytes(&tag[1..5]);
            let story_id = parse_4_bytes(&tag[5..9]);
            let story_status = Status::try_from(tag[10])?;
            Ok(Event::UpdateStoryStatus { epic_id, story_id, status: story_status })
        } else {
            Err(DbError::DoesNotExist(format!("unable to parse tag and stream")))
        }
    }
}

fn parse_4_bytes(bytes: &[u8]) -> u32 {
    let mut res = 0;
    for i in 0..4 {
        res ^= (bytes[i] as u32) << (i * 8);
    }
    res
}

enum Response {
    ClientAlreadyExists,
}

/// Accepts a `addrs` representing a socket address that will listen for incoming connections,
/// and a `channel_buf_size` representing the capacity of channel that connects to the broker task.
/// The function will start a new listener awaiting for incoming connections from clients, it then starts
/// a new broker task, and then passes each client connection to a separate connection task.
async fn accept_loop(addrs: impl ToSocketAddrs + Debug + Clone, channel_buf_size: usize, db_dir: String, db_file_name: String, epic_dir: String) -> Result<(), DbError> {
    // Connect to the servers socket address
    println!("connecting to {:?}...", addrs);
    let addrs_clone = addrs.clone();
    let listener = TcpListener::bind(addrs).await.map_err(|_e| DbError::ConnectionError(format!("could not connect to {:?}", addrs_clone)))?;

    // Get a channel to the broker, and spawn the brokers task
    let (broker_sender, broker_receiver) = mpsc::channel::<Option<Event>>(channel_buf_size);
    task::spawn(broker(broker_receiver, db_dir, db_file_name, epic_dir, channel_buf_size));

    while let Some(stream_res) = listener.incoming().next().await {
        let stream = stream_res.map_err(|_e| DbError::ConnectionError(format!("unable to accept stream")))?;
        // TODO: add error handling to log all errors
        task::spawn(connection_loop(stream, broker_sender.clone()));
    }

    Ok(())
}

/// Takes a `TcpStream` and a `Sender<Option<Event>>` representing the client connection and the sending
/// end of a channel connected to a broker task. Attempts to read new events from the client stream and send them
/// to the broker task. If a new event is successfully read from the client stream it is sent to the broker via `broker_sender`,
/// otherwise it sends `None`. The function can fail if there is an error parsing `client_stream.peer_addr()` as a string,
/// a new `Event` is not able to be sent to the broker.
async fn connection_loop(client_stream: TcpStream, mut broker_sender: Sender<Option<Event>>) -> Result<(), DbError> {
    let client_stream = Arc::new(client_stream);
    let mut client_stream_reader = &*client_stream;

    // TODO: Add better error handling, i.e. custom connect error handling
    let peer_addr = client_stream.peer_addr().map_err(|_| DbError::ParseError(format!("unable to parse address from client")))?.to_string();

    // Create a new client and send to broker
    let new_client = Event::NewClient { peer_addr: peer_addr.clone(), stream: client_stream.clone() };
    broker_sender.send(Some(new_client))
        .await
        .map_err(|_| DbError::ConnectionError(format!("unable to send client to broker")))?;

    let mut tag = [0u8; 13];

    while let Ok(_) = client_stream_reader.read_exact(&mut tag).await {
        // TODO: handle the case when input from user is not parse-able ie send a special error event to the broker to send back to the client.
        match Event::try_from(&tag, client_stream_reader).await {
            Ok(event) => {
                broker_sender
                    .send(Some(event))
                    .await
                    .map_err(|_| DbError::ConnectionError(format!("unable to send event to broker")))?;
            }
            Err(_) => {
                broker_sender
                    .send(None)
                    .await
                    .map_err(|_| DbError::ConnectionError(format!("unable to send event to broker")))?;
            }
        }
    }
    // Handle event when client disconnects
    broker_sender
        .send(Some(Event::ClientDisconnected))
        .await
        .map_err(|_| DbError::ConnectionError(String::from("unable to send event to broker")))?;
    Ok(())
}

async fn connection_write_loop(stream: Arc<TcpStream>, client_receiver: Receiver<Response>) -> Result<(), DbError> {
    todo!()
}

/// Takes a `Receiver<Option<Event>>` and implements the logic associated with each event.
/// The `broker()` function starts a connection to the database, and holds client addresses in a `HashMap`.
/// Whenever a response needs to be sent back to the client, a new write task will be generated.
async fn broker(
    mut receiver: Receiver<Option<Event>>,
    db_dir: String,
    db_file_name: String,
    epic_dir: String,
    channel_buf_size: usize,
) -> Result<(), DbError> {
    let mut db_handle = AsyncDbState::load(db_dir, db_file_name, epic_dir)?;
    let mut clients: HashMap<String, Sender<Response>> = HashMap::new();

    while let Some(event) = receiver.next().await {
        // Process each event received
        match event {
            Some(Event::NewClient { peer_addr, stream }) => {
                if !clients.contains_key(&peer_addr) {
                    let (client_sender, client_receiver) = mpsc::channel::<Response>(channel_buf_size);
                    clients.insert(peer_addr, client_sender);
                    task::spawn(connection_write_loop(stream, client_receiver));
                } else {
                    let mut client_sender = clients.get_mut(&peer_addr).unwrap();
                    //TODO: handle errors with more specificity. Log the error in the case that we can't send a response to the client
                    client_sender.send(Response::ClientAlreadyExists)
                        .await
                        .map_err(|_| DbError::ConnectionError(String::from("unable to send client response")))?;
                }
            }
            _ => todo!()
        }
    }

    todo!()
}

fn main() {todo!()}