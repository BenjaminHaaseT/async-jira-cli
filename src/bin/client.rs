//! Implements a simple server that will send requests and receive responses from the server.
//! Gives users an interface to interact with the server.

use async_std::{
    io::{stdin, BufRead, BufReader, Stdin},
    net::{TcpStream, ToSocketAddrs},
    prelude::*,
    task,
};

use std::fmt::Debug;

mod interface;

use async_jira_cli::response::prelude::*;
use async_jira_cli::utils::prelude::*;

#[derive(Debug)]
enum UserError {
    ServerConnection(String),
    ParseResponseError(String),
    ReadFrameError(String),
    ParseFrameError(String),
}

async fn run(server_addrs: impl ToSocketAddrs + Debug + Clone) -> Result<(), UserError> {
    // Connect to the server
    let stream = TcpStream::connect(server_addrs.clone())
        .await
        .map_err(|_| {
            UserError::ServerConnection(format!("unable to connect to {:?}", server_addrs))
        })?;

    // Split the stream into read/write halves. The sender will only write
    // and the receiver will only read
    let (mut sender, mut receiver) = (&stream, &stream);
    let mut input = BufReader::new(stdin());

    // Create a response tag for parsing responses from a stream of bytes
    let mut tag = [0u8; 18];

    loop {
        // Attempt to read response from server
        receiver.read_exact(&mut tag).await.map_err(|_| {
            UserError::ServerConnection(format!("unable to read response tag from server"))
        })?;
        // let (type_and_flag, epic_id, story_id, data_len) = Response::decode(tag);
        // // server, display appropriate data to the client
        // TODO: parse the response from the server, and display the appropriate page to user
        // TODO: get input from the user, parse it (with error handling) and send request to server
        todo!()
    }

    todo!()
}

fn main() {}
