//! Provides the necessary components for creating a smoothing CLI for clients interacting
//! with the server.
//!

use async_std::net::TcpStream;
use async_std::io::{ReadExt, Read};
use async_jira_cli::response::prelude::*;
use async_jira_cli::utils::prelude::*;
use crate::UserError;

mod frame;
mod pages;

// use frame::prelude::*;
// use pages::prelude::*;

/// Implements the functionality for creating a CLI that a client can use to interact with
/// the server.
///
/// The struct can take correctly parsed `Response` tags read from a `TcpStream` and display
/// the appropriate prompt to the user on stdout via `parse_response` method. Internally,
/// an `Interface` will keep a stack of pages that can be navigated through by the client, and
/// will display appropriate messages when errors arise.
pub struct Interface {
    // page_stack = Vec<Box<dyn Page>>
}

impl Interface {

    // Attempt to read a response from the stream. If response read correctly, then parse frames
    // from the read data and create a new page and ad page to page_stack, or display a message and a current page depending
    // on the logic.
    pub async fn parse_response(tag_buf: &[u8; 18], stream: &TcpStream) -> Result<(), UserError> {
        let mut stream = stream;
        let (type_and_flag, epic_id, story_id, data_len) = Response::decode(tag_buf);
        if type_and_flag & 1 != 0 {
            let mut data = vec![0; data_len as usize];
            stream.read_exact(data.as_mut_slice())
                .await
                .map_err(|_|
                UserError::ParseResponseError(format!("unable to parse response from server correctly"))
            )?
            // TODO: create homepage, parse the epic frames from `data`. Then add homepage to `self.page_stack`

        }

        todo!()
    }
}