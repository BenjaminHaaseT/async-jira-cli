//! Implements a simple server that will send requests and receive responses from the server.
//! Gives users an interface to interact with the server.

// use std::io::{BufRead, BufReader, stdin};
use std::fmt::{Debug, Display, Formatter};
use clap::Parser;
use async_std::{
    io::{Read, ReadExt, prelude::{BufRead, BufReadExt}, BufReader, stdin},
    net::{TcpStream, ToSocketAddrs},
    prelude::*,
    task::block_on,
};
use tracing::{instrument, event, Level};
use tracing_subscriber::prelude::*;
use tracing_appender;

use crate::interface::prelude::*;
use async_jira_cli::utils::prelude::*;

mod interface;

/// The error type for any user facing error encountered by the client.
#[derive(Debug)]
pub enum UserError {
    /// An error the connection to the server.
    ServerConnection(String),

    /// An error from parsing the response received by the server.
    ParseResponseError(String),

    /// An error from reading a frame.
    ReadFrameError(String),

    /// An error from trying to parse a frame.
    ParseFrameError(String),

    /// Internal server error.
    InternalServerError,

    /// An error in parsing the request from the client.
    ParseRequestOption,

    /// An invalid request by the client.
    InvalidRequest,

    /// Invalid input from the client.
    InvalidInput(String),

    /// An error parsing input received from the client.
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

/// Represents the command line interface arguments for running the binary.
#[derive(Parser)]
struct Cli {
    /// The address of the server the client is connecting to
    address: String,

    /// Port number
    port: u16,
}

/// Takes `server_addrs` and starts a new client connection.
#[instrument(ret, err)]
async fn run(server_addrs: impl ToSocketAddrs + Debug + Clone) -> Result<(), UserError> {
    println!("connecting to server {:?}...", server_addrs);
    event!(Level::INFO, server_address = ?server_addrs, "attempting to open a connection to server at {:?}", server_addrs);
    let connection = TcpStream::connect(server_addrs.clone())
        .await
        .map_err(
        |_| UserError::ServerConnection(format!("unable to connect to server {:?}", server_addrs))
    )?;
    let client_input = BufReader::new(stdin());
    let mut interface = Interface::new(connection, client_input);
    event!(Level::INFO, server_address = ?server_addrs, "successfully opened a connection to server at {:?}", server_addrs);
    interface.run().await
}

fn main() {
    // Create appender for logging client events
    let appender = tracing_appender::rolling::never("/Users/benjaminhaase/development/Personal/async_jira_cli", "client_logs.log");
    let (writer, _guard) = tracing_appender::non_blocking(appender);
    let filter = tracing_subscriber::EnvFilter::from_default_env();

    tracing_subscriber::fmt()
        .with_level(true)
        .with_line_number(true)
        .with_target(true)
        .with_thread_ids(true)
        .with_file(true)
        .with_env_filter(filter)
        .with_writer(writer)
        .init();

    let cli = Cli::parse();
    let addrs = (cli.address.as_str(), cli.port);
    if let Err(e) = block_on(run(addrs)) {
        eprintln!("{e}");
    }
    println!("goodbye");
}
