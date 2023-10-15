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

mod models;
use models::prelude::*;
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
            let story_id = parse_4_bytes(&tag[5..10]);
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
            todo!()
        } else if event_byte & 128 != 0 {
            todo!()
        } else {
            todo!()
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

// // impl TryFrom<([u8; 13], &TcpStream)> for Event {
//     type Error = DbError;
//     async fn try_from(value: ([u8; 13], &TcpStream)) -> Result<Self, Self::Error> {
//         let (tag, stream) = (value.0, value.1);
//         let event_byte = tag[0];
//         if event_byte & 1 != 0 {
//             let epic_name_len = parse_4_bytes(&tag[1..5]);
//             let epic_description_len = parse_4_bytes(&tag[5..10]);
//             let mut epic_name_bytes = vec![0u8; epic_name_len as usize];
//             let mut epic_description_len = vec![0u8; epic_description_len as usize];
//             stream.read_exact(epic_name_bytes.as_mut_slice()).await.map_err(|_| DbError::ParseError("unable to parse name bytes from stream".to_string()))?;
//
//             todo!()
//         } else if event_byte & 2 != 0 {
//             todo!()
//         } else if event_byte & 4 != 0 {
//             todo!()
//         } else if event_byte & 8 != 0 {
//             todo!()
//         } else if event_byte & 16 != 0 {
//             todo!()
//         } else if event_byte & 32 != 0 {
//             todo!()
//         } else if event_byte & 64 != 0 {
//             todo!()
//         } else if event_byte & 128 != 0 {
//             todo!()
//         } else {
//             todo!()
//         }
//     }
// }

async fn accept_loop(addrs: impl ToSocketAddrs + Debug + Clone, channel_buf_size: usize) -> Result<(), DbError> {
    // Connect to the servers socket address
    println!("connecting to {:?}...", addrs);
    let addrs_clone = addrs.clone();
    let listener = TcpListener::bind(addrs).await.map_err(|_e| DbError::ConnectionError(format!("could not connect to {:?}", addrs_clone)))?;

    // Get a channel to the broker, and spawn the brokers task
    let (broker_sender, broker_receiver) = mpsc::channel::<Event>(channel_buf_size);
    task::spawn(broker(broker_receiver));

    while let Some(stream_res) = listener.incoming().next().await {
        let stream = stream_res.map_err(|_e| DbError::ConnectionError(format!("unable to accept stream")))?;
        // TODO: add error handling to log all errors
        task::spawn(connection_loop(stream, broker_sender.clone()));
    }

    Ok(())
}

async fn connection_loop(client_stream: TcpStream, mut broker_sender: Sender<Event>) -> Result<(), DbError> {
    let client_stream = Arc::new(client_stream);
    let mut client_stream_reader = &*client_stream;

    // TODO: Add better error handling, i.e. custom connect error handling
    let peer_addr = client_stream.peer_addr().map_err(|_| DbError::ParseError(format!("unable to parse address from client")))?.to_string();

    // Create a new client and send to broker
    let new_client = Event::NewClient { peer_addr: peer_addr.clone(), stream: client_stream.clone() };
    broker_sender.send(new_client)
        .await
        .map_err(|_| DbError::ConnectionError(format!("unable to send client to broker")))?;

    let mut tag = [0u8; 13];

    while let Ok(_) = client_stream_reader.read_exact(&mut tag).await {
        // TODO: handle the case when input from user is not parse-able ie send a special error event to the broker to send back to the client.
        let event = Event::try_from(&tag, client_stream_reader).await?;
        broker_sender
            .send(event)
            .await
            .map_err(|_| DbError::ConnectionError(format!("unable to send event to broker")))?;
    }

    Ok(())
}

async fn broker(reciever: Receiver<Event>) -> Result<(), DbError> {
    todo!()
}
fn main() {}