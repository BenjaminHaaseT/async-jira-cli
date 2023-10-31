//! Contains helpful structs and traits for creating a page based CLI

use crate::interface::frame::prelude::*;
use crate::UserError;
use async_jira_cli::models::prelude::*;
use async_jira_cli::utils::prelude::*;
use std::io::{Read, Cursor, Seek, ErrorKind};

pub mod prelude {
    pub use super::*;
}

/// The `Page` trait allows for different types of CLI interface pages to share
/// the functionality of any typical CLI page i.e. `print_page`. The main purpose is to allow
/// different types of pages to be held in a single data structure as trait objects.
pub trait Page {
    fn print_page(&self);
}

/// Represents the first page that the user will see.
struct HomePage {
    epic_frames: Vec<EpicFrame>
}

impl HomePage {
    pub fn try_create(mut data: Vec<u8>) -> Result<Self, UserError> {
        let data_len = data.len() as u64;
        let mut cursor = Cursor::new(data);
        let mut tag_buf = [0u8; 13];
        let mut cur_pos = cursor.position();
        let mut epic_frames = vec![];

        loop {
            // Ensure we can read properly a properly formatted tag
            match cursor.read_exact(&mut tag_buf) {
                Ok(_) => {
                    let epic_frame = EpicFrame::try_from_reader(&tag_buf, &mut cursor)?;
                    epic_frames.push(epic_frame);
                }
                Err(e) if e.kind() == ErrorKind::UnexpectedEof => {
                    // Check that we have reached the end of the cursor, and that the current position
                    // is equal to the cursor's current position. If either of these conditions is not true
                    // we know the data was not formatted correctly.
                    if cur_pos != cursor.position() || cursor.position() != data_len {
                        return Err(UserError::ReadFrameError("unable to read frame from response data".to_string()));
                    }
                    break;
                }
                _ => return Err(UserError::ReadFrameError("unable to read frame from response data".to_string()));
            }
            // Keep track of the current position,
            // used to ensure that we have read properly formatted data from the server
            cur_pos = cursor.position();
        }

        Ok(HomePage { epic_frames })
    }
}





