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

use futures::channel::mpsc::{self, Receiver, Sender, SendError, UnboundedReceiver};
use futures::select;
use futures::sink::SinkExt;
use futures::{StreamExt, Stream, FutureExt};
use uuid::Uuid;

use std::fmt::Debug;
use std::sync::Arc;
use std::convert::TryFrom;
use std::collections::HashMap;
use std::thread::spawn;

use models::prelude::*;
use events::prelude::*;
use response::prelude::*;
mod interface;


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
    let broker_handle = task::spawn(broker(broker_receiver, db_dir, db_file_name, epic_dir, channel_buf_size));

    while let Some(stream_res) = listener.incoming().next().await {
        let stream = stream_res.map_err(|_e| DbError::ConnectionError(format!("unable to accept stream")))?;
        let _ = spawn_and_log_errors(connection_loop(stream, broker_sender.clone()));
    }
    // Drop the broker's sender
    drop(broker_sender);
    // Await the result from the broker's task
    broker_handle.await?;
    Ok(())
}

enum Void {}

/// Takes a `TcpStream` and a `Sender<Option<Event>>` representing the client connection and the sending
/// end of a channel connected to a broker task. Attempts to read new events from the client stream and send them
/// to the broker task. If a new event is successfully read from the client stream it is sent to the broker via `broker_sender`,
/// otherwise it sends `None`. The function can fail if there is an error parsing `client_stream.peer_addr()` as a string,
/// a new `Event` is not able to be sent to the broker.
async fn connection_loop(client_stream: TcpStream, mut broker_sender: Sender<Event>) -> Result<(), DbError> {
    let client_stream = Arc::new(client_stream);
    let mut client_stream_reader = &*client_stream;

    // TODO: set up the synchronization method signal to broker that a peer's connection has been dropped in this task
    let (_shutdown_sender, shutdown_receiver) = mpsc::unbounded::<Void>();

    // Create custom id for new client
    let client_id = Uuid::new_v4();

    // Create a new client and send to broker
    let new_client = Event::NewClient { peer_id: client_id.clone(), stream: client_stream.clone(), shutdown: shutdown_receiver };
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

    Ok(())
}

/// Takes `stream` and `client_receiver` and writes all responses received from the broker task to
/// `stream`.
async fn connection_write_loop(stream: Arc<TcpStream>, client_receiver: &mut Receiver<Response>, client_shutdown: UnboundedReceiver<Void>) -> Result<(), DbError> {
    let mut stream = &*stream;
    let mut client_receiver = client_receiver.fuse();
    let mut client_shutdown = client_shutdown.fuse();
    loop {
        select! {
            response = client_receiver.next().fuse() => match response {
                Some(resp) => {
                    stream.write_all(resp.as_bytes().as_slice())
                            .await
                            .map_err(|_| DbError::ConnectionError(format!("unable to send response to client")))?
                }
                None => break
            },
            void = client_shutdown.next().fuse() => match void {
                Some(void) => {}
                None => break,
            }
        }
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
    // For reaping disconnected peers
    let (disconnect_sender, disconnect_receiver) = mpsc::unbounded::<(Uuid, Receiver<Response>)>();

    // For managing the state of the database
    let mut db_handle = AsyncDbState::load(db_dir, db_file_name, epic_dir)?;

    // Holds clients currently connected to the server
    let mut clients: HashMap<Uuid, Sender<Response>> = HashMap::new();

    let mut events = receiver.fuse();
    let mut disconnect_receiver = disconnect_receiver.fuse();

    loop {
        // Attempt to read an event or disconnect a peer
        let event = select! {
            event = events.next().fuse() => match event {
                Some(event) => event,
                None => break,
            },
            disconnect = disconnect_receiver.next().fuse() => match disconnect {
                Some((peer_id, _client_receiver)) => {
                    assert!(clients.remove(&peer_id).is_some());
                    continue;
                }
                None => break,
            },
        };

        // Process each event received
        match event {
            // New client, ensure client has not already been added, and start a new write process
            // that will write responses to the write-half of the clients TcpStream. Instantiate
            // client_sender/client_receiver sides of a channel for sending/receiving responses from
            // the broker. Clone the disconnect_sender so that when the client process is finished,
            // the broker may be alerted.
            Event::NewClient { peer_id, stream, shutdown } => {
                if !clients.contains_key(&peer_id) {
                    let (mut client_sender, mut client_receiver) = mpsc::channel::<Response>(channel_buf_size);
                    clients.insert(peer_id, client_sender.clone());
                    let mut disconnect_sender = disconnect_sender.clone();
                    let _ = spawn_and_log_errors(
                        async move {
                            let res = connection_write_loop(stream, &mut client_receiver, shutdown).await;
                            let _ = disconnect_sender.send((peer_id, client_receiver)).await;
                            res
                        }
                    );
                    let _ = log_connection_error(client_sender.send(Response::ClientAddedOk(db_handle.as_bytes())).await, peer_id);
                } else {
                    // let mut client_sender = clients.get_mut(&peer_id).unwrap();
                    // let _ = log_connection_error(client_sender.send(Response::ClientAlreadyExists).await, peer_id);
                    // TODO: Log errors, instead of writing to stderr
                    eprintln!("error: client already exists");
                }
            }
            // Adds an epic to the db_handle, and writes it to the database
            Event::AddEpic { peer_id, epic_name, epic_description } => {
                let epic_id = db_handle.add_epic(epic_name, epic_description);
                let mut client_sender = clients.get_mut(&peer_id).expect("client should exist");
                // Send response to client
                let _ = log_connection_error(client_sender.send(Response::AddedEpicOk(epic_id, db_handle.as_bytes())).await, peer_id);
                // Ensure changes persist in database
                db_handle.write().await?;
            }
            // Deletes an epic from the db_handle, writes changes to the database
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
            // Gets an epics information from the database
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
            // Update the status of an epic in the database
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
            // Gets a story from the database
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
            // Adds a story to the the current epic, writes changes to the epic's file
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
            // Deletes a story from the current epic, writes changes to the epic's file
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
            // Updates the status of a story, writes changes to the epic's file
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
            // The event was unable to be parsed, send a response informing the client
            Event::UnparseableEvent { peer_id } => {
                let mut client_sender = clients.get_mut(&peer_id).expect("client should exist");
                let _ = log_connection_error(client_sender.send(Response::RequestNotParsed).await, peer_id);
            }
        }
    }
    // Drop clients so that writers will finish
    drop(clients);
    // Drop disconnect_sender, and drain the rest of the disconnected peers
    drop(disconnect_sender);
    while let Some((_peer_id, _client_receiver)) = disconnect_receiver.next().await {}
    Ok(())
}

fn main() {todo!()}