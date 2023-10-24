//! The binary that will run the server for the asynchronous database
mod models;
mod events;
mod response;
mod utils;


use async_std::{
    prelude::*,
    task,
    net::{TcpListener, ToSocketAddrs, TcpStream},
};

use futures::channel::mpsc::{self, Receiver, Sender, SendError};
// use futures::io::AsyncReadExt;
use futures::sink::SinkExt;
use uuid::Uuid;

use std::fmt::Debug;
use std::sync::Arc;
use std::convert::TryFrom;
use std::collections::HashMap;

use models::prelude::*;
use events::prelude::*;
use response::prelude::*;
mod interface;

// /// A response to an `Event` sent by a client.
// enum Response {
//     /// Response for a successful client connection, holds the database
//     /// `Epics` encoded in a `Vec<u8`
//     ClientAddedOk(Vec<u8>),
//
//     /// Response for an unsuccessful client connection
//     ClientAlreadyExists,
//
//     /// Response of a successful addition of an `Epic` to the database, holds the epic id
//     /// and the encoded database epic's in a `u32` and `Vec<u8>`
//     AddedEpicOk(u32, Vec<u8>),
//
//     /// Response for a successful deletion of an `Epic`, holds the epic id and the
//     /// encoded state of database as a `Vec<u8>`
//     DeletedEpicOk(u32, Vec<u8>),
//
//     /// Response for successful retrieval of an `Epic`, holds
//     /// the encoded data of the epic in a `Vec<u8>`
//     GetEpicOk(Vec<u8>),
//
//     /// Response for a successful status update for a particular `Epic`, holds the id of
//     /// the updated epic and its encoded data in `u32` and `Vec<u8>` respectively
//     EpicStatusUpdateOk(u32, Vec<u8>),
//
//     /// Response for an unsuccessful retrieval of an `Epic`, holds the id of the epic in a `u32`
//     EpicDoesNotExist(u32),
//
//     /// Response for a successful retrieval of a `Story`, holds the
//     /// encoded data of the story in a `Vec<u8>`
//     GetStoryOk(Vec<u8>),
//
//     /// Response for an unsuccessful retrieval of a `Story`, holds the epic id and story id
//     StoryDoesNotExist(u32, u32),
//
//     /// Response for a successful addition of a `Story`, holds the epic id,
//     /// story id and the epic encoded as (`u32`, `u32`, `Vec<u8>`)
//     AddedStoryOk(u32, u32, Vec<u8>),
//
//     /// Response for a successful deletion of a `Story`, holds the epic id,
//     /// story id and the epic encoded as (`u32`, `u32`, `Vec<u8>`)
//     DeletedStoryOk(u32, u32, Vec<u8>),
//
//     /// Response for successful update of `Story` status, holds the epic id the contains
//     /// the story, the story id and the epic encoded as (`u32`, `u32`, `Vec<u8>`)
//     StoryStatusUpdateOk(u32, u32, Vec<u8>),
//
//     /// Response for any event that was unable to be parsed correctly
//     RequestNotParsed
// }
//
// impl Response {
//     /// Method that will convert a `Response` into an encoded slice of bytes
//     fn as_bytes(&self) -> &[u8] {
//         todo!()
//     }
// }

/// Helper function that will take a future, spawn it as a new task and log any errors propagated from the spawned future.
async fn spawn_and_log_errors(f: impl Future<Output = Result<(), DbError>> + Send + 'static) -> task::JoinHandle<()> {
    task::spawn(async move {
        if let Err(e) = f.await {
            eprintln!("error: {}", e);
        }
    })
}

/// Helper function that takes a `Result` and logs the error if it occurs.
///
/// # Returns
///
/// A boolean, true if an error occurred and was logged, false otherwise.
fn log_connection_error(result: Result<(), SendError>, peer_id: Uuid) -> bool {
    match result.map_err(|_| DbError::ConnectionError(format!("unable to send response to client: {}", peer_id))) {
        Err(e) => {
            eprintln!("error: {}", e);
            true
        }
        Ok(()) => false
    }
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
    let (broker_sender, broker_receiver) = mpsc::channel::<Event>(channel_buf_size);
    let _broker_handle = task::spawn(broker(broker_receiver, db_dir, db_file_name, epic_dir, channel_buf_size));

    while let Some(stream_res) = listener.incoming().next().await {
        let stream = stream_res.map_err(|_e| DbError::ConnectionError(format!("unable to accept stream")))?;
        let _ = spawn_and_log_errors(connection_loop(stream, broker_sender.clone()));
    }

    Ok(())
}

/// Takes a `TcpStream` and a `Sender<Option<Event>>` representing the client connection and the sending
/// end of a channel connected to a broker task. Attempts to read new events from the client stream and send them
/// to the broker task. If a new event is successfully read from the client stream it is sent to the broker via `broker_sender`,
/// otherwise it sends `None`. The function can fail if there is an error parsing `client_stream.peer_addr()` as a string,
/// a new `Event` is not able to be sent to the broker.
async fn connection_loop(client_stream: TcpStream, mut broker_sender: Sender<Event>) -> Result<(), DbError> {
    let client_stream = Arc::new(client_stream);
    let mut client_stream_reader = &*client_stream;

    // TODO: set up the synchronization method signal to broker that a peer's connection has been dropped in this task

    // Create custom id for new client
    let client_id = Uuid::new_v4();

    // Create a new client and send to broker
    let new_client = Event::NewClient { peer_id: client_id.clone(), stream: client_stream.clone() };
    broker_sender.send(new_client)
        .await
        .unwrap();

    let mut tag = [0u8; 13];

    while let Ok(_) = client_stream_reader.read_exact(&mut tag).await {
        match Event::try_create(client_id.clone(), &tag, client_stream_reader).await {
            Ok(event) => {
                broker_sender
                    .send(event)
                    .await
                    .unwrap();
            }
            // We were unable to parse a valid event from the clients stream,
            Err(_) => {
                broker_sender
                    .send(Event::UnparseableEvent { peer_id: client_id.clone() })
                    .await
                    .unwrap();
            }
        }
    }
    // TODO: Handle event when client disconnects, need to set up a synchornization method
    Ok(())
}

/// Takes `stream` and `client_receiver` and writes all responses received from the broker task to
/// `stream`.
async fn connection_write_loop(stream: Arc<TcpStream>, mut client_receiver: Receiver<Response>) -> Result<(), DbError> {
    let mut stream = &*stream;
    while let Some(resp) = client_receiver.next().await {
        stream.write_all(resp.as_bytes().as_slice())
            .await
            .map_err(|_| DbError::ConnectionError(format!("unable to send response to client")))?;
    }
    Ok(())
}

/// Takes a `Receiver<Option<Event>>` and implements the logic associated with each event.
/// The `broker()` function starts a connection to the database, and holds client addresses in a `HashMap`.
/// Whenever a response needs to be sent back to the client, a new write task will be generated.
async fn broker(
    mut receiver: Receiver<Event>,
    db_dir: String,
    db_file_name: String,
    epic_dir: String,
    channel_buf_size: usize,
) -> Result<(), DbError> {
    let mut db_handle = AsyncDbState::load(db_dir, db_file_name, epic_dir)?;
    let mut clients: HashMap<Uuid, Sender<Response>> = HashMap::new();
    while let Some(event) = receiver.next().await {
        // Process each event received
        match event {
            Event::NewClient { peer_id, stream } => {
                if !clients.contains_key(&peer_id) {
                    let (mut client_sender, client_receiver) = mpsc::channel::<Response>(channel_buf_size);
                    task::spawn(connection_write_loop(stream, client_receiver));
                    if !log_connection_error(client_sender.send(Response::ClientAddedOk(db_handle.as_bytes())).await, peer_id) {
                        clients.insert(peer_id, client_sender);
                    }
                } else {
                    let mut client_sender = clients.get_mut(&peer_id).unwrap();
                    let _ = log_connection_error(client_sender.send(Response::ClientAlreadyExists).await, peer_id);
                }
            }
            Event::AddEpic { peer_id, epic_name, epic_description } => {
                let epic_id = db_handle.add_epic(epic_name, epic_description);
                let mut client_sender = clients.get_mut(&peer_id).expect("client should exist");
                let _ = log_connection_error(client_sender.send(Response::AddedEpicOk(epic_id, db_handle.as_bytes())).await, peer_id);
                // Ensure changes persist in database
                db_handle.write().await?;
            }
            Event::DeleteEpic { peer_id, epic_id} => {
                let mut client_sender = clients.get_mut(&peer_id).expect("client should exist");
                // Ensure epic_id is a valid epic
                match db_handle.delete_epic(epic_id) {
                    // We have successfully removed the epic
                    Ok(_epic) => {
                        let _ = log_connection_error(client_sender.send(Response::DeletedEpicOk(epic_id, db_handle.as_bytes())).await, peer_id);
                        // Ensure changes persist in the database
                        db_handle.write().await?;
                    }
                    // The epic does not exist, send reply back to client
                    Err(_e) => {
                        let _ = log_connection_error(client_sender.send(Response::EpicDoesNotExist(epic_id, db_handle.as_bytes())).await, peer_id);
                    }
                }
            }
            Event::GetEpic { peer_id, epic_id } => {
                let mut client_sender = clients.get_mut(&peer_id).expect("client should exist");
                match db_handle.get_epic(epic_id) {
                    Some(epic) => {
                        let epic_bytes = epic.as_bytes();
                        let _ = log_connection_error(client_sender.send(Response::GetEpicOk(epic_bytes)).await, peer_id);
                    }
                    None => {
                        let _ = log_connection_error(client_sender.send(Response::EpicDoesNotExist(epic_id, db_handle.as_bytes())).await, peer_id);
                    }
                }
            }
            Event::UpdateEpicStatus { peer_id, epic_id, status}  => {
                let mut client_sender = clients.get_mut(&peer_id).expect("client should exist");
                if let Some(epic) = db_handle.get_epic_mut(epic_id) {
                    epic.update_status(status);
                    // Send response that status was updated successfully
                    let _ = log_connection_error(client_sender.send(Response::EpicStatusUpdateOk(epic_id, epic.as_bytes())).await, peer_id);
                    epic.write_async().await?;
                } else {
                    let _ = log_connection_error(client_sender.send(Response::EpicDoesNotExist(epic_id, db_handle.as_bytes())).await, peer_id);
                }
            }
            Event::GetStory { peer_id,epic_id, story_id} => {
                let mut client_sender = clients.get_mut(&peer_id).expect("client should exist");
                if let Some(epic) = db_handle.get_epic(epic_id) {
                    match epic.get_story(story_id) {
                        Some(story) => {
                            let _ = log_connection_error(client_sender.send(Response::GetStoryOk(story.as_bytes())).await, peer_id);
                        }
                        None => {
                            let _ = log_connection_error(client_sender.send(Response::StoryDoesNotExist(epic_id, story_id, epic.as_bytes())).await, peer_id);
                        }
                    }
                } else {
                    let _ = log_connection_error(client_sender.send(Response::EpicDoesNotExist(epic_id, db_handle.as_bytes())).await, peer_id);
                }
            }
            Event::AddStory { peer_id, epic_id, story_name, story_description} => {
                let mut client_sender = clients.get_mut(&peer_id).expect("client should exist");
                match db_handle.add_story(epic_id, story_name, story_description).await {
                    Ok(story_id) => {
                        let _ = log_connection_error(client_sender.send(Response::AddedStoryOk(epic_id, story_id, db_handle.get_epic(epic_id).unwrap().as_bytes())).await, peer_id);
                        // Write new story to database
                        db_handle.get_epic_mut(epic_id).unwrap().write_async().await?;
                    }
                    Err(e) => {
                        let _ = log_connection_error(client_sender.send(Response::EpicDoesNotExist(epic_id, db_handle.as_bytes())).await, peer_id);
                    }
                }
            }
            Event::DeleteStory {peer_id, epic_id, story_id} => {
                let mut client_sender = clients.get_mut(&peer_id).expect("client should exist");
                if let Some(epic) = db_handle.get_epic_mut(epic_id) {
                    match epic.delete_story(story_id) {
                        Ok(_) => {
                            let _ = log_connection_error(client_sender.send(Response::DeletedStoryOk(epic_id, story_id, epic.as_bytes())).await, peer_id);
                            // Write changes to database
                            epic.write_async().await?;
                        }
                        Err(_) => {
                            let _ = log_connection_error(client_sender.send(Response::StoryDoesNotExist(epic_id, story_id, epic.as_bytes())).await, peer_id);
                        }
                    }
                } else {
                    let _ = log_connection_error(client_sender.send(Response::EpicDoesNotExist(epic_id, db_handle.as_bytes())).await, peer_id);
                }
            }
            Event::UpdateStoryStatus { peer_id, epic_id, story_id, status} => {
                let mut client_sender = clients.get_mut(&peer_id).expect("client should exist");
                if let Some(epic) = db_handle.get_epic_mut(epic_id) {
                    match epic.update_story_status(story_id, status) {
                        Ok(_) => {
                            let _ = log_connection_error(client_sender.send(Response::StoryStatusUpdateOk(epic_id, story_id, epic.as_bytes())).await, peer_id);
                            // Write changes to database
                            epic.write_async().await?;
                        }
                        Err(_) => {
                            let _ = log_connection_error(client_sender.send(Response::StoryDoesNotExist(epic_id, story_id, epic.as_bytes())).await, peer_id);
                        }
                    }
                } else {
                    let _ = log_connection_error(client_sender.send(Response::EpicDoesNotExist(epic_id, db_handle.as_bytes())).await, peer_id);
                }
            }
            Event::UnparseableEvent { peer_id } => {
                let mut client_sender = clients.get_mut(&peer_id).expect("client should exist");
                let _ = log_connection_error(client_sender.send(Response::RequestNotParsed).await, peer_id);
            }
        }
    }
    // TODO: handle gracefull shutdown, ensure writing tasks get complete etc...
    todo!()
}

fn main() {todo!()}