//! The binary that will run the server for the asynchronous database

use async_std::{
    prelude::*,
    task,
    net::{TcpListener, ToSocketAddrs, TcpStream},
};
use futures::channel::mpsc::{self, Receiver, Sender};
use futures::sink::SinkExt;

use std::fmt::Debug;

mod models;
use models::prelude::*;
mod interface;

enum Event {}

async fn accept_loop(addrs: impl ToSocketAddrs + Debug, channel_buf_size: usize) -> Result<(), DbError> {
    println!("connecting to {:?}...", addrs);
    let listener = TcpListener::bind(addrs).await.map_err(|_e| DbError::ConnectionError(format!("could not connect to {:?}", addrs)))?;
    let (broker_sender, broker_receiver) = mpsc::channel::<Event>(channel_buf_size);
    task::spawn(broker(broker_receiver));

    while let Some(stream_res) = listener.incoming().next().await {
        let stream = stream_res.map_err(DbError::ConnectionError(format!("unable to accept stream")))?;
        task::spawn(connection_loop(stream, broker_sender.clone()));
    }

    Ok(())
}

async fn connection_loop(client_stream: TcpStream, broker_sender: Sender<Event>) -> Result<(), DbError> {
    todo!()
}

async fn broker(reciever: Receiver<Event>) -> Result<(), DbError> {
    todo!()
}
fn main() {}