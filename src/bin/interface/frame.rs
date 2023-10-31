//! Module for helping client parse the data from the responses.

use std::fmt::Debug;
use std::io::Read;
use async_jira_cli::models::{Epic, Story, Status};
use async_jira_cli::utils::BytesEncode;
use crate::UserError;

pub mod prelude {
    pub use super::*;
}

/// An interface that allows implementors to be created from a reader and a tag.
pub trait TryFromReader {
    fn try_from_reader<R: Read + Debug>(tag_buf: &[u8; 13], reader: &mut R) -> Result<Self, UserError>;
}

/// Represents the important client facing information of a single `Epic`
#[derive(Debug)]
pub struct EpicFrame {
    id: u32,
    name: String,
    description: String,
    status: Status,
}

impl TryFromReader for EpicFrame {
    fn try_from_reader<R: Read + Debug>(tag_buf: &[u8; 13], reader: &mut R) -> Result<Self, UserError> {
        let mut reader = reader;
        let (epic_id, name_len, description_len, status) = Epic::decode(tag_buf);
        let mut name_bytes = vec![0u8; name_len as usize];
        let mut description_bytes = vec![0; description_len as usize];
        let status = Status::try_from(status)
            .map_err(|_| UserError::ParseFrameError(format!("unable to parse status byte for epic {}", epic_id)))?;

        // Attempt to read bytes from the reader
        reader.read_exact(name_bytes.as_mut_slice())
            .map_err(|_| UserError::ReadFrameError(format!("unable to read frame from reader {:?}", reader)))?;
        reader.read_exact(description_bytes.as_mut_slice())
            .map_err(|_| UserError::ReadFrameError(format!("unable to read frame from reader {:?}", reader)))?;

        let name = String::from_utf8(name_bytes)
            .map_err(|_| UserError::ParseFrameError("unable to parse Epic Frame's name as valid utf8".to_string()))?;
        let description = String::from_utf8(description_bytes)
            .map_err(|_| UserError::ParseFrameError("unable to parse Epic Frame's description as valid utf8".to_string()))?;

        Ok(EpicFrame { id: epic_id, name, description, status })
    }
}

/// Represents the important client facing information for a single `Story`
#[derive(Debug)]
pub struct StoryFrame {
    id: u32,
    name: String,
    description: String,
    status: Status,
}

impl TryFromReader for StoryFrame {
    fn try_from_reader<R: Read + Debug>(tag_buf: &[u8; 13], reader: &mut R) -> Result<Self, UserError> {
        let mut reader = reader;
        let (story_id, name_len, description_len, status) = Story::decode(tag_buf);
        let mut name_bytes = vec![0; name_len as usize];
        let mut description_bytes = vec![0; description_len as usize];
        let status = Status::try_from(status)
            .map_err(|_| UserError::ParseFrameError(format!("unable to parse status for story {}", story_id)))?;

        // Attempt to read bytes from the reader
        reader.read_exact(name_bytes.as_mut_slice())
            .map_err(|_| UserError::ReadFrameError(format!("unable to read story name from reader {:?}", reader)))?;
        reader.read_exact(description_bytes.as_mut_slice())
            .map_err(|_| UserError::ReadFrameError(format!("unable to read story description from reader {:?}", reader)))?;

        let name = String::from_utf8(name_bytes)
            .map_err(|_| UserError::ParseFrameError("unable to parse Story Frame's name as valid utf8".to_string()))?;
        let description = String::from_utf8(description_bytes)
            .map_err(|_| UserError::ParseFrameError("unable to parse Story Frame's description as valid utf8".to_string()))?;

        Ok(StoryFrame { id: story_id, name, description, status })
    }
}


