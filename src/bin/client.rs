//! Implements a simple server that will send requests and receive responses from the server.
//! Gives users an interface to interact with the server.

use std::io::{BufRead, stdin};
use async_std::{
    io::ReadExt,
    net::{TcpStream, ToSocketAddrs},
    prelude::*,
    task,
};

use std::fmt::{Debug, Display, Formatter};

mod interface;

use crate::interface::prelude::*;
use async_jira_cli::response::prelude::*;
use async_jira_cli::utils::prelude::*;

#[derive(Debug)]
pub enum UserError {
    ServerConnection(String),
    ParseResponseError(String),
    ReadFrameError(String),
    ParseFrameError(String),
    InternalServerError,
    ParseRequestOption,
    InvalidRequest,
    InvalidInput(String),
    ParseInputError,
}

impl Display for UserError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            UserError::ServerConnection(s) => write!(f, "{s}"),
            UserError::ParseResponseError(s) => write!(f, "{s}"),
            UserError::ReadFrameError(s) => write!(f, "{s}"),
            UserError::ParseFrameError(s) => write!(f, "{s}"),
            UserError::InternalServerError => write!(f, "An internal server error occurred, please try again"),
            UserError::ParseRequestOption => write!(f, "Unable to parse entered option, please try again"),
            UserError::InvalidRequest => write!(f, "Invalid request, please choose a valid option"),
            UserError::InvalidInput(s) => write!(f, "{s}"),
            UserError::ParseInputError => write!(f, "Unable to parse input, please try again"),
        }
    }
}

impl std::error::Error for UserError {}



// async fn run(server_addrs: impl ToSocketAddrs + Debug + Clone) -> Result<(), UserError> {
//     // Connect to the server
//     let stream = TcpStream::connect(server_addrs.clone())
//         .await
//         .map_err(|_| {
//             UserError::ServerConnection(format!("unable to connect to {:?}", server_addrs))
//         })?;
//
//     let mut user_interface = Interface::new();
//
//     // Split the stream into read/write halves. The sender will only write
//     // and the receiver will only read
//     let (mut writer, mut reader) = (&stream, &stream);
//     let mut input = BufReader::new(stdin());
//
//     // Create a response tag for parsing responses from a stream of bytes
//     let mut tag = [0u8; 18];
//
//     loop {
//         // Attempt to read response from server
//         reader.read_exact(&mut tag).await.map_err(|_| {
//             UserError::ServerConnection(format!("unable to read response tag from server"))
//         })?;
//         // // let (type_and_flag, epic_id, story_id, data_len) = Response::decode(tag);
//         // // // server, display appropriate data to the client
//         // // TODO: parse the response from the server, and display the appropriate page to user
//         // // TODO: get input from the user, parse it (with error handling) and send request to server
//         // todo!()
//         let _ = user_interface.parse_response(&tag, reader).await.unwrap();
//     }
//
//     todo!()
// }

async fn run(server_addrs: impl ToSocketAddrs + Debug + Clone) -> Result<(), UserError> {
    println!("connection to {:?}...", server_addrs);
    let stream = TcpStream::connect(server_addrs.clone())
        .await
        .map_err(|_| UserError::ServerConnection(format!("unable to connect to {:?}", server_addrs)))?;

    // split into read/write halves
    let (reader, writer) = (&stream, &stream);

    // user input reader
    let user_input = std::io::BufReader::new(stdin());

    // create the interface
    let mut interface = Interface::new(
        reader,
        writer,
        user_input,
    );

    // TODO: log errors
    interface.run().await
}

fn main() {}
