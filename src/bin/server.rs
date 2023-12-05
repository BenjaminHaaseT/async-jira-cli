//! The binary that will run the server for the asynchronous database

use std::collections::HashMap;
use std::convert::TryFrom;
use std::fmt::Debug;
use std::sync::Arc;
use async_std::{
    net::{TcpListener, TcpStream, ToSocketAddrs},
    prelude::*,
    task,
};
use futures::channel::mpsc::{self, Receiver, SendError, Sender, UnboundedReceiver};
use futures::select;
use futures::sink::SinkExt;
use futures::{FutureExt, Stream, StreamExt};
use uuid::Uuid;
use clap::Parser;
use futures::stream::FusedStream;
use tracing::{instrument, Level, event};


use async_jira_cli::events::prelude::*;
use async_jira_cli::models::prelude::*;
use async_jira_cli::response::prelude::*;
use async_jira_cli::utils::{AsBytes, Void};

/// Helper function that will take a future, spawn it as a new task and log any errors propagated from the spawned future.
async fn spawn_and_log_errors(
    f: impl Future<Output = Result<(), DbError>> + Send + 'static,
) -> task::JoinHandle<()> {
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
    match result.map_err(|_| {
        DbError::ConnectionError(format!("unable to send response to client: {}", peer_id))
    }) {
        Err(e) => {
            eprintln!("error: {}", e);
            true
        }
        Ok(()) => false,
    }
}

/// Loop that accepts incoming client connections.
/// Accepts an address, `addrs` representing a socket address that will listen for incoming connections,
/// and a `channel_buf_size` representing the capacity of channel that connects to the broker task.
/// The function will start a new listener awaiting for incoming connections from clients, it then starts
/// a new broker task, and then passes each client connection to a separate connection task.
///
/// # Returns
///
/// A `Result<(), DbError>`, the `Ok` variant if no errors occurred, otherwise `Err`.
#[instrument(ret, err)]
async fn accept_loop(
    addrs: impl ToSocketAddrs + Debug + Clone,
    channel_buf_size: usize,
    db_dir: String,
    db_file_name: String,
    epic_dir: String,
) -> Result<(), DbError> {
    // Connect to the servers socket address
    println!("listening at {:?}...", addrs);
    event!(Level::DEBUG, address = ?addrs, "listening at {:?}", addrs);

    let addrs_clone = addrs.clone();
    let listener = TcpListener::bind(addrs.clone()).await.map_err(|_e| {
        DbError::ConnectionError(format!("could not connect to {:?}", addrs_clone))
    })?;

    event!(Level::INFO, address = ?addrs, "successfully bound listener to address");
    event!(Level::INFO, "spawning broker task");
    // Get a channel to the broker, and spawn the brokers task
    let (broker_sender, broker_receiver) = mpsc::channel::<Event>(channel_buf_size);
    let broker_handle = task::spawn(broker(
        broker_receiver,
        db_dir,
        db_file_name,
        epic_dir,
        channel_buf_size,
    ));
    event!(Level::INFO, "accepting incoming connections");
    while let Some(stream_res) = listener.incoming().next().await {
        event!(Level::INFO, stream = ?stream_res, "accepting stream");
        let stream = stream_res
            .map_err(|_e| DbError::ConnectionError(String::from("unable accept incoming client stream")))?;
        event!(Level::INFO, stream = ?stream, "spawning connection loop for stream");
        let broker_sender_clone = broker_sender.clone();
        task::spawn(async move {
            if let Err(e) = connection_loop(stream, broker_sender_clone).await {
                event!(Level::ERROR, error = %e, "error from connection loop task");
            }
        });
    }
    event!(Level::INFO, "initiating graceful shutdown");
    // Drop the broker's sender
    drop(broker_sender);
    // Await the result from the broker's task
    broker_handle.await?;
    event!(Level::INFO, "graceful shutdown achieved without errors");
    Ok(())
}

/// Takes a `TcpStream` and a `Sender<Option<Event>>` representing the client connection and the sending
/// end of a channel connected to the broker task. Attempts to read new events from the client stream and send them
/// to the broker task. If a new event is successfully read from the client stream it is sent to the broker via `broker_sender`,
/// otherwise it sends `Event::UnparseableEvent`. The function can fail if there is an error parsing `client_stream.peer_addr()` as a string or
/// a new `Event` is not able to be sent to the broker.
///
/// # Returns
///
/// A `Result<(), DbError>`, the `Ok` variant if successful, otherwise `Err`.
#[instrument(ret, err)]
async fn connection_loop(
    client_stream: TcpStream,
    mut broker_sender: Sender<Event>,
) -> Result<(), DbError> {
    event!(Level::DEBUG, client_stream = ?client_stream, "started connection loop task for client");
    let client_stream = Arc::new(client_stream);
    let mut client_stream_reader = &*client_stream;

    let (_shutdown_sender, shutdown_receiver) = mpsc::unbounded::<Void>();

    // Create custom id for new client
    let client_id = Uuid::new_v4();
    event!(Level::INFO, client_stream = ?client_stream, client_id = ?client_id, "client connection received id");

    // Create a new client and send to broker
    let new_client = Event::NewClient {
        peer_id: client_id.clone(),
        stream: client_stream.clone(),
        shutdown: shutdown_receiver,
    };

    broker_sender.send(new_client)
        .await
        .map_err(|_e| DbError::ConnectionError(format!("client {:?} unable to send event to broker task, broker task should be connected", client_id)))?;

    event!(Level::INFO, client_id = ?client_id, "sent new client event to broker task");

    let mut tag = [0u8; 13];

    while let Ok(_) = client_stream_reader.read_exact(&mut tag).await {
        match Event::try_create(client_id.clone(), &tag, client_stream_reader).await {
            Ok(event) => {

                broker_sender.send(event)
                    .await
                    .map_err(|_e| DbError::ConnectionError(format!("client {:?} unable to send event to broker task, broker task should be connected", client_id)))?;
                event!(Level::INFO, client_id = ?client_id, "sent event to client");
            }
            // We were unable to parse a valid event from the clients stream,
            Err(e) => {
                event!(Level::ERROR, error = %e, client_id = ?client_id, "unable to parse event from client");
                broker_sender
                    .send(Event::UnparseableEvent {
                        peer_id: client_id.clone(),
                    })
                    .await
                    .map_err(|_e| DbError::ConnectionError(format!("client {:?} unable to send event to broker task, broker task should be connected", client_id)))?;
            }
        }
    }

    Ok(())
}

/// Takes `stream` and `client_receiver` and writes all responses received from the broker task to
/// `stream`. In the case that the client is disconnected, this task is notified via `client_shutdown`,
/// in which case the loop terminates and the task finishes.
///
/// # Returns
///
/// A `Result<(), DbError>`, the `Ok` variant if successful, otherwise the `Err` variant.
#[instrument(ret, err)]
async fn connection_write_loop(
    stream: Arc<TcpStream>,
    client_receiver: &mut Receiver<Response>,
    client_shutdown: UnboundedReceiver<Void>,
    peer_id: Uuid,
) -> Result<(), DbError> {
    let mut stream = &*stream;
    let mut client_receiver = client_receiver.fuse();
    let mut client_shutdown = client_shutdown.fuse();
    loop {
        select! {
            response = client_receiver.next().fuse() => match response {
                Some(resp) => {
                    event!(Level::INFO, response = ?resp, peer_id = ?peer_id, "client {} received response from broker", peer_id);
                    stream.write_all(resp.as_bytes().as_slice())
                            .await
                            .map_err(|_| DbError::ConnectionError(format!("unable to send response to client {}", peer_id)))?;
                }
                None => break
            },
            void = client_shutdown.next().fuse() => match void {

                Some(void) => {}
                None => {
                    event!(Level::INFO, peer_id = ?peer_id, "client {} write task received shutdown signal", peer_id);
                    break;
                },
            }
        }
    }
    event!(Level::INFO, peer_id = ?peer_id, "client {} write task shutting down", peer_id);
    Ok(())
}

/// The main workhorse of the server. Starts a new database connection using `db_dir`, `db_file_name`
/// and `epic_dir`, receives and processes events using `receiver` and manages disconnecting clients.
///
/// # Returns
///
/// A `Result<(), DbError>`, the `Ok` variant if successful, otherwise the `Err` variant.
#[instrument(ret, err)]
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
    let mut db_handle = DbState::load(db_dir, db_file_name, epic_dir).await?;

    // Holds clients currently connected to the server
    let mut clients: HashMap<Uuid, Sender<Response>> = HashMap::new();

    let mut events = receiver.fuse();
    let mut disconnect_receiver = disconnect_receiver.fuse();

    loop {
        // Attempt to read an event or disconnect a peer
        let event = select! {
            event = events.next().fuse() => match event {
                Some(event) => {
                    event!(Level::INFO, event = ?event, "broker task received event");
                    event
                },
                None => {
                    event!(Level::INFO, "broker task received shutdown signal from events channel");
                    break;
                },
            },
            disconnect = disconnect_receiver.next().fuse() => match disconnect {
                Some((peer_id, _client_receiver)) => {
                    let removed_client_sender = clients
                        .remove(&peer_id)
                        .ok_or(DbError::DoesNotExist(format!("client with id {} should exist in clients map", peer_id)))?;
                    event!(Level::INFO, peer_id = ?peer_id, "removed client {} successfully from client map", peer_id);
                    continue;
                }
                None => {
                    event!(Level::WARN, "broker task received shutdown signal from client disconnection channel");
                    break;
                },
            },
        };

        // Process each event received
        match event {
            // New client, ensure client has not already been added, and start a new write process
            // that will write responses to the write-half of the clients TcpStream. Instantiate
            // client_sender/client_receiver sides of a channel for sending/receiving responses from
            // the broker. Clone the disconnect_sender so that when the client process is finished,
            // the broker may be alerted.
            Event::NewClient {
                peer_id,
                stream,
                shutdown,
            } => {
                event!(Level::INFO, peer_id = ?peer_id, "received 'NewClient' event from client {}", peer_id);
                handlers::handle_new_client(peer_id, stream, shutdown, &mut clients, &disconnect_sender, channel_buf_size, &mut db_handle).await?;
            }
            // Adds an epic to the db_handle, and writes it to the database
            Event::AddEpic {
                peer_id,
                epic_name,
                epic_description,
            } => {
                event!(Level::INFO, peer_id = ?peer_id, epic_name, epic_description, "received 'AddEpic' event from client {}", peer_id);
                handlers::add_epic_handler(peer_id, epic_name, epic_description, &mut clients, &mut db_handle).await?;
            }
            // Deletes an epic from the db_handle, writes changes to the database
            Event::DeleteEpic { peer_id, epic_id } => {
                event!(Level::INFO, peer_id = ?peer_id, epic_id, "received 'DeleteEpic' event from client {}", peer_id);
                handlers::delete_epic_handler(peer_id, epic_id, &mut clients, &mut db_handle).await?;
            }
            // Gets an epics information from the database
            Event::GetEpic { peer_id, epic_id } => {
                event!(Level::INFO, peer_id = ?peer_id, epic_id, "received 'GetEpic' event from client {}", peer_id);
                handlers::get_epic_handler(peer_id, epic_id, &mut clients, &mut db_handle).await?;
            }
            // Update the status of an epic in the database
            Event::UpdateEpicStatus {
                peer_id,
                epic_id,
                status,
            } => {
                event!(Level::INFO, peer_id = ?peer_id, epic_id, status = ?status, "received 'UpdateEpicStatus' event from client {}", peer_id);
                handlers::update_epic_status_handler(peer_id, epic_id, status, &mut clients, &mut db_handle).await?;
            }
            // Gets a story from the database
            Event::GetStory {
                peer_id,
                epic_id,
                story_id,
            } => {
                event!(Level::INFO, peer_id = ?peer_id, epic_id, story_id, "received 'GetStory' event from client {}", peer_id);
                handlers::get_story_handler(peer_id, epic_id, story_id, &mut clients, &mut db_handle).await?;
            }
            // Adds a story to the the current epic, writes changes to the epic's file
            Event::AddStory {
                peer_id,
                epic_id,
                story_name,
                story_description,
            } => {
                event!(Level::INFO, peer_id = ?peer_id, epic_id, story_name, story_description, "received 'AddStory' event from client {}", peer_id);
                handlers::add_story_handler(peer_id, epic_id, story_name, story_description, &mut clients, &mut db_handle).await?;
            }
            // Deletes a story from the current epic, writes changes to the epic's file
            Event::DeleteStory {
                peer_id,
                epic_id,
                story_id,
            } => {
                event!(Level::INFO, peer_id = ?peer_id, epic_id, story_id, "received 'DeleteStory' event from client {}", peer_id);
                handlers::delete_story_handler(peer_id, epic_id, story_id, &mut clients, &mut db_handle).await?;
            }
            // Updates the status of a story, writes changes to the epic's file
            Event::UpdateStoryStatus {
                peer_id,
                epic_id,
                story_id,
                status,
            } => {
                event!(Level::INFO, peer_id = ?peer_id, epic_id, story_id, status = ?status, "received 'UpdateStoryStatus from client {}", peer_id);
                handlers::update_story_status(peer_id, epic_id, story_id, status, &mut clients, &mut db_handle).await?;
            }
            // The event was unable to be parsed, send a response informing the client
            Event::UnparseableEvent { peer_id } => {
                event!(Level::WARN, peer_id = ?peer_id, "received 'UnparseableEvent' from client {}", peer_id);
                handlers::unparseable_event_handler(peer_id, &mut clients).await?;
            }
        }
    }

    // Drop clients so that writers will finish
    event!(Level::INFO, clients = ?clients, "completed brokers select loop, dropping remaining client channels");
    drop(clients);

    // Drop disconnect_sender, and drain the rest of the disconnected peers
    event!(Level::INFO, "dropping client disconnection channel");
    drop(disconnect_sender);

    event!(Level::INFO, "draining remaining client write tasks");
    while let Some((_peer_id, _client_receiver)) = disconnect_receiver.next().await {}

    Ok(())
}

/// A private helper module for the server binary. Breaks up handling logic executed by the broker
/// into separate functions for the purpose of brevity.
mod handlers {
    use futures::channel::mpsc::UnboundedSender;
    use super::*;

    /// Handles the event of a new client connecting to the database server.
    ///
    /// Adds the client to `clients` and starts a new writing task so that the broker can send responses
    /// to the new client. This function is also responsible for setting up the synchronization mechanism that
    /// manages the clients disconnection.
    ///
    /// # Returns
    ///
    /// A `Result<(), DbError>`, the `Ok` variant if successful, otherwise the `Err` variant.
    pub async fn handle_new_client(
        peer_id: Uuid,
        stream: Arc<TcpStream>,
        shutdown: UnboundedReceiver<Void>,
        clients: &mut HashMap<Uuid, Sender<Response>>,
        disconnect_sender: &UnboundedSender<(Uuid, Receiver<Response>)>,
        channel_buf_size: usize,
        db_handle: &mut DbState,
    ) -> Result<(), DbError> {
        if !clients.contains_key(&peer_id) {
            let (mut client_sender, mut client_receiver) =
                mpsc::channel::<Response>(channel_buf_size);

            clients.insert(peer_id, client_sender.clone());
            let mut disconnect_sender = disconnect_sender.clone();

            let _ = task::spawn(async move {
                let res = connection_write_loop(stream, &mut client_receiver, shutdown, peer_id).await;
                let _ = disconnect_sender.send((peer_id, client_receiver)).await;
                if let Err(e) = res {
                    eprintln!("{e}");
                    Err(e)
                } else {
                    Ok(())
                }
            });

            let _ = log_connection_error(
                client_sender
                    .send(Response::ClientAddedOk(db_handle.as_bytes()))
                    .await,
                peer_id,
            );
        } else {
            let mut client_sender = clients.get_mut(&peer_id).unwrap();
            let _ = log_connection_error(client_sender.send(Response::ClientAlreadyExists).await, peer_id);
            // TODO: Log errors, instead of writing to stderr
            eprintln!("error: client already exists");
        }
        Ok(())
    }

    pub async fn add_epic_handler(peer_id: Uuid, epic_name: String, epic_description: String, clients: &mut HashMap<Uuid, Sender<Response>>, db_handle: &mut DbState) -> Result<(), DbError> {
        let epic_id = db_handle.add_epic(epic_name, epic_description);
        let mut client_sender = clients.get_mut(&peer_id).expect("client should exist");
        // Send response to client
        let _ = log_connection_error(
            client_sender
                .send(Response::AddedEpicOk(epic_id, db_handle.as_bytes()))
                .await,
            peer_id,
        );
        // Ensure changes persist in database
        db_handle.write_async().await
    }

    pub async fn delete_epic_handler(peer_id: Uuid, epic_id: u32, clients: &mut HashMap<Uuid, Sender<Response>>, db_handle: &mut DbState) -> Result<(), DbError> {
        let mut client_sender = clients.get_mut(&peer_id).expect("client should exist");
        // Ensure epic_id is a valid epic
        match db_handle.delete_epic(epic_id) {
            // We have successfully removed the epic
            Ok(_epic) => {
                let _ = log_connection_error(
                    client_sender
                        .send(Response::DeletedEpicOk(epic_id, db_handle.as_bytes()))
                        .await,
                    peer_id,
                );
                // Ensure changes persist in the database
                db_handle.write_async().await?;
            }
            // The epic does not exist, send reply back to client
            Err(_e) => {
                let _ = log_connection_error(
                    client_sender
                        .send(Response::EpicDoesNotExist(epic_id, db_handle.as_bytes()))
                        .await,
                    peer_id,
                );
            }
        }
        Ok(())
    }

    pub async fn get_epic_handler(peer_id: Uuid, epic_id: u32, clients: &mut HashMap<Uuid, Sender<Response>>, db_handle: &mut DbState) -> Result<(), DbError> {
        let mut client_sender = clients.get_mut(&peer_id).expect("client should exist");
        match db_handle.get_epic(epic_id) {
            Some(epic) => {
                let epic_bytes = epic.as_bytes();
                let _ = log_connection_error(
                    client_sender.send(Response::GetEpicOk(epic_id,epic_bytes)).await,
                    peer_id,
                );
            }
            None => {
                let _ = log_connection_error(
                    client_sender
                        .send(Response::EpicDoesNotExist(epic_id, db_handle.as_bytes()))
                        .await,
                    peer_id,
                );
            }
        }
        Ok(())
    }

    pub async fn update_epic_status_handler(peer_id: Uuid, epic_id: u32, status: Status, clients: &mut HashMap<Uuid, Sender<Response>>, db_handle: &mut DbState) -> Result<(), DbError> {
        let mut client_sender = clients.get_mut(&peer_id).expect("client should exist");
        if let Some(epic) = db_handle.get_epic_mut(epic_id) {
            epic.update_status(status);
            // Send response that status was updated successfully
            let _ = log_connection_error(
                client_sender
                    .send(Response::EpicStatusUpdateOk(epic_id, epic.as_bytes()))
                    .await,
                peer_id,
            );
            epic.write_async().await?;
        } else {
            let _ = log_connection_error(
                client_sender
                    .send(Response::EpicDoesNotExist(epic_id, db_handle.as_bytes()))
                    .await,
                peer_id,
            );
        }
        Ok(())
    }

    pub async fn get_story_handler(peer_id: Uuid, epic_id: u32, story_id: u32, clients: &mut HashMap<Uuid, Sender<Response>>, db_handle: &mut DbState) -> Result<(), DbError> {
        let mut client_sender = clients.get_mut(&peer_id).expect("client should exist");
        if let Some(epic) = db_handle.get_epic(epic_id) {
            match epic.get_story(story_id) {
                Some(story) => {
                    let _ = log_connection_error(
                        client_sender
                            .send(Response::GetStoryOk(story_id, story.as_bytes()))
                            .await,
                        peer_id,
                    );
                }
                None => {
                    let _ = log_connection_error(
                        client_sender
                            .send(Response::StoryDoesNotExist(
                                epic_id,
                                story_id,
                                epic.as_bytes(),
                            ))
                            .await,
                        peer_id,
                    );
                }
            }
        } else {
            let _ = log_connection_error(
                client_sender
                    .send(Response::EpicDoesNotExist(epic_id, db_handle.as_bytes()))
                    .await,
                peer_id,
            );
        }
        Ok(())
    }

    pub async fn add_story_handler(peer_id: Uuid, epic_id: u32, story_name: String, story_description: String, clients: &mut HashMap<Uuid, Sender<Response>>, db_handle: &mut DbState) -> Result<(), DbError> {
        let mut client_sender = clients.get_mut(&peer_id).expect("client should exist");
        match db_handle
            .add_story_async(epic_id, story_name, story_description)
            .await
        {
            Ok(story_id) => {
                let _ = log_connection_error(
                    client_sender
                        .send(Response::AddedStoryOk(
                            epic_id,
                            story_id,
                            db_handle.get_epic(epic_id).unwrap().as_bytes(),
                        ))
                        .await,
                    peer_id,
                );
                // Write new story to database
                db_handle
                    .get_epic_mut(epic_id)
                    .unwrap()
                    .write_async()
                    .await?;
            }
            Err(e) => {
                let _ = log_connection_error(
                    client_sender
                        .send(Response::EpicDoesNotExist(epic_id, db_handle.as_bytes()))
                        .await,
                    peer_id,
                );
            }
        }
        Ok(())
    }

    pub async fn delete_story_handler(peer_id: Uuid, epic_id: u32, story_id: u32, clients: &mut HashMap<Uuid, Sender<Response>>, db_handle: &mut DbState) -> Result<(), DbError> {
        let mut client_sender = clients.get_mut(&peer_id).expect("client should exist");
        if let Some(epic) = db_handle.get_epic_mut(epic_id) {
            match epic.delete_story(story_id) {
                Ok(_) => {
                    let _ = log_connection_error(
                        client_sender
                            .send(Response::DeletedStoryOk(
                                epic_id,
                                story_id,
                                epic.as_bytes(),
                            ))
                            .await,
                        peer_id,
                    );
                    // Write changes to database
                    epic.write_async().await?;
                }
                Err(_) => {
                    let _ = log_connection_error(
                        client_sender
                            .send(Response::StoryDoesNotExist(
                                epic_id,
                                story_id,
                                epic.as_bytes(),
                            ))
                            .await,
                        peer_id,
                    );
                }
            }
        } else {
            let _ = log_connection_error(
                client_sender
                    .send(Response::EpicDoesNotExist(epic_id, db_handle.as_bytes()))
                    .await,
                peer_id,
            );
        }
        Ok(())
    }

    pub async fn update_story_status(peer_id: Uuid, epic_id: u32, story_id: u32, status: Status, clients: &mut HashMap<Uuid, Sender<Response>>, db_handle: &mut DbState) -> Result<(), DbError> {
        let mut client_sender = clients.get_mut(&peer_id).expect("client should exist");
        if let Some(epic) = db_handle.get_epic_mut(epic_id) {
            match epic.update_story_status(story_id, status) {
                Ok(_) => {
                    let _ = log_connection_error(
                        client_sender
                            .send(Response::StoryStatusUpdateOk(
                                epic_id,
                                story_id,
                                epic.get_story(story_id).unwrap().as_bytes(),
                            ))
                            .await,
                        peer_id,
                    );
                    // Write changes to database
                    epic.write_async().await?;
                }
                Err(_) => {
                    let _ = log_connection_error(
                        client_sender
                            .send(Response::StoryDoesNotExist(
                                epic_id,
                                story_id,
                                epic.as_bytes(),
                            ))
                            .await,
                        peer_id,
                    );
                }
            }
        } else {
            let _ = log_connection_error(
                client_sender
                    .send(Response::EpicDoesNotExist(epic_id, db_handle.as_bytes()))
                    .await,
                peer_id,
            );
        }
        Ok(())
    }

    pub async fn unparseable_event_handler(peer_id: Uuid, clients: &mut HashMap<Uuid, Sender<Response>>) -> Result<(), DbError> {
        let mut client_sender = clients.get_mut(&peer_id).expect("client should exist");
        let _ = log_connection_error(
            client_sender.send(Response::RequestNotParsed).await,
            peer_id,
        );
        Ok(())
    }
}

/// The type that represents the command line interface for starting a new server.
#[derive(Parser)]
struct Cli {
    /// Absolute path to the directory where the database file is
    #[arg(short = 'd')]
    database_directory: String,

    /// Name of the database file
    #[arg(short = 'f')]
    database_filename: String,

    /// Name of the Epic's directory in the database directory
    #[arg(short = 'e')]
    epic_directory: String,

    /// limiting size of the broker's channel buffer
    #[arg(short = 'c')]
    channel_size: usize,

    /// Address the server will run on
    #[arg(short = 'a')]
    address: String,

    /// The port for the address of the server
    #[arg(short = 'p')]
    port: u16
}

fn main() {
    // let cli = Cli::parse();
    println!("Starting server...");
    // if let Err(e) = task::block_on(accept_loop(
    //     (cli.address.as_str(), cli.port),
    //     cli.channel_size,
    //     cli.database_directory,
    //     cli.database_filename,
    //     cli.epic_directory
    // )) {
    //     eprintln!("{e}");
    // }

    if let Err(e) = task::block_on(accept_loop(
        ("0.0.0.0", 8080),
        100,
        "/Users/benjaminhaase/development/Personal/async_jira_cli/src/test_database".to_string(),
        "test.txt".to_string(),
        "test_epics".to_string(),
    )) {
        eprintln!("{e}");
    }
}
