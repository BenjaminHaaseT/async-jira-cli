//! Implements a simple server that will send requests and receive responses from the server.
//! Gives users an interface to interact with the server.

use std::fmt::Debug;
use async_std::{
    prelude::*,
    net::{ToSocketAddrs, TcpStream},
    io::{BufReader, BufRead, stdin, Stdin},
    task
};

use async_jira_cli::response::prelude::*;
use async_jira_cli::utils::prelude::*;
enum UserError {
    ServerConnection(String),
}

async fn run(server_addrs: impl ToSocketAddrs + Debug + Clone) -> Result<(), UserError> {
    // Connect to the server
    let stream = TcpStream::connect(server_addrs.clone())
        .await
        .map_err(|_| UserError::ServerConnection(format!("unable to connect to {:?}", server_addrs)))?;

    // Split the stream into read/write halves. The sender will only write
    // and the receiver will only read
    let (mut sender, mut receiver) = (&stream, &stream);
    let mut input = BufReader::new(stdin());

    // Create a response tag for parsing responses from a stream of bytes
    let mut tag = [0u8; 13];

    loop {
        // Attempt to read response from server
        receiver
            .read_exact(&mut tag)
            .await
            .map_err(|_| UserError::ServerConnection(format!("unable to read response tag from server")))?;
        // TODO: We need a way to parse the response from the server, then provide
        // TODO: a menu of options to the user, and parse an appropriate resposne
        todo!()
    }

    todo!()
}

fn main() {}