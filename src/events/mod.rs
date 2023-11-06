//! Module that contains the `Event` struct.
use crate::models::{DbError, Status};
use crate::utils::{parse_4_bytes, Void};
use async_std::{net::TcpStream, prelude::*, io::{Read, ReadExt}};
use futures::channel::mpsc::UnboundedReceiver;
use std::sync::Arc;
use std::marker::Unpin;
use uuid::Uuid;

pub mod prelude {
    pub use super::*;
}

/// A struct that is used to parse particular events from a `TcpStream` and sent a a broker task.
#[derive(Debug)]
pub enum Event {
    /// Represents the event of a new client connecting
    NewClient {
        peer_id: Uuid,
        stream: Arc<TcpStream>,
        shutdown: UnboundedReceiver<Void>,
    },

    /// Represents a client request to add an `Epic`
    AddEpic {
        peer_id: Uuid,
        epic_name: String,
        epic_description: String,
    },

    /// Represents a client request to delete an `Epic`
    DeleteEpic { peer_id: Uuid, epic_id: u32 },

    /// Represents a client request to get an `Epics` data
    GetEpic { peer_id: Uuid, epic_id: u32 },

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
    UnparseableEvent { peer_id: Uuid },
}

impl Event {
    /// An associated method attempt creation of a new `Event` given `client_id`, `tag` and `stream`.
    /// Is fallible, and hence returns a `Result<Event, DbError>`.
    pub async fn try_create<R: ReadExt + Unpin>(
        client_id: Uuid,
        tag: &[u8; 13],
        stream: R,
    ) -> Result<Event, DbError> {
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
            Err(DbError::DoesNotExist(format!(
                "unable to parse tag and stream"
            )))
        }
    }

    /// Helper method for `try_create`. Attempts to create a `AddEpic` variant.
    async fn try_create_add_epic<R: ReadExt + Unpin>(
        client_id: Uuid,
        tag: &[u8; 13],
        stream: R,
    ) -> Result<Event, DbError> {
        let mut stream = stream;
        let epic_name_len = parse_4_bytes(&tag[1..5]);
        let epic_description_len = parse_4_bytes(&tag[5..9]);
        let mut epic_name_bytes = vec![0u8; epic_name_len as usize];
        let mut epic_description_bytes = vec![0u8; epic_description_len as usize];
        stream.read_exact(&mut epic_name_bytes).await.map_err(|_| {
            DbError::ParseError(format!("unable to read epic name bytes from stream"))
        })?;
        stream
            .read_exact(&mut epic_description_bytes)
            .await
            .map_err(|_| {
                DbError::ParseError(format!(
                    "unable to parse epic description bytes from stream"
                ))
            })?;
        let epic_name = String::from_utf8(epic_name_bytes).map_err(|_| {
            DbError::ParseError(format!("unable to parse epic name as well formed utf8"))
        })?;
        let epic_description = String::from_utf8(epic_description_bytes).map_err(|_| {
            DbError::ParseError(format!(
                "unable to parse epic description as well formed utf8"
            ))
        })?;
        Ok(Event::AddEpic {
            peer_id: client_id,
            epic_name,
            epic_description,
        })
    }

    /// Helper method for `try_create`. Attempts to create a `DeleteEpic` variant.
    async fn try_create_delete_epic(client_id: Uuid, tag: &[u8; 13]) -> Result<Event, DbError> {
        let epic_id = parse_4_bytes(&tag[1..5]);
        Ok(Event::DeleteEpic {
            peer_id: client_id,
            epic_id,
        })
    }

    /// Helper method for `try_create`. Attempts to create a `GetEpic` variant.
    async fn try_create_get_epic(client_id: Uuid, tag: &[u8; 13]) -> Result<Event, DbError> {
        let epic_id = parse_4_bytes(&tag[1..5]);
        Ok(Event::GetEpic {
            peer_id: client_id,
            epic_id,
        })
    }

    /// Helper method for `try_create`. Attempts to create a `UpdateEpicStatus` variant.
    async fn try_create_update_epic_status(
        client_id: Uuid,
        tag: &[u8; 13],
    ) -> Result<Event, DbError> {
        let epic_id = parse_4_bytes(&tag[1..5]);
        let epic_status = tag[5];
        let status = Status::try_from(epic_status)?;
        Ok(Event::UpdateEpicStatus {
            peer_id: client_id,
            epic_id,
            status,
        })
    }

    /// Helper method for `try_create`. Attempts to create a `GetStory` variant.
    async fn try_create_get_story(client_id: Uuid, tag: &[u8; 13]) -> Result<Event, DbError> {
        let epic_id = parse_4_bytes(&tag[1..5]);
        let story_id = parse_4_bytes(&tag[5..9]);
        Ok(Event::GetStory {
            peer_id: client_id,
            epic_id,
            story_id,
        })
    }

    /// Helper method for `try_create`. Attempts to create a `AddStory` variant.
    async fn try_create_add_story<R: ReadExt + Unpin>(
        client_id: Uuid,
        tag: &[u8; 13],
        stream: R,
    ) -> Result<Event, DbError> {
        let mut stream = stream;
        let epic_id = parse_4_bytes(&tag[1..5]);
        let story_name_len = parse_4_bytes(&tag[5..9]) as usize;
        let story_description_len = parse_4_bytes(&tag[9..]) as usize;
        let mut story_name_bytes = vec![0u8; story_name_len];
        let mut story_description_bytes = vec![0u8; story_description_len];
        stream
            .read_exact(&mut story_name_bytes)
            .await
            .map_err(|_| {
                DbError::ParseError(format!("unable to read story name bytes from stream"))
            })?;
        stream
            .read_exact(&mut story_description_bytes)
            .await
            .map_err(|_| {
                DbError::ParseError(format!(
                    "unable to ready story description bytes from stream"
                ))
            })?;
        let story_name = String::from_utf8(story_name_bytes).map_err(|_| {
            DbError::ParseError(format!("unable to read story name as well formed utf8"))
        })?;
        let story_description = String::from_utf8(story_description_bytes).map_err(|_| {
            DbError::ParseError(format!(
                "unable to read story description bytes as well formed utf8"
            ))
        })?;
        Ok(Event::AddStory {
            peer_id: client_id,
            epic_id,
            story_name,
            story_description,
        })
    }

    /// Helper method for `try_create`. Attempts to create a `DeleteStory` variant.
    async fn try_create_delete_story(client_id: Uuid, tag: &[u8; 13]) -> Result<Event, DbError> {
        let epic_id = parse_4_bytes(&tag[1..5]);
        let story_id = parse_4_bytes(&tag[5..9]);
        Ok(Event::DeleteStory {
            peer_id: client_id,
            epic_id,
            story_id,
        })
    }

    /// Helper method for `try_create`. Attempts to create a `UpdateStoryStatus` variant.
    async fn try_create_update_story_status(
        client_id: Uuid,
        tag: &[u8; 13],
    ) -> Result<Event, DbError> {
        let epic_id = parse_4_bytes(&tag[1..5]);
        let story_id = parse_4_bytes(&tag[5..9]);
        let story_status = Status::try_from(tag[9])?;
        Ok(Event::UpdateStoryStatus {
            peer_id: client_id,
            epic_id,
            story_id,
            status: story_status,
        })
    }

    /// Associated function, creates an valid tag that will parse to an `AddEpic` variant
    /// from `epic_name` and `epic_description`
    pub fn add_epic_tag(epic_name: &String, epic_description: &String) -> [u8; 13] {
        let epic_name_bytes_len = epic_name.as_bytes().len();
        let epic_description_bytes_len = epic_description.as_bytes().len();
        let mut tag = [0; 13];
        tag[0] ^= 1;
        for i in 0..4 {
            tag[i + 1] ^= ((epic_name_bytes_len >> (i * 8)) & 0xff) as u8;
        }
        for i in 0..4 {
            tag[i + 5] ^= ((epic_description_bytes_len >> (i * 8)) & 0xff) as u8;
        }
        tag
    }

    /// Associated function, creates a tag that will parse to an `DeleteEpic` variant
    /// from `epic_id`
    pub fn delete_epic_tag(epic_id: u32) -> [u8; 13] {
        let mut tag = [0; 13];
        tag[0] ^= (1 << 1);
        for i in 0..4 {
            tag[i + 1] ^= ((epic_id >> (i as u32 * 8)) & 0xff) as u8;
        }
        tag
    }

    /// Associated function, creates a tag that will parse as an `GetEpic` variant
    /// from `epic_id`
    pub fn get_epic_tag(epic_id: u32) -> [u8; 13] {
        let mut tag = [0; 13];
        tag[0] ^= (1 << 2);
        for i in 0..4 {
            tag[i + 1] ^= ((epic_id >> (i as u32 * 8)) & 0xff) as u8;
        }
        tag
    }

    /// Associated function creates a tag that will parse as a `UpdateEpicStatus` variant
    /// from `epic_id` and `status`. Note that, `status` is assumed to be a valid `u8` representation
    /// of a `Status` i.e if `status` is passed as a parameter then `Status::try_from(status)` should
    /// not return an `Err`.
    pub fn get_update_epic_status_tag(epic_id: u32, status: u8) -> [u8; 13] {
        let mut tag = [0; 13];
        tag[0] ^= 1 << 3;
        for i in 0..4 {
            tag[i + 1] ^= ((epic_id >> (i as u32 * 8)) &0xff) as u8;
        }
        tag[5] = status;
        tag
    }

    /// Associated function, creates a tag that will parse as a `GetStory` variant from
    /// `epic_id` and `story_id`.
    pub fn get_story_tag(epic_id: u32, story_id: u32) -> [u8; 13] {
        let mut tag = [0; 13];
        tag[0] ^= 1 << 4;
        for i in 0..4 {
            tag[i + 1] ^= ((epic_id >> (i as u32 * 8)) & 0xff) as u8;
        }
        for i in 0..4 {
            tag[i + 5] ^= ((story_id >> (i as u32 * 8)) & 0xff) as u8;
        }
        tag
    }

    /// Associated function, creates a tag that will parse as a `AddStory` variant from
    /// `epic_id`, `story_name` and `story_description`.
    pub fn get_add_story_tag(epic_id: u32, story_name: &String, story_description: &String) -> [u8; 13] {
        let story_name_bytes_len = story_name.as_bytes().len();
        let story_description_bytes_len = story_description.as_bytes().len();
        let mut tag = [0; 13];
        tag[0] ^= 1 << 5;
        for i in 0.. 4 {
            tag[i + 1] ^= ((epic_id >> (i as u32 * 8)) & 0xff) as u8;
        }
        for i in 0 .. 4 {
            tag[i + 5] ^= ((story_name_bytes_len >> (i * 8)) & 0xff) as u8;
        }
        for i in 0..4 {
            tag[i + 9] ^= ((story_description_bytes_len >> (i * 8)) & 0xff) as u8;
        }
        tag
    }

    /// Associated function, creates a tag that will parse as a `DeleteStory` variant from
    /// `epic_id` and `story_id`.
    pub fn get_delete_story_tag(epic_id: u32, story_id: u32) -> [u8; 13] {
        let mut tag = [0; 13];
        tag[0] ^= 1 << 6;
        for i in 0..4 {
            tag[i + 1] ^= ((epic_id >> (i as u32 * 8)) &0xff) as u8;
        }
        for i in 0..4 {
            tag[i + 5] ^= ((story_id >> (i as u32 * 8)) & 0xff) as u8;
        }
        tag
    }

    /// Associated function, creates a tag that will parse as a `UpdateStoryStatus` variant from
    /// `epic_id`, `story_id` and `status`. Note that, `status` is assumed to be a valid `u8` representation
    /// of a `Status` i.e if `status` is passed as a parameter then `Status::try_from(status)` should
    /// not return an `Err`.
    pub fn get_update_story_status_tag(epic_id: u32, story_id: u32, status: u8) -> [u8; 13] {
        let mut tag = [0; 13];
        tag[0] ^= 1 << 7;
        for i in 0..4 {
            tag[i + 1] ^= ((epic_id >> (i as u32 * 8)) & 0xff) as u8;
        }
        for i in 0..4 {
            tag[i + 5] ^= ((story_id >> (i as u32 * 8)) & 0xff) as u8;
        }
        tag[9] = status;
        tag
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use uuid::Uuid;
    use crate::models::{Epic, Story, Status};
    use crate::utils::BytesEncode;
    use std::collections::HashMap;
    use std::path::PathBuf;
    use async_std::io::Cursor;
    use async_std::task::block_on;


    #[test]
    fn test_add_epic_tag() {
        let epic_name = "Test Epic".to_string();
        let epic_description = "A good test epic".to_string();
        let tag = Event::add_epic_tag(&epic_name, &epic_description);
        println!("{:?}", tag);
        assert_eq!(tag, [1, 9, 0, 0, 0, 16, 0, 0, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn test_try_create_add_epic() {
        let epic = Epic::new(0, String::from("A Test Epic"), String::from("A good test epic"), Status::Open, PathBuf::new(), HashMap::new());
        let mut bytes = vec![];
        let tag = Event::add_epic_tag(epic.name(), epic.description());
        let test_client_id = Uuid::new_v4();

        bytes.extend_from_slice(epic.name().as_bytes());
        bytes.extend_from_slice(epic.description().as_bytes());
        let mut stream = Cursor::new(bytes);

        let add_epic_event_res = block_on(Event::try_create_add_epic(test_client_id.clone(), &tag, &mut stream));
        println!("{:?}", add_epic_event_res);

        assert!(add_epic_event_res.is_ok());
        let add_epic_event = add_epic_event_res.unwrap();
        let Event::AddEpic { peer_id, epic_name, epic_description } = add_epic_event else { panic!("epic values should have been parsed correctly") };

        assert_eq!(peer_id, test_client_id);
        assert_eq!(epic_name, epic.name().as_str().to_owned());
        assert_eq!(epic_description, epic.description().as_str().to_owned());
    }

    #[test]
    fn test_delete_epic_tag() {
        let epic_id = 93;
        let tag = Event::delete_epic_tag(epic_id);
        println!("{:?}", tag);
        assert_eq!(tag, [2, 93, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn test_try_create_delete_epic_event() {
        let test_epic_id = 93;
        let tag = Event::delete_epic_tag(test_epic_id);
        let test_client_id = Uuid::new_v4();

        let delete_epic_event_res = block_on(Event::try_create_delete_epic(test_client_id, &tag));

        println!("{:?}", delete_epic_event_res);
        assert!(delete_epic_event_res.is_ok());

        let Event::DeleteEpic { peer_id, epic_id } = delete_epic_event_res.unwrap() else { panic!("event should have been parsed correctly") };

        assert_eq!(peer_id, test_client_id);
        assert_eq!(epic_id, test_epic_id);
    }

    #[test]
    fn test_get_epic_tag() {
        let test_epic_id = 2353;
        let tag = Event::get_epic_tag(test_epic_id);
        println!("{:?}", tag);
        assert_eq!(tag, [4, 49, 9, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])
    }

    #[test]
    fn test_try_create_get_epic_event() {
        let test_epic_id = 2353;
        let tag = Event::get_epic_tag(test_epic_id);
        let test_client_id = Uuid::new_v4();

        let get_epic_event_res = block_on(Event::try_create_get_epic(test_client_id, &tag));

        println!("{:?}", get_epic_event_res);
        assert!(get_epic_event_res.is_ok());

        let Event::GetEpic { peer_id, epic_id} = get_epic_event_res.unwrap() else { panic!("unable to parse event correctly") };

        assert_eq!(peer_id, test_client_id);
        assert_eq!(epic_id, test_epic_id);
    }

    #[test]
    fn test_get_updated_epic_status_tag() {
        let test_epic_id = 2353;
        let status = 1;
        let tag = Event::get_update_epic_status_tag(test_epic_id, status);

        println!("{:?}", tag);
        assert_eq!(tag, [8, 49, 9, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn test_try_create_update_epic_status_event() {
        let test_epic_id = 2353;
        let test_status = 1;
        let tag = Event::get_update_epic_status_tag(test_epic_id, test_status);
        let test_client_id = Uuid::new_v4();

        let update_epic_status_event_res = block_on(Event::try_create_update_epic_status(test_client_id, &tag));

        println!("{:?}", update_epic_status_event_res);
        assert!(update_epic_status_event_res.is_ok());

        let Event::UpdateEpicStatus { peer_id, epic_id, status} = update_epic_status_event_res.unwrap() else { panic!("unable t parse event correctly") };

        assert_eq!(peer_id, test_client_id);
        assert_eq!(epic_id, test_epic_id);
        assert_eq!(status, Status::try_from(test_status).expect("status should be created successfully"));
    }

    #[test]
    fn test_get_story_tag() {
        let test_epic_id = 2353;
        let test_story_id = 4798;
        let tag = Event::get_story_tag(test_epic_id, test_story_id);

        println!("{:?}", tag);
        assert_eq!(tag, [16, 49, 9, 0, 0, 190, 18, 0, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn test_try_create_get_story_event() {
        let test_epic_id = 2353;
        let test_story_id = 4798;
        let tag = Event::get_story_tag(test_epic_id, test_story_id);
        let test_client_id = Uuid::new_v4();

        let get_story_event_res = block_on(Event::try_create_get_story(test_client_id, &tag));

        println!("{:?}", get_story_event_res);
        assert!(get_story_event_res.is_ok());

        let Event::GetStory { peer_id, epic_id, story_id } = get_story_event_res.unwrap() else { panic!("unable to parse event correctly") };

        assert_eq!(peer_id, test_client_id);
        assert_eq!(epic_id, test_epic_id);
        assert_eq!(story_id, test_story_id);
    }

    #[test]
    fn test_get_add_story_tag() {
        let test_epic_id = 2353;
        let story_name = String::from("Test Story");
        let story_description = String::from("A good test Story");
        let tag = Event::get_add_story_tag(test_epic_id, &story_name, &story_description);

        println!("{:?}", tag);
        assert_eq!(tag, [32, 49, 9, 0, 0, 10, 0, 0, 0, 17, 0, 0, 0]);
    }

    #[test]
    fn test_try_create_add_story_event() {
        let test_epic_id = 2353;
        let test_story_name = String::from("Test Story");
        let test_story_description = String::from("A good test Story");
        let tag = Event::get_add_story_tag(test_epic_id, &test_story_name, &test_story_description);
        let test_client_id = Uuid::new_v4();
        let mut bytes = test_story_name.as_bytes().to_vec();
        bytes.extend_from_slice(test_story_description.as_bytes());
        let mut cursor = Cursor::new(bytes);

        let add_story_event_res = block_on(Event::try_create_add_story(test_client_id, &tag, &mut cursor));

        println!("{:?}", add_story_event_res);
        assert!(add_story_event_res.is_ok());

        let Event::AddStory { peer_id, epic_id, story_name, story_description} = add_story_event_res.unwrap() else { panic!("unable to parse event correctly") };

        assert_eq!(peer_id, test_client_id);
        assert_eq!(epic_id, test_epic_id);
        assert_eq!(story_name, test_story_name);
        assert_eq!(story_description, test_story_description);
    }

    #[test]
    fn test_get_delete_story_tag() {
        todo!()
    }

    #[test]
    fn test_try_create_delete_story_event() {
        todo!()
    }

    #[test]
    fn test_get_update_story_status_tag() {
        todo!()
    }

    #[test]
    fn test_try_create_update_story_status_event() {
        todo!()
    }
}
