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
    fn try_from_reader<R: Read + Debug>(tag_buf: &[u8; 13], reader: &mut R) -> Result<Self, UserError> where Self: Sized;
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

#[cfg(test)]
mod test {
    use super::*;
    use std::path::PathBuf;
    use std::collections::HashMap;
    use async_jira_cli::utils::prelude::*;
    use std::io::Cursor;

    #[test]
    fn test_try_from_reader_epic() {
        // let mut db = DbState::new("test_dir".to_string(), "test_file".to_string(), "test_epic_dir".to_string());
        let test_epic = Epic::new(
            0,
            "A simple test epic1".to_string(),
            "A simple test epic for testing purposes".to_string(),
            Status::Open,
            PathBuf::new(),
            HashMap::new());

        let test_epic_bytes = test_epic.as_bytes();

        let mut cursor = Cursor::new(test_epic_bytes);

        let mut tag_buf = [0; 13];

        assert!(cursor.read_exact(&mut tag_buf).is_ok());

        let epic_frame_result = EpicFrame::try_from_reader(&tag_buf, &mut cursor);

        println!("{:?}", epic_frame_result);

        assert!(epic_frame_result.is_ok());

        let epic_frame = epic_frame_result.unwrap();

        println!("{:?}", epic_frame);

        assert_eq!(epic_frame.id, test_epic.id());
        assert_eq!(epic_frame.name, test_epic.name().clone());
        assert_eq!(epic_frame.description, test_epic.description().clone());
        assert_eq!(epic_frame.status, test_epic.status());
    }

    #[test]
    fn test_try_from_reader_story() {
        let test_story = Story::new(
            0,
            "A simple test story".to_string(),
            "A simple test story for testing purposes".to_string(),
            Status::InProgress
        );

        let test_story_bytes = test_story.as_bytes();

        let mut cursor = Cursor::new(test_story_bytes);

        let mut tag_buf = [0; 13];

        assert!(cursor.read_exact(&mut tag_buf).is_ok());

        let story_frame_result = StoryFrame::try_from_reader(&tag_buf, &mut cursor);

        println!("{:?}", story_frame_result);

        let story_frame = story_frame_result.unwrap();

        println!("{:?}", story_frame);

        assert_eq!(story_frame.id, test_story.id());
        assert_eq!(story_frame.name, test_story.name().clone());
        assert_eq!(story_frame.description, test_story.description().clone());
        assert_eq!(story_frame.status, test_story.status());
    }
}
