//! Collection of Structs and functions for responses sent from the server to the client
use crate::models::DbError;
use crate::utils::{parse_4_bytes, AsBytes, BytesEncode, TagDecoding, TagEncoding};
use async_std::net::TcpStream;
use tracing::instrument;

pub mod prelude {
    pub use super::*;
}

/// A response to an `Event` sent by a client.
#[derive(Debug)]
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
    GetEpicOk(u32, Vec<u8>),

    /// Response for a successful status update for a particular `Epic`, holds the id of
    /// the updated epic and its encoded data in `u32` and `Vec<u8>` respectively
    EpicStatusUpdateOk(u32, Vec<u8>),

    /// Response for an unsuccessful retrieval of an `Epic`, holds the id of the epic in a `u32` and
    /// the database encoded as a `Vec<u8>`
    EpicDoesNotExist(u32, Vec<u8>),

    /// Response for a successful retrieval of a `Story`, holds the
    /// encoded data of the story in a `Vec<u8>`
    GetStoryOk(u32, Vec<u8>),

    /// Response for an unsuccessful retrieval of a `Story`, holds the epic id and story id and the
    /// `Epic` encoded as a `Vec<u8>` that was queried
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
    RequestNotParsed,
}

impl Response {
    pub async fn try_create(tag: [u8; 10], stream: &TcpStream) -> Result<Response, DbError> {
        todo!()
    }

    /// Associated helper method to aid in serialization/deserialization of a `Response`.
    /// Takes a `ResponseEncodeTag`, `buf` and a vector of bytes `data`
    /// and serializes the length of `data` into `buf`.
    fn encode_data_len(buf: &mut [u8; 18], data: &Vec<u8>) {
        let data_len = data.len() as u64;
        for i in 0..7 {
            buf[i + 10] ^= ((data_len >> (i as u64 * 8)) & 0xff) as u8;
        }
    }

    /// Associated helper method to aid in serialization/deserialization of a `Response`.
    /// Takes a `ResponseEncodeTag`, `buf` and a `u32` , `epic_id` and serializes `epic_id`
    /// into `buf`.
    fn encode_epic_id(buf: &mut [u8; 18], epic_id: u32) {
        for i in 2..6 {
            buf[i] ^= ((epic_id >> (8 * (i as u32 - 2))) & 0xff) as u8;
        }
    }

    /// Associated helper method to aid in serialization/deserialization of a `Response`.
    /// Takes a `ResponseEncodeTag`, `buf` and a `u32`, `story_id` and serializes `story_id` into
    /// `buf`.
    fn encode_story_id(buf: &mut [u8; 18], story_id: u32) {
        for i in 6..10 {
            buf[i] ^= ((story_id >> (8 * ((i as u32 - 2) % 4))) & 0xff) as u8;
        }
    }
}

impl AsBytes for Response {
    /// Converts `Response` into a serialized vector of bytes.
    #[instrument(name = "response as bytes", fields(response = ?self))]
    fn as_bytes(&self) -> Vec<u8> {
        let mut bytes = vec![];
        bytes.extend_from_slice(&self.encode());
        match self {
            Response::ClientAddedOk(data) => bytes.extend_from_slice(data.as_slice()),
            Response::ClientAlreadyExists => {}
            Response::AddedEpicOk(_, data) => bytes.extend_from_slice(data.as_slice()),
            Response::DeletedEpicOk(_, data) => bytes.extend_from_slice(data.as_slice()),
            Response::GetEpicOk(_,data) => bytes.extend_from_slice(data.as_slice()),
            Response::EpicStatusUpdateOk(_, data) => bytes.extend_from_slice(data.as_slice()),
            Response::EpicDoesNotExist(_, data) => bytes.extend_from_slice(data.as_slice()),
            Response::GetStoryOk(_, data) => bytes.extend_from_slice(data.as_slice()),
            Response::StoryDoesNotExist(_, _, data) => bytes.extend_from_slice(data.as_slice()),
            Response::AddedStoryOk(_, _, data) => bytes.extend_from_slice(data.as_slice()),
            Response::DeletedStoryOk(_, _, data) => bytes.extend_from_slice(data.as_slice()),
            Response::StoryStatusUpdateOk(_, _, data) => bytes.extend_from_slice(data.as_slice()),
            Response::RequestNotParsed => {}
        }
        bytes
    }
}

/// The type of tag use to serialize a `Response` into
type ResponseEncodeTag = [u8; 18];

impl TagEncoding for ResponseEncodeTag {}

/// The type of tag use to deserialize a `Response` from
type ResponseDecodeTag = (u16, u32, u32, u64);

impl TagDecoding for ResponseDecodeTag {}

impl BytesEncode for Response {
    type Tag = ResponseEncodeTag;

    type DecodedTag = ResponseDecodeTag;

    #[instrument(name = "serializing response tag", fields(response = ?self))]
    fn encode(&self) -> Self::Tag {
        let mut encoded_tag = [0u8; 18];
        match self {
            Response::ClientAddedOk(data) => {
                encoded_tag[0] ^= (1 << 7);
                encoded_tag[1] ^= 1;
                Response::encode_data_len(&mut encoded_tag, data);
                encoded_tag
            }
            Response::ClientAlreadyExists => {
                encoded_tag[1] ^= (1 << 1);
                encoded_tag
            }
            Response::AddedEpicOk(epic_id, data) => {
                encoded_tag[0] ^= (1 << 7);
                encoded_tag[1] ^= (1 << 2);
                Response::encode_epic_id(&mut encoded_tag, *epic_id);
                Response::encode_data_len(&mut encoded_tag, data);
                encoded_tag
            }
            Response::DeletedEpicOk(epic_id, data) => {
                encoded_tag[0] ^= (1 << 7);
                encoded_tag[1] ^= (1 << 3);
                Response::encode_epic_id(&mut encoded_tag, *epic_id);
                Response::encode_data_len(&mut encoded_tag, data);
                encoded_tag
            }
            Response::GetEpicOk(epic_id, data) => {
                encoded_tag[0] ^= (1 << 7);
                encoded_tag[1] ^= (1 << 4);
                Response::encode_epic_id(&mut encoded_tag, *epic_id);
                Response::encode_data_len(&mut encoded_tag, data);
                encoded_tag
            }
            Response::EpicStatusUpdateOk(epic_id, data) => {
                encoded_tag[0] ^= (1 << 7);
                encoded_tag[1] ^= (1 << 5);
                Response::encode_epic_id(&mut encoded_tag, *epic_id);
                Response::encode_data_len(&mut encoded_tag, data);
                encoded_tag
            }
            Response::EpicDoesNotExist(epic_id, data) => {
                encoded_tag[0] ^= (1 << 7);
                encoded_tag[1] ^= (1 << 6);
                Response::encode_epic_id(&mut encoded_tag, *epic_id);
                Response::encode_data_len(&mut encoded_tag, data);
                encoded_tag
            }
            Response::GetStoryOk(story_id, data) => {
                encoded_tag[0] ^= (1 << 7);
                encoded_tag[1] ^= (1 << 7);
                Response::encode_story_id(&mut encoded_tag, *story_id);
                Response::encode_data_len(&mut encoded_tag, data);
                encoded_tag
            }
            Response::StoryDoesNotExist(epic_id, story_id, data) => {
                encoded_tag[0] ^= (1 << 7);
                encoded_tag[0] ^= 1;
                Response::encode_epic_id(&mut encoded_tag, *epic_id);
                Response::encode_story_id(&mut encoded_tag, *story_id);
                Response::encode_data_len(&mut encoded_tag, data);
                encoded_tag
            }
            Response::AddedStoryOk(epic_id, story_id, data) => {
                encoded_tag[0] ^= (1 << 7);
                encoded_tag[0] ^= (1 << 1);

                Response::encode_epic_id(&mut encoded_tag, *epic_id);
                Response::encode_story_id(&mut encoded_tag, *story_id);
                Response::encode_data_len(&mut encoded_tag, data);
                encoded_tag
            }
            Response::DeletedStoryOk(epic_id, story_id, data) => {
                encoded_tag[0] ^= (1 << 7);
                encoded_tag[0] ^= (1 << 2);
                Response::encode_epic_id(&mut encoded_tag, *epic_id);
                Response::encode_story_id(&mut encoded_tag, *story_id);
                Response::encode_data_len(&mut encoded_tag, data);
                encoded_tag
            }
            Response::StoryStatusUpdateOk(epic_id, story_id, data) => {
                encoded_tag[0] ^= (1 << 7);
                encoded_tag[0] ^= (1 << 3);
                Response::encode_epic_id(&mut encoded_tag, *epic_id);
                Response::encode_story_id(&mut encoded_tag, *story_id);
                Response::encode_data_len(&mut encoded_tag, data);
                encoded_tag
            }
            Response::RequestNotParsed => {
                encoded_tag[0] ^= (1 << 4);
                encoded_tag
            }
        }
    }

    #[instrument(name = "deserializing response tag")]
    fn decode(tag: &Self::Tag) -> Self::DecodedTag {
        let mut type_and_flag_bytes = 0u16;
        let mut epic_id = 0u32;
        let mut story_id = 0u32;
        let mut data_len = 0u64;
        type_and_flag_bytes ^= (tag[0] as u16) << 8;
        type_and_flag_bytes ^= tag[1] as u16;
        if type_and_flag_bytes & (1 << 2) != 0
            || type_and_flag_bytes & (1 << 3) != 0
            || type_and_flag_bytes & (1 << 4) != 0
            || type_and_flag_bytes & (1 << 5) != 0
            || type_and_flag_bytes & (1 << 6) != 0
            || type_and_flag_bytes & (1 << 8) != 0
            || type_and_flag_bytes & (1 << 9) != 0
            || type_and_flag_bytes & (1 << 10) != 0
            || type_and_flag_bytes & (1 << 10) != 0
            || type_and_flag_bytes & (1 << 11) != 0
        {
            epic_id = parse_4_bytes(&tag[2..6]);
        }

        if type_and_flag_bytes & (1 << 7) != 0
            || type_and_flag_bytes & (1 << 8) != 0
            || type_and_flag_bytes & (1 << 9) != 0
            || type_and_flag_bytes & (1 << 10) != 0
            || type_and_flag_bytes & (1 << 11) != 0
        {
            story_id = parse_4_bytes(&tag[6..10]);
        }
        if type_and_flag_bytes & (1 << 15) != 0 {
            for i in 0..8 {
                data_len ^= (tag[i + 10] as u64) << ((i as u64) * 8);
            }
        }
        (type_and_flag_bytes, epic_id, story_id, data_len)
    }

}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_response_encode() {
        let response = Response::ClientAddedOk(vec![1, 2, 3, 4, 5]);
        let encoding = response.encode();
        println!("{:?}", encoding);
        assert_eq!(encoding, [128, 1, 0, 0, 0, 0, 0, 0, 0, 0, 5, 0, 0, 0, 0, 0, 0, 0]);

        let response = Response::ClientAlreadyExists;
        let encoding = response.encode();
        println!("{:?}", encoding);
        assert_eq!(encoding, [0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);

        let response = Response::AddedEpicOk(2353, vec![1, 2, 3, 4, 5]);
        let encoding = response.encode();
        println!("{:?}", encoding);
        assert_eq!(encoding, [128, 4, 49, 9, 0, 0, 0, 0, 0, 0, 5, 0, 0, 0, 0, 0, 0, 0]);

        let response = Response::DeletedEpicOk(2353, vec![1, 2, 3, 4, 5]);
        let encoding = response.encode();
        println!("{:?}", encoding);
        assert_eq!(encoding, [128, 8, 49, 9, 0, 0, 0, 0, 0, 0, 5, 0, 0, 0, 0, 0, 0, 0]);

        let response = Response::GetEpicOk(2353, vec![1, 2, 3, 4, 5]);
        let encoding = response.encode();
        println!("{:?}", encoding);
        assert_eq!(encoding, [128, 16, 49, 9, 0, 0, 0, 0, 0, 0, 5, 0, 0, 0, 0, 0, 0, 0]);

        let response = Response::EpicStatusUpdateOk(2353, vec![1, 2, 3, 4, 5]);
        let encoding = response.encode();
        println!("{:?}", encoding);
        assert_eq!(encoding, [128, 32, 49, 9, 0, 0, 0, 0, 0, 0, 5, 0, 0, 0, 0, 0, 0, 0]);

        let response = Response::EpicDoesNotExist(2353, vec![1, 2, 3, 4, 5]);
        let encoding = response.encode();
        println!("{:?}", encoding);
        assert_eq!(encoding, [128, 64, 49, 9, 0, 0, 0, 0, 0, 0, 5, 0, 0, 0, 0, 0, 0, 0]);

        let response = Response::GetStoryOk(4798,vec![1, 2, 3, 4, 5]);
        let encoding = response.encode();
        println!("{:?}", encoding);
        assert_eq!(encoding, [128, 128, 0, 0, 0, 0, 190, 18, 0, 0, 5, 0, 0, 0, 0, 0, 0, 0]);

        let response = Response::StoryDoesNotExist(2353, 4798, vec![1, 2, 3, 4, 5]);
        let encoding = response.encode();
        println!("{:?}", encoding);
        assert_eq!(encoding, [129, 0, 49, 9, 0, 0, 190, 18, 0, 0, 5, 0, 0, 0, 0, 0, 0, 0]);

        let response = Response::AddedStoryOk(2353, 4798, vec![1, 2, 3, 4, 5]);
        let encoding = response.encode();
        println!("{:?}", encoding);
        assert_eq!(encoding, [130, 0, 49, 9, 0, 0, 190, 18, 0, 0, 5, 0, 0, 0, 0, 0, 0, 0]);

        let response = Response::DeletedStoryOk(2353, 4798, vec![1, 2, 3, 4, 5]);
        let encoding = response.encode();
        println!("{:?}", encoding);
        assert_eq!(encoding, [132, 0, 49, 9, 0, 0, 190, 18, 0, 0, 5, 0, 0, 0, 0, 0, 0, 0]);

        let response = Response::StoryStatusUpdateOk(2353, 4798, vec![1, 2, 3, 4, 5]);
        let encoding = response.encode();
        println!("{:?}", encoding);
        assert_eq!(encoding, [136, 0, 49, 9, 0, 0, 190, 18, 0, 0, 5, 0, 0, 0, 0, 0, 0, 0]);

        let response = Response::RequestNotParsed;
        let encoding = response.encode();
        println!("{:?}", encoding);
        assert_eq!(encoding, [16, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn test_response_decode() {
        let response = Response::ClientAddedOk(vec![]);
        let encoding = response.encode();
        assert_eq!(encoding, [128, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        let decoding = Response::decode(&encoding);
        println!("{:?}", decoding);
        assert_eq!(decoding, (32769, 0, 0, 0));
        println!("{:016b}", decoding.0);

        let response = Response::ClientAlreadyExists;
        let encoding = response.encode();
        assert_eq!(encoding, [0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        let decoding = Response::decode(&encoding);
        println!("{:?}", decoding);
        assert_eq!(decoding, (2, 0, 0, 0));
        println!("{:016b}", decoding.0);

        let response = Response::AddedEpicOk(2353, vec![1, 2, 3, 4, 5]);
        let encoding = response.encode();
        assert_eq!(encoding, [128, 4, 49, 9, 0, 0, 0, 0, 0, 0, 5, 0, 0, 0, 0, 0, 0, 0]);
        let decoding = Response::decode(&encoding);
        println!("{:?}", decoding);
        assert_eq!(decoding, (32772, 2353, 0, 5));
        println!("{:016b}", decoding.0);

        let response = Response::DeletedEpicOk(2353, vec![1, 2, 3, 4, 5]);
        let encoding = response.encode();
        assert_eq!(encoding, [128, 8, 49, 9, 0, 0, 0, 0, 0, 0, 5, 0, 0, 0, 0, 0, 0, 0]);
        let decoding = Response::decode(&encoding);
        println!("{:?}", decoding);
        assert_eq!(decoding, (32776, 2353, 0, 5));
        println!("{:016b}", decoding.0);

        let response = Response::GetEpicOk(2353, vec![1, 2, 3, 4, 5]);
        let encoding = response.encode();
        assert_eq!(encoding, [128, 16, 49, 9, 0, 0, 0, 0, 0, 0, 5, 0, 0, 0, 0, 0, 0, 0]);
        let decoding = Response::decode(&encoding);
        println!("{:?}", decoding);
        assert_eq!(decoding, (32784, 2353, 0, 5));
        println!("{:016b}", decoding.0);

        let response = Response::EpicStatusUpdateOk(2353, vec![1, 2, 3, 4, 5]);
        let encoding = response.encode();
        assert_eq!(encoding, [128, 32, 49, 9, 0, 0, 0, 0, 0, 0, 5, 0, 0, 0, 0, 0, 0, 0]);
        let decoding = Response::decode(&encoding);
        println!("{:?}", decoding);
        assert_eq!(decoding, (32800, 2353, 0, 5));
        println!("{:016b}", decoding.0);

        let response = Response::EpicDoesNotExist(2353, vec![1, 2, 3, 4, 5]);
        let encoding = response.encode();
        assert_eq!(encoding, [128, 64, 49, 9, 0, 0, 0, 0, 0, 0, 5, 0, 0, 0, 0, 0, 0, 0]);
        let decoding = Response::decode(&encoding);
        println!("{:?}", decoding);
        assert_eq!(decoding, (32832, 2353, 0, 5));
        println!("{:016b}", decoding.0);

        let response = Response::GetStoryOk(4798, vec![1, 2, 3, 4, 5]);
        let encoding = response.encode();
        assert_eq!(encoding, [128, 128, 0, 0, 0, 0, 190, 18, 0, 0, 5, 0, 0, 0, 0, 0, 0, 0]);
        let decoding = Response::decode(&encoding);
        println!("{:?}", decoding);
        assert_eq!(decoding, (32896, 0, 4798, 5));
        println!("{:016b}", decoding.0);

        let response = Response::StoryDoesNotExist(2353, 4798, vec![1, 2, 3, 4, 5]);
        let encoding = response.encode();
        println!("{:?}", encoding);
        assert_eq!(encoding, [129, 0, 49, 9, 0, 0, 190, 18, 0, 0, 5, 0, 0, 0, 0, 0, 0, 0]);
        let decoding = Response::decode(&encoding);
        println!("{:?}", decoding);
        assert_eq!(decoding, (33024, 2353, 4798, 5));
        println!("{:016b}", decoding.0);

        let response = Response::AddedStoryOk(2353, 4798, vec![1, 2, 3, 4, 5]);
        let encoding = response.encode();
        assert_eq!(encoding, [130, 0, 49, 9, 0, 0, 190, 18, 0, 0, 5, 0, 0, 0, 0, 0, 0, 0]);
        let decoding = Response::decode(&encoding);
        println!("{:?}", decoding);
        assert_eq!(decoding, (33280, 2353, 4798, 5));
        println!("{:016b}", decoding.0);

        let response = Response::DeletedStoryOk(2353, 4798, vec![1, 2, 3, 4, 5]);
        let encoding = response.encode();
        assert_eq!(encoding, [132, 0, 49, 9, 0, 0, 190, 18, 0, 0, 5, 0, 0, 0, 0, 0, 0, 0]);
        let decoding = Response::decode(&encoding);
        println!("{:?}", decoding);
        assert_eq!(decoding, (33792, 2353, 4798, 5));
        println!("{:016b}", decoding.0);

        let response = Response::StoryStatusUpdateOk(2353, 4798, vec![1, 2, 3, 4, 5]);
        let encoding = response.encode();
        assert_eq!(encoding, [136, 0, 49, 9, 0, 0, 190, 18, 0, 0, 5, 0, 0, 0, 0, 0, 0, 0]);
        let decoding = Response::decode(&encoding);
        println!("{:?}", decoding);
        assert_eq!(decoding, (34816, 2353, 4798, 5));
        println!("{:016b}", decoding.0);

        let response = Response::RequestNotParsed;
        let encoding = response.encode();
        assert_eq!(encoding, [16, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        let decoding = Response::decode(&encoding);
        println!("{:?}", decoding);
        assert_eq!(decoding, (4096, 0, 0, 0));
        println!("{:016b}", decoding.0);
    }

    #[test]
    fn test_response_as_bytes() {
        let response = Response::ClientAddedOk(vec![1, 2, 3]);
        let bytes = response.as_bytes();
        println!("{:?}", bytes);
        assert_eq!(bytes, vec![128, 1, 0, 0, 0, 0, 0, 0, 0, 0, 3, 0, 0, 0, 0, 0, 0, 0, 1, 2, 3]);

        let response = Response::ClientAlreadyExists;
        let bytes = response.as_bytes();
        println!("{:?}", bytes);
        assert_eq!(bytes, vec![0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,]);

        let response = Response::AddedEpicOk(2353, vec![1, 2, 3]);
        let bytes = response.as_bytes();
        println!("{:?}", bytes);
        assert_eq!(bytes, vec![128, 4, 49, 9, 0, 0, 0, 0, 0, 0, 3, 0, 0, 0, 0, 0, 0, 0, 1, 2, 3]);

        let response = Response::DeletedEpicOk(2353, vec![1, 2, 3]);
        let bytes = response.as_bytes();
        println!("{:?}", bytes);
        assert_eq!(bytes, vec![128, 8, 49, 9, 0, 0, 0, 0, 0, 0, 3, 0, 0, 0, 0, 0, 0, 0, 1, 2, 3]);

        let response = Response::GetEpicOk(2353, vec![1, 2, 3]);
        let bytes = response.as_bytes();
        println!("{:?}", bytes);
        assert_eq!(bytes, [128, 16, 49, 9, 0, 0, 0, 0, 0, 0, 3, 0, 0, 0, 0, 0, 0, 0, 1, 2, 3]);

        let response = Response::EpicStatusUpdateOk(2353, vec![1, 2, 3]);
        let bytes = response.as_bytes();
        println!("{:?}", bytes);
        assert_eq!(bytes, [128, 32, 49, 9, 0, 0, 0, 0, 0, 0, 3, 0, 0, 0, 0, 0, 0, 0, 1, 2, 3]);

        let response = Response::EpicDoesNotExist(2353, vec![1, 2, 3, 4]);
        let bytes = response.as_bytes();
        println!("{:?}", bytes);
        assert_eq!(bytes, [128, 64, 49, 9, 0, 0, 0, 0, 0, 0, 4, 0, 0, 0, 0, 0, 0, 0, 1, 2, 3, 4]);

        let response = Response::GetStoryOk(4798, vec![1, 2, 3]);
        let bytes = response.as_bytes();
        println!("{:?}", bytes);
        assert_eq!(bytes, [128, 128, 0, 0, 0, 0, 190, 18, 0, 0, 3, 0, 0, 0, 0, 0, 0, 0, 1, 2, 3]);

        let response = Response::StoryDoesNotExist(2353, 4798, vec![1, 2, 3]);
        let bytes = response.as_bytes();
        println!("{:?}", bytes);
        assert_eq!(bytes, [129, 0, 49, 9, 0, 0, 190, 18, 0, 0, 3, 0, 0, 0, 0, 0, 0, 0, 1, 2, 3]);

        let response = Response::AddedStoryOk(2353, 4798, vec![1, 2, 3]);
        let bytes = response.as_bytes();
        println!("{:?}", bytes);
        assert_eq!(bytes, [130, 0, 49, 9, 0, 0, 190, 18, 0, 0, 3, 0, 0, 0, 0, 0, 0, 0, 1, 2, 3]);

        let response = Response::DeletedStoryOk(2353, 4798, vec![1, 2, 3]);
        let bytes = response.as_bytes();
        println!("{:?}", bytes);
        assert_eq!(bytes, [132, 0, 49, 9, 0, 0, 190, 18, 0, 0, 3, 0, 0, 0, 0, 0, 0, 0, 1, 2, 3]);

        let response = Response::StoryStatusUpdateOk(2353, 4798, vec![1, 2, 3]);
        let bytes = response.as_bytes();
        println!("{:?}", bytes);
        assert_eq!(bytes, [136, 0, 49, 9, 0, 0, 190, 18, 0, 0, 3, 0, 0, 0, 0, 0, 0, 0, 1, 2, 3]);

        let response = Response::RequestNotParsed;
        let bytes = response.as_bytes();
        println!("{:?}", bytes);
        assert_eq!(bytes, [16, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,]);
    }
}
