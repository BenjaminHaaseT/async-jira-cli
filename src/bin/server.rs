//! The binary that will run the server for the asynchronous database

use async_std::{
    prelude::*,
    task,
    net::{TcpListener, ToSocketAddrs, TcpStream},
};
use futures::channel::mpsc::{self, Receiver, Sender};
use futures::sink::SinkExt;

use std::fmt::Debug;
use std::sync::Arc;

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
    },
}

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
    // let mut client_stream = Arc::new(client_stream);
    let client_stream_reader = async_std::io::BufReader::new(client_stream.clone());
    let mut client_stream = Arc::new(client_stream);
    // TODO: Address error handling, build custom error for connection/dbError differentiation
    let peer_addr = client_stream.peer_addr().map_err(|_e| DbError::ConnectionError(format!("unable to get address from client")))?.to_string();
    let new_client = Event::NewClient { peer_addr, stream: client_stream.clone() };

    // Send a new client event to the broker
    broker_sender
        .send(new_client)
        .await
        .map_err(|_e| DbError::ConnectionError(format!("unable to send event to broker")))?;

    // Read the lines from the client
    let mut lines = client_stream_reader.lines();
    while let Some(line) = lines.next().await {
        let line = line.map_err(|_e| DbError::ConnectionError(format!("unable to read line from client")))?;
        // parse line into an event and send to broker
        // todo!();
        (&*client_stream).write(&[0, 0, 0]);
        todo!();
    }

    Ok(())
}

async fn broker(reciever: Receiver<Event>) -> Result<(), DbError> {
    todo!()
}
fn main() {}