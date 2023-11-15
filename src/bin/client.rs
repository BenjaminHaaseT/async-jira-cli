//! Implements a simple server that will send requests and receive responses from the server.
//! Gives users an interface to interact with the server.

use std::io::{BufRead, BufReader, stdin};
use std::fmt::{Debug, Display, Formatter};
use async_std::{
    io::ReadExt,
    net::{TcpStream, ToSocketAddrs},
    prelude::*,
};

mod interface;

use crate::interface::prelude::*;
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

async fn run(server_addrs: impl ToSocketAddrs + Debug + Clone) -> Result<(), UserError> {
    println!("connecting to server {:?}...", server_addrs);
    let connection = TcpStream::connect(server_addrs.clone())
        .await
        .map_err(
        |_| UserError::ServerConnection(format!("unable to connect to server {:?}", server_addrs))
    )?;
    let client_input = BufReader::new(stdin());
    let mut interface = Interface::new(connection, client_input);
    interface.run().await
}

fn main() {

}
