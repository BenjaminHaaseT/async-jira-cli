//! The binary that will run the server for the asynchronous database
mod models;
mod events;

use async_std::{
    prelude::*,
    task,
    net::{TcpListener, ToSocketAddrs, TcpStream},

};

use futures::channel::mpsc::{self, Receiver, Sender};
// use futures::io::AsyncReadExt;
use futures::sink::SinkExt;
use uuid::Uuid;

use std::fmt::Debug;
use std::sync::Arc;
use std::convert::TryFrom;
use std::collections::HashMap;

use models::prelude::*;
use events::prelude::*;
mod interface;

// enum Event {
//     NewClient {
//         peer_addr: String,
//         stream: Arc<TcpStream>,
//     },
//     AddEpic {
//         epic_name: String,
//         epic_description: String,
//     },
//     DeleteEpic {
//         epic_id: u32,
//     },
//     GetEpic {
//         epic_id: u32,
//     },
//     UpdateEpicStatus {
//         epic_id: u32,
//         status: Status,
//     },
//     GetStory {
//         epic_id: u32,
//         story_id: u32,
//     },
//     AddStory {
//         epic_id: u32,
//         story_name: String,
//         story_description: String,
//     },
//     DeleteStory {
//         epic_id: u32,
//         story_id: u32,
//     },
//     UpdateStoryStatus {
//         epic_id: u32,
//         story_id: u32,
//         status: Status,
//     },
//     ClientDisconnected
// }

// impl Event {
//     async fn try_from(tag: &[u8; 13], stream: &TcpStream) -> Result<Event, DbError> {
//         let event_byte = tag[0];
//         let mut stream = stream;
//         if event_byte & 1 != 0 {
//             let epic_name_len = parse_4_bytes(&tag[1..5]);
//             let epic_description_len = parse_4_bytes(&tag[5..9]);
//             let mut epic_name_bytes = vec![0u8; epic_name_len as usize];
//             let mut epic_description_bytes = vec![0u8; epic_description_len as usize];
//             stream.read_exact(&mut epic_name_bytes)
//                 .await
//                 .map_err(|_| DbError::ParseError(format!("unable to read epic name bytes from stream")))?;
//             stream.read_exact(&mut epic_description_bytes)
//                 .await
//                 .map_err(|_| DbError::ParseError(format!("unable to parse epic description bytes from stream")))?;
//             let epic_name = String::from_utf8(epic_name_bytes)
//                 .map_err(|_| DbError::ParseError(format!("unable to parse epic name as well formed utf8")))?;
//             let epic_description = String::from_utf8(epic_description_bytes)
//                 .map_err(|_| DbError::ParseError(format!("unable to parse epic description as well formed utf8")))?;
//             Ok(Event::AddEpic { epic_name, epic_description })
//         } else if event_byte & 2 != 0 {
//             let epic_id = parse_4_bytes(&tag[1..5]);
//             Ok(Event::DeleteEpic { epic_id })
//         } else if event_byte & 4 != 0 {
//             let epic_id = parse_4_bytes(&tag[1..5]);
//             Ok(Event::GetEpic { epic_id })
//         } else if event_byte & 8 != 0 {
//             let epic_id = parse_4_bytes(&tag[1..5]);
//             let epic_status = tag[5];
//             let status = Status::try_from(epic_status)?;
//             Ok(Event::UpdateEpicStatus { epic_id, status })
//         } else if event_byte & 16 != 0 {
//             let epic_id = parse_4_bytes(&tag[1..5]);
//             let story_id = parse_4_bytes(&tag[5..9]);
//             Ok(Event::GetStory { epic_id, story_id })
//         } else if event_byte & 32 != 0 {
//             let epic_id = parse_4_bytes(&tag[1..5]);
//             let story_name_len = parse_4_bytes(&tag[5..9]) as usize;
//             let story_description_len = parse_4_bytes(&tag[9..]) as usize;
//             let mut story_name_bytes = vec![0u8; story_name_len];
//             let mut story_description_bytes = vec![0u8; story_description_len];
//             stream.read_exact(&mut story_name_bytes)
//                 .await
//                 .map_err(|_| DbError::ParseError(format!("unable to read story name bytes from stream")))?;
//             stream.read_exact(&mut story_description_bytes)
//                 .await
//                 .map_err(|_| DbError::ParseError(format!("unable to ready story description bytes from stream")))?;
//             let story_name = String::from_utf8(story_name_bytes)
//                 .map_err(|_| DbError::ParseError(format!("unable to read story name as well formed utf8")))?;
//             let story_description = String::from_utf8(story_description_bytes)
//                 .map_err(|_| DbError::ParseError(format!("unable to read story description bytes as well formed utf8")))?;
//             Ok(Event::AddStory { epic_id, story_name, story_description })
//         } else if event_byte & 64 != 0 {
//             let epic_id = parse_4_bytes(&tag[1..5]);
//             let story_id = parse_4_bytes(&tag[5..9]);
//             Ok(Event::DeleteStory { epic_id, story_id })
//         } else if event_byte & 128 != 0 {
//             let epic_id = parse_4_bytes(&tag[1..5]);
//             let story_id = parse_4_bytes(&tag[5..9]);
//             let story_status = Status::try_from(tag[10])?;
//             Ok(Event::UpdateStoryStatus { epic_id, story_id, status: story_status })
//         } else {
//             Err(DbError::DoesNotExist(format!("unable to parse tag and stream")))
//         }
//     }
// }



enum Response {
    ClientAlreadyExists,
    AddedEpicOk(u32),
    DeletedEpicOk(u32),
    GetEpicOk(Vec<u8>),
    EpicStatusUpdateOk(u32),
    EpicDoesNotExist(u32),
    GetStoryOk(Vec<u8>),
    StoryDoesNotExist(u32, u32),
    StoryAddedOk(u32),

}

impl Response {
    /// Method that will convert a `Response` into an encoded slice of bytes
    fn as_bytes(&self) -> &[u8] {
        todo!()
    }
}

/// Helper function that will take a future, spawn it as a new task and log any errors propagated from the spawned future.
async fn spawn_and_log_errors(f: impl Future<Output = Result<(), DbError>> + Send + 'static) -> task::JoinHandle<()> {
    task::spawn(async move {
        if let Err(e) = f.await {
            eprintln!("error: {}", e);
        }
    })
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
    task::spawn(broker(broker_receiver, db_dir, db_file_name, epic_dir, channel_buf_size));

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
        stream.write_all(resp.as_bytes())
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
    // TODO: log errors instead of breaking from the loop, except in the case of necessary panics
    while let Some(event) = receiver.next().await {
        // Process each event received
        match event {
            Event::NewClient { peer_id, stream } => {
                if !clients.contains_key(&peer_id) {
                    let (client_sender, client_receiver) = mpsc::channel::<Response>(channel_buf_size);
                    clients.insert(peer_id, client_sender);
                    task::spawn(connection_write_loop(stream, client_receiver));
                } else {
                    let mut client_sender = clients.get_mut(&peer_id).unwrap();
                    //TODO: handle errors with more specificity
                    if let Err(e) = client_sender.send(Response::ClientAlreadyExists)
                        .await
                        .map_err(|_| DbError::ConnectionError(String::from("unable to send client response"))) {
                        eprintln!("error: {}", e);
                    }
                }
            }
            Event::AddEpic { peer_id, epic_name, epic_description } => {
                let epic_id = db_handle.add_epic(epic_name, epic_description);
                let mut client_sender = clients.get_mut(&peer_id).expect("client should exist");
                if let Err(e) = client_sender.send(Response::AddedEpicOk(epic_id))
                    .await
                    .map_err(|_| DbError::ConnectionError(format!("unable to send response to client {}", peer_id))) {
                    eprintln!("error: {}", e);
                }

            }
            Event::DeleteEpic { peer_id, epic_id} => {
                // TODO: ensure changes persist in database i.e write db_handle to file
                let mut client_sender = clients.get_mut(&peer_id).expect("client should exist");
                // Ensure epic_id is a valid epic
                match db_handle.delete_epic(epic_id) {
                    // We have successfully removed the epic
                    Ok(_epic) => {
                        if let Err(e) = client_sender.send(Response::DeletedEpicOk(epic_id))
                            .await
                            .map_err(|_| DbError::ConnectionError(format!("unable to send response to client {}", peer_id))) {
                            eprintln!("error: {}", e);
                        }
                    }
                    // The epic does not exist, send reply back to client
                    Err(_e) => {
                        if let Err(e) = client_sender.send(Response::EpicDoesNotExist(epic_id))
                            .await
                            .map_err(|_| DbError::ConnectionError(format!("unable to send response to client {}", peer_id))) {
                            eprintln!("error: {}", e);
                        }
                    }
                }
            }
            Event::GetEpic { peer_id, epic_id } => {

                let mut client_sender = clients.get_mut(&peer_id).expect("client should exist");
                match db_handle.get_epic(epic_id) {
                    Some(epic) => {
                        let epic_bytes = epic.as_bytes();
                        if let Err(e) = client_sender.send(Response::GetEpicOk(epic_bytes))
                            .await
                            .map_err(|_| DbError::ConnectionError(format!("unable to send response to client {}", peer_id))) {
                            eprintln!("error: {}", e);
                        }
                    }
                    None => {
                        if let Err(e) = client_sender
                            .send(Response::EpicDoesNotExist(epic_id))
                            .await
                            .map_err(|_| DbError::ConnectionError(format!("unable to send response to client {}", peer_id))) {
                            eprintln!("error: {}", e);
                        }
                    }
                }
            }
            Event::UpdateEpicStatus { peer_id, epic_id, status}  => {
                // TODO: ensure changes persist in database i.e write db_handle to file
                let mut client_sender = clients.get_mut(&peer_id).expect("client should exist");
                if let Some(epic) = db_handle.get_epic_mut(epic_id) {
                    epic.update_status(status);
                    // Send response that status was updated successfully
                    if let Err(e) = client_sender.send(Response::EpicStatusUpdateOk(epic_id))
                        .await
                        .map_err(|_| DbError::ConnectionError(format!("unable to send response to client {}", peer_id))) {
                        eprintln!("error: {}", e);
                    }
                    // Send the updated epic
                    if let Err(e) = client_sender.send(Response::GetEpicOk(epic.as_bytes()))
                        .await
                        .map_err(|_| DbError::ConnectionError(format!("unable to send response to client {}", peer_id))) {
                        eprintln!("error: {}", e);
                    }
                } else {
                    if let Err(e) = client_sender.send(Response::EpicDoesNotExist(epic_id))
                        .await
                        .map_err(|_| DbError::ConnectionError(format!("unable to send response to client {}", peer_id))) {
                        eprintln!("error: {}", e);
                    }
                }
            }
            Event::GetStory { peer_id,epic_id, story_id} => {
                let mut client_sender = clients.get_mut(&peer_id).expect("client should exist");
                if let Some(epic) = db_handle.get_epic(epic_id) {
                    match epic.get_story(story_id) {
                        Some(story) => {
                            if let Err(_) = client_sender.send(Response::GetStoryOk(story.as_bytes())).await {
                                eprintln!("error: {}", DbError::ConnectionError(format!("unable to send response to client {}", peer_id)));
                            }
                        }
                        None => {
                            if let Err(_) = client_sender.send(Response::StoryDoesNotExist(epic_id, story_id)).await {
                                eprintln!("error: {}", DbError::ConnectionError(format!("unable to send response to client {}", peer_id)));
                            }
                        }
                    }
                } else {
                    if let Err(_) = client_sender.send(Response::EpicDoesNotExist(epic_id)).await {
                        eprintln!("error: {}", DbError::ConnectionError(format!("unable to send response to client {}", peer_id)));
                    }
                }
            }
            Event::AddStory { peer_id, epic_id, story_name, story_description} => {
                // TODO: ensure changes persist in database i.e write db_handle to file
                let mut client_sender = clients.get_mut(&peer_id).expect("client should exist");
                match db_handle.add_story(epic_id, story_name, story_description).await {
                    Ok(story_id) => {
                        if let Err(_) = client_sender.send(Response::StoryAddedOk(story_id)).await {
                            eprintln!("error: {}", DbError::ConnectionError(format!("unable to send response to client {}", peer_id)));
                        }
                        if let Err(_) = client_sender.send(Response::GetEpicOk(db_handle.get_epic(epic_id).unwrap().as_bytes())).await {
                            eprintln!("error: {}", DbError::ConnectionError(format!("unable to send response to client {}", peer_id)));
                        }
                    }
                    Err(e) => {
                        if let Err(_) = client_sender.send(Response::EpicDoesNotExist(epic_id)).await {
                            eprintln!("error: {}", DbError::ConnectionError(format!("unable to send response to client {}", peer_id)));
                        }
                    }
                }

            }
            _ => todo!()
        }
    }

    todo!()
}

fn main() {todo!()}