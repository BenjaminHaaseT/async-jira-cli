//! Collection of Structs and functions for responses sent from the server to the client
use crate::models::{BytesEncode, TagDecoding, TagEncoding};
use crate::utils::parse_4_bytes;

pub mod prelude {
    pub use super::*;
}

/// A response to an `Event` sent by a client.
pub enum Response {
    /// Response for a successful client connection, holds the database
    /// `Epics` encoded in a `Vec<u8`
    ClientAddedOk(Vec<u8>),

    /// Response for an unsuccessful client connection
    ClientAlreadyExists,

    /// Response of a successful addition of an `Epic` to the database, holds the epic id
    /// and the encoded database epic's in a `u32` and `Vec<u8>`
    AddedEpicOk(u32, Vec<u8>),

    /// Response for a successful deletion of an `Epic`, holds the epic id of the deleted `Epic`
    /// and the encoded state of database as a `Vec<u8>`
    DeletedEpicOk(u32, Vec<u8>),

    /// Response for successful retrieval of an `Epic`, holds
    /// the encoded data of the epic in a `Vec<u8>`
    GetEpicOk(Vec<u8>),

    /// Response for a successful status update for a particular `Epic`, holds the id of
    /// the updated epic and its encoded data in `u32` and `Vec<u8>` respectively
    EpicStatusUpdateOk(u32, Vec<u8>),

    /// Response for an unsuccessful retrieval of an `Epic`, holds the id of the epic in a `u32` and
    /// the database encoded as a `Vec<u8>`
    EpicDoesNotExist(u32, Vec<u8>),

    /// Response for a successful retrieval of a `Story`, holds the
    /// encoded data of the story in a `Vec<u8>`
    GetStoryOk(Vec<u8>),

    /// Response for an unsuccessful retrieval of a `Story`, holds the epic id and story id
    StoryDoesNotExist(u32, u32, Vec<u8>),

    /// Response for a successful addition of a `Story`, holds the epic id,
    /// story id and the epic encoded as (`u32`, `u32`, `Vec<u8>`)
    AddedStoryOk(u32, u32, Vec<u8>),

    /// Response for a successful deletion of a `Story`, holds the epic id,
    /// story id and the epic encoded as (`u32`, `u32`, `Vec<u8>`)
    DeletedStoryOk(u32, u32, Vec<u8>),

    /// Response for successful update of `Story` status, holds the epic id the contains
    /// the story, the story id and the epic encoded as (`u32`, `u32`, `Vec<u8>`)
    StoryStatusUpdateOk(u32, u32, Vec<u8>),

    /// Response for any event that was unable to be parsed correctly
    RequestNotParsed
}

impl Response {
    /// Method that will convert a `Response` into an encoded slice of bytes
    pub fn as_bytes(&self) -> &[u8] {
        todo!()
    }
}

type ResponseEncodeTag = [u8; 10];

impl TagEncoding for ResponseEncodeTag {}

type ResponseDecodeTag = (u16, u32, u32);

impl TagDecoding for ResponseDecodeTag {}

impl BytesEncode for Response {
    type Tag = ResponseEncodeTag;

    type DecodedTag = ResponseDecodeTag;

    fn encode(&self) -> Self::Tag {
        let mut bytes = [0u8; 10];
        match self {
            Response::ClientAddedOk(_data) => {
                bytes[0] ^= (1 << 7);
                bytes[1] ^= 1;
                bytes
            }
            Response::ClientAlreadyExists => {
                bytes[1] ^= (1 << 1);
                bytes
            }
            Response::AddedEpicOk(epic_id,_data) => {
                bytes[0] ^= (1 << 7);
                bytes[1] ^= (1 << 2);
                for i in 2..6u32 {
                    bytes[i as usize] ^= (((*epic_id) >> (8 * (i - 2))) & 0xff) as u8;
                }
                bytes
            }
            Response::DeletedEpicOk(epic_id, _data) => {
                bytes[0] ^= (1 << 7);
                bytes[1] ^= (1 << 3);
                for i in 2..6u32 {
                    bytes[i as usize] ^= (((*epic_id) >> (8 * (i - 2))) &0xff) as u8;
                }
                bytes
            }
            Response::GetEpicOk(_data) => {
                bytes[0] ^= (1 << 7);
                bytes[1] ^= (1 << 4);
                bytes
            }
            Response::EpicStatusUpdateOk(epic_id, _data) => {
                bytes[0] ^= (1 << 7);
                bytes[1] ^= (1 << 5);
                for i in 2..6u32 {
                    bytes[i as usize] ^= (((*epic_id) >> (8 * (i - 2))) & 0xff) as u8;
                }
                bytes
            }
            Response::EpicDoesNotExist(epic_id, _data) => {
                bytes[0] ^= (1 << 7);
                bytes[1] ^= (1 << 6);
                for i in 2..6u32 {
                    bytes[i as usize] ^= (((*epic_id) >> (8 * (i - 2))) & 0xff) as u8;
                }
                bytes
            }
            Response::GetStoryOk(_data) => {
                bytes[0] ^= (1 << 7);
                bytes[1] ^= (1 << 7);
                bytes
            }
            Response::StoryDoesNotExist(epic_id, story_id,_data) => {
                bytes[0] ^= (1 << 7);
                bytes[0] ^= 1;
                for i in 2..6u32 {
                    bytes[i as usize] ^= (((*epic_id) >> (8 * (i - 2))) & 0xff) as u8;
                }
                for i in 6..10u32 {
                    bytes[i as usize] ^= (((*story_id) >> (8 * (i - 2))) & 0xff) as u8;
                }
                bytes
            }
            Response::AddedStoryOk(epic_id, story_id, _data) => {
                bytes[0] ^= (1 << 7);
                bytes[0] ^= (1 << 1);
                for i in 2..6u32 {
                    bytes[i as usize] ^= (((*epic_id) >> (8 * (i - 2))) & 0xff) as u8;
                }
                for i in 6..10u32 {
                    bytes[i as usize] ^= (((*story_id) >> (8 * (i - 2))) & 0xff) as u8;
                }
                bytes
            }
            Response::DeletedStoryOk(epic_id, story_id, _data) => {
                bytes[0] ^= (1 << 7);
                bytes[0] ^= (1 << 2);
                for i in 2..6u32 {
                    bytes[i as usize] ^= (((*epic_id) >> (8 * (i - 2))) & 0xff) as u8;
                }
                for i in 6..10u32 {
                    bytes[i as usize] ^= (((*story_id) >> (8 * (i - 2))) & 0xff) as u8;
                }
                bytes
            }
            Response::StoryStatusUpdateOk(epic_id, story_id, _data) => {
                bytes[0] ^= (1 << 7);
                bytes[0] ^= (1 << 3);
                for i in 2..6u32 {
                    bytes[i as usize] ^= (((*epic_id) >> (8 * (i - 2))) & 0xff) as u8;
                }
                for i in 6..10u32 {
                    bytes[i as usize] ^= (((*story_id) >> (8 * (i - 2))) & 0xff) as u8;
                }
                bytes
            }
            Response::RequestNotParsed => {
                bytes[0] ^= (1 << 4);
                bytes
            }
        }
    }

    fn decode(tag: Self::Tag) -> Self::DecodedTag {
        let mut type_and_flag_bytes = 0u16;
        let mut epic_id = 0u32;
        let mut story_id = 0u32;
        type_and_flag_bytes ^= (tag[0] << 8) as u16;
        type_and_flag_bytes ^= tag[1] as u16;
        if type_and_flag_bytes & (1 << 2) != 0 || type_and_flag_bytes & (1 << 3) != 0
            || type_and_flag_bytes & (1 << 5) != 0 || type_and_flag_bytes & (1 << 6) != 0
            || type_and_flag_bytes & (1 << 8) != 0 || type_and_flag_bytes & (1 << 9) != 0
            || type_and_flag_bytes & (1 << 10) != 0 || type_and_flag_bytes & (1 << 10) != 0
            || type_and_flag_bytes & (1 << 11) != 0

        {
            epic_id = parse_4_bytes(&tag[2..6]);
            if type_and_flag_bytes & (1 << 8) != 0 || type_and_flag_bytes & (1 << 9) != 0
                || type_and_flag_bytes & (1 << 10) != 0 || type_and_flag_bytes & (1 << 11) != 0
            {
                story_id = parse_4_bytes(&tag[6..]);
            }
        }
        (type_and_flag_bytes, epic_id, story_id)
    }
}