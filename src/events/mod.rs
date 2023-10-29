//! Module that contains the `Event` struct.
use std::sync::Arc;
use async_std::{
    prelude::*,
    net::TcpStream,
};
use futures::channel::mpsc::UnboundedReceiver;
use uuid::Uuid;
use crate::models::{Status, DbError};
use crate::utils::{parse_4_bytes, Void};

pub mod prelude {
    pub use super::*;
}

/// A struct that is used to parse particular events from a `TcpStream` and sent a a broker task.
pub enum Event {
    /// Represents the event of a new client connecting
    NewClient {
        peer_id: Uuid,
        stream: Arc<TcpStream>,
        shutdown: UnboundedReceiver<Void>
    },

    /// Represents a client request to add an `Epic`
    AddEpic {
        peer_id: Uuid,
        epic_name: String,
        epic_description: String,
    },

    /// Represents a client request to delete an `Epic`
    DeleteEpic {
        peer_id: Uuid,
        epic_id: u32,
    },

    /// Represents a client request to get an `Epics` data
    GetEpic {
        peer_id: Uuid,
        epic_id: u32,
    },

    /// Represents a client request to update an `Epic`s status field
    UpdateEpicStatus {
        peer_id: Uuid,
        epic_id: u32,
        status: Status,
    },

    /// Represents a client request to get a `Story's data
    GetStory {
        peer_id: Uuid,
        epic_id: u32,
        story_id: u32,
    },

    /// Represents a client request to add a `Story`
    AddStory {
        peer_id: Uuid,
        epic_id: u32,
        story_name: String,
        story_description: String,
    },

    /// Represents a client request to delete a `Story`
    DeleteStory {
        peer_id: Uuid,
        epic_id: u32,
        story_id: u32,
    },

    /// Represents a client request to update a `Story` status
    UpdateStoryStatus {
        peer_id: Uuid,
        epic_id: u32,
        story_id: u32,
        status: Status,
    },

    /// Signifies an unsuccessful parsing of an event that needs to be relayed back to the client
    UnparseableEvent { peer_id: Uuid }
}

impl Event {
    /// An associated method attempt creation of a new `Event` given `client_id`, `tag` and `stream`.
    /// Is fallible, and hence returns a `Result<Event, DbError>`.
    pub async fn try_create(client_id: Uuid, tag: &[u8; 13], stream: &TcpStream) -> Result<Event, DbError> {
        let event_byte = tag[0];
        if event_byte & 1 != 0 {
            Event::try_create_add_epic(client_id, tag, stream).await
        } else if event_byte & 2 != 0 {
            Event::try_create_delete_epic(client_id, tag).await
        } else if event_byte & 4 != 0 {
            Event::try_create_get_epic(client_id, tag).await
        } else if event_byte & 8 != 0 {
            Event::try_create_update_epic_status(client_id, tag).await
        } else if event_byte & 16 != 0 {
            Event::try_create_get_story(client_id, tag).await
        } else if event_byte & 32 != 0 {
            Event::try_create_add_story(client_id, tag, stream).await
        } else if event_byte & 64 != 0 {
            Event::try_create_delete_story(client_id, tag).await
        } else if event_byte & 128 != 0 {
            Event::try_create_update_story_status(client_id, tag).await
        } else {
            Err(DbError::DoesNotExist(format!("unable to parse tag and stream")))
        }
    }

    /// Helper method for `try_create`. Attempts to create a `AddEpic` variant.
    async fn try_create_add_epic(client_id: Uuid, tag: &[u8; 13], stream: &TcpStream) -> Result<Event, DbError> {
        let mut stream = stream;
        let epic_name_len = parse_4_bytes(&tag[1..5]);
        let epic_description_len = parse_4_bytes(&tag[5..9]);
        let mut epic_name_bytes = vec![0u8; epic_name_len as usize];
        let mut epic_description_bytes = vec![0u8; epic_description_len as usize];
        stream.read_exact(&mut epic_name_bytes)
            .await
            .map_err(|_| DbError::ParseError(format!("unable to read epic name bytes from stream")))?;
        stream.read_exact(&mut epic_description_bytes)
            .await
            .map_err(|_| DbError::ParseError(format!("unable to parse epic description bytes from stream")))?;
        let epic_name = String::from_utf8(epic_name_bytes)
            .map_err(|_| DbError::ParseError(format!("unable to parse epic name as well formed utf8")))?;
        let epic_description = String::from_utf8(epic_description_bytes)
            .map_err(|_| DbError::ParseError(format!("unable to parse epic description as well formed utf8")))?;
        Ok(Event::AddEpic {peer_id: client_id, epic_name, epic_description })
    }

    /// Helper method for `try_create`. Attempts to create a `DeleteEpic` variant.
    async fn try_create_delete_epic(client_id: Uuid, tag: &[u8; 13]) -> Result<Event, DbError> {
        let epic_id = parse_4_bytes(&tag[1..5]);
        Ok(Event::DeleteEpic { peer_id: client_id, epic_id })
    }

    /// Helper method for `try_create`. Attempts to create a `GetEpic` variant.
    async fn try_create_get_epic(client_id: Uuid, tag: &[u8; 13]) -> Result<Event, DbError> {
        let epic_id = parse_4_bytes(&tag[1..5]);
        Ok(Event::GetEpic { peer_id: client_id, epic_id })
    }

    /// Helper method for `try_create`. Attempts to create a `UpdateEpicStatus` variant.
    async fn try_create_update_epic_status(client_id: Uuid, tag: &[u8; 13]) -> Result<Event, DbError> {
        let epic_id = parse_4_bytes(&tag[1..5]);
        let epic_status = tag[5];
        let status = Status::try_from(epic_status)?;
        Ok(Event::UpdateEpicStatus { peer_id: client_id, epic_id, status })
    }

    /// Helper method for `try_create`. Attempts to create a `GetStory` variant.
    async fn try_create_get_story(client_id: Uuid, tag: &[u8; 13]) -> Result<Event, DbError> {
        let epic_id = parse_4_bytes(&tag[1..5]);
        let story_id = parse_4_bytes(&tag[5..9]);
        Ok(Event::GetStory { peer_id: client_id, epic_id, story_id })
    }

    /// Helper method for `try_create`. Attempts to create a `AddStory` variant.
    async fn try_create_add_story(client_id: Uuid, tag: &[u8; 13], stream: &TcpStream) -> Result<Event, DbError> {
        let mut stream = stream;
        let epic_id = parse_4_bytes(&tag[1..5]);
        let story_name_len = parse_4_bytes(&tag[5..9]) as usize;
        let story_description_len = parse_4_bytes(&tag[9..]) as usize;
        let mut story_name_bytes = vec![0u8; story_name_len];
        let mut story_description_bytes = vec![0u8; story_description_len];
        stream.read_exact(&mut story_name_bytes)
            .await
            .map_err(|_| DbError::ParseError(format!("unable to read story name bytes from stream")))?;
        stream.read_exact(&mut story_description_bytes)
            .await
            .map_err(|_| DbError::ParseError(format!("unable to ready story description bytes from stream")))?;
        let story_name = String::from_utf8(story_name_bytes)
            .map_err(|_| DbError::ParseError(format!("unable to read story name as well formed utf8")))?;
        let story_description = String::from_utf8(story_description_bytes)
            .map_err(|_| DbError::ParseError(format!("unable to read story description bytes as well formed utf8")))?;
        Ok(Event::AddStory { peer_id: client_id, epic_id, story_name, story_description })
    }

    /// Helper method for `try_create`. Attempts to create a `DeleteStory` variant.
    async fn try_create_delete_story(client_id: Uuid, tag: &[u8; 13]) -> Result<Event, DbError> {
        let epic_id = parse_4_bytes(&tag[1..5]);
        let story_id = parse_4_bytes(&tag[5..9]);
        Ok(Event::DeleteStory { peer_id: client_id, epic_id, story_id })
    }

    /// Helper method for `try_create`. Attempts to create a `UpdateStoryStatus` variant.
    async fn try_create_update_story_status(client_id: Uuid, tag: &[u8; 13]) -> Result<Event, DbError> {
        let epic_id = parse_4_bytes(&tag[1..5]);
        let story_id = parse_4_bytes(&tag[5..9]);
        let story_status = Status::try_from(tag[10])?;
        Ok(Event::UpdateStoryStatus { peer_id: client_id, epic_id, story_id, status: story_status })
    }
}

