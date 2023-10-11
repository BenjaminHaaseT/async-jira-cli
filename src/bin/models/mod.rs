//! Module that contains the structs that models the database.

use std::collections::HashMap;
use std::sync::RwLock;
use std::fs::{self, DirBuilder, ReadDir, File, OpenOptions};
use std::path::{Path, PathBuf};
use std::error::Error;
use std::io::{self, Seek, SeekFrom, Read, Write, BufRead, BufReader, BufWriter, ErrorKind};
use std::convert::{TryFrom, Into, AsRef};
use std::cmp::PartialEq;

/// A top level abstraction for the database. Handles all of the reads and writes to the data base.
pub struct DbState {
    /// Holds the mapping from epic id's to `Epic`s
    epics: HashMap<u32, Epic>,
    db_dir: String,
    db_file_name: String,
    epic_dir: String,
    file_path: PathBuf,
}

impl DbState {
    /// Associated method for creating a new `DbState`.
    pub fn create(db_dir: String, db_file_name: String, epic_dir: String) -> DbState {
        let mut file_path = PathBuf::from(db_dir.as_str());
        file_path.push(db_file_name.as_str());
        DbState {
            epics: HashMap::new(),
            db_dir,
            db_file_name,
            epic_dir,
            file_path,
        }
    }

    /// Associated method for loading data already saved into a new `DbState`.
    pub fn load(db_dir: String, db_file_name: String, epic_dir: String) -> Result<DbState, DbError> {
        // Create file path
        let mut root_path = PathBuf::from(db_dir.as_str());
        root_path.push(db_file_name.as_str());

        // Load file
        let mut file = if let Ok(f) = OpenOptions::new().read(true).open(root_path.clone()) {
            BufReader::new(f)
        } else {
            return Err(DbError::FileLoadError(format!("unable to load file from {}, {}", db_dir.clone(), db_file_name.clone())));
        };

        let mut epics = HashMap::new();
        let mut lines = file.lines();

        while let Some(line) = lines.next() {
            let line = line.map_err(|e| DbError::FileReadError(db_file_name.clone()))?;
            let (epic_id, epic_file_path) = DbState::parse_db_line(line)?;
            let epic = Epic::load(epic_file_path)?;
            epics.insert(epic_id, epic);
        }

        Ok(DbState {
            epics,
            epic_dir,
            db_dir,
            db_file_name,
            file_path: root_path,
        })
    }

    fn parse_db_line(line: String) -> Result<(u32, PathBuf), DbError> {
        let (id_str, path_str) = match line.find(',') {
            Some(idx) => (&line[..idx], &line[idx+1..]),
            None => return Err(DbError::FileReadError(format!("unable to parse data base line: {}", line))),
        };

        let id = id_str.parse::<u32>()
            .map_err(|_e| DbError::FileReadError(format!("unable to parse epic id: {}", id_str)))?;
        let path = PathBuf::from(path_str);
        Ok((id, path))
    }
}

/// A struct that encapsulates all pertinent information and behaviors for a single epic.
#[derive(Debug, PartialEq)]
pub struct Epic {
    /// The unique identifier of the epic
    id: u32,
    /// The name of the epic
    name: String,
    /// The description of the epic
    description: String,
    /// Holds the current status of the Epic
    status: Status,
    /// Holds the file path the current epic is located in
    file_path: PathBuf,
    /// The mapping from unique story id's to the stories contained in the particular `Epic`
    stories: HashMap<u32, Story>
}

impl Epic {
    /// Associated method for creating a new `Epic` from `id`, `name`, `description`, `status`, `file_path` and `stories`.
    pub fn new(id: u32, name: String, description: String, status: Status, file_path: PathBuf, stories: HashMap<u32, Story>) -> Epic {
        Epic {
            id,
            name,
            description,
            status,
            file_path,
            stories,
        }
    }
    /// Associated method for loading a `Epic` from `path`. The method is fallible, and so a `Result<Epic, DbError>` is returned,
    /// where the `Err` variant is the unsuccessful `load`.
    pub fn load(path: PathBuf) -> Result<Epic, DbError> {
        // Attempt to open the file
        let mut file = if let Ok(f) = OpenOptions::new().read(true).open(path.clone()) {
            BufReader::new(f)
        } else {
            return Err(DbError::FileLoadError(format!("unable to load file {:?}", path.to_str())))
        };

        // Read the bytes for the epic tag
        let mut epic_tag = [0_u8; 13];
        file.read_exact(&mut epic_tag)
            .map_err(|_e| DbError::FileReadError(format!("unable to read file {:?}", path.to_str())))?;

        // Extract data from epic tag
        let (id, name_len, description_len, status_byte) = <Epic as BytesEncode>::decode(epic_tag);

        let mut epic_name_bytes = vec![0_u8; name_len as usize];
        let mut epic_description_bytes = vec![0_u8; description_len as usize];
        // TODO: rewrite epic so file path is not saved in file
        // let mut epic_file_path_bytes = vec![0_u8; file_path_len as usize];

        // Read the bytes from the file
        file.read_exact(epic_name_bytes.as_mut_slice())
            .map_err(|_e| DbError::FileReadError(format!("unable to read epic: {id} name from file {:?}", path.to_str())))?;
        file.read_exact(epic_description_bytes.as_mut_slice())
            .map_err(|_e| DbError::FileReadError(format!("unable to read epic: {id} description from file {:?}", path.to_str())))?;
        // file.read_exact(epic_file_path_bytes.as_mut_slice())
        //     .map_err(|_e| DbError::FileReadError(format!("unable to read epic: {id} file path from file {:?}", path.as_str())))?;

        // Create epic name, description and path as strings
        let epic_name = String::from_utf8(epic_name_bytes)
            .map_err(|_e| DbError::ParseError(format!("unable to parse epic: {id} name")))?;
        let epic_description = String::from_utf8(epic_description_bytes)
            .map_err(|_e| DbError::ParseError(format!("unable to parse epic: {id} description")))?;
        // let epic_file_path = String::from_utf8(epic_file_path_bytes)
        //     .map_err(|_e| DbError::ParseError(format!("unable to parse epic: {id} file path")))?;

        // Create stories hashmap and tag for the current story if any
        let mut stories = HashMap::new();
        let mut cur_story_tag = [0_u8; 13];

        loop {
            // Match for any errors when reading the tag from the file
            // eof error needs to trigger break from the loop, all other others need to be propagated
            if let Err(e) = file.read_exact(&mut cur_story_tag) {
                match e.kind() {
                    ErrorKind::UnexpectedEof => break,
                    _ => return Err(DbError::FileReadError(format!("unable to read stories from {:?}", path.to_str()))),
                }
            }

            // Decode story tag and read bytes from file, propagate errors when they occur
            let (story_id, story_name_len, story_description_len, story_status_byte) = <Story as BytesEncode>::decode(cur_story_tag);
            let mut story_name_bytes = vec![0_u8; story_name_len as usize];
            let mut story_description_bytes = vec![0_u8; story_description_len as usize];

            file.read_exact(story_name_bytes.as_mut_slice())
                .map_err(|_e| DbError::FileReadError(format!("unable to read story: {id} name from file")))?;

            file.read_exact(story_description_bytes.as_mut_slice())
                .map_err(|_e| DbError::FileReadError(format!("unable to read story: {id} description from file")))?;

            let story_name = String::from_utf8(story_name_bytes)
                .map_err(|_e| DbError::ParseError(format!("unable to parse story: {id} name as valid string")))?;

            let story_description = String::from_utf8(story_description_bytes)
                .map_err(|_e| DbError::ParseError(format!("unable to parse story: {id} description as valid string")))?;

            // Insert parsed story into hashmap
            let story_status = Status::try_from(story_status_byte)?;

            let story = Story::new(story_id, story_name, story_description, story_status);

            stories.insert(story_id, story);

        }

        // Create the Epic and return the result
        let epic_status = Status::try_from(status_byte)?;
        let epic = Epic::new(
            id,
            epic_name,
            epic_description,
            epic_status,
            path,
            stories
        );

        Ok(epic)
    }
    /// Writes the `Epic` to the file it is associated with.
    pub fn write(&mut self) -> Result<(), DbError> {
        // Attempt to open file
        let mut writer = if let Ok(f) = OpenOptions::new().write(true).create(true).open::<&Path>(self.file_path.as_ref()) {
            BufWriter::new(f)
        } else {
            return Err(DbError::FileLoadError(format!("unable to open file: {:?}", self.file_path.to_str())));
        };

        // Encode the tag of `self` and write epic data to file
        let epic_tag = self.encode();
        writer.write_all(&epic_tag)
            .map_err(|_e| DbError::FileWriteError(format!("unable to write epic: {} tag to file", self.id)))?;
        writer.write_all(self.name.as_bytes())
            .map_err(|_e| DbError::FileWriteError(format!("unable to write epic: {} name to file", self.id)))?;
        writer.write_all(self.description.as_bytes())
            .map_err(|_e| DbError::FileWriteError(format!("unable to write epic: {} description to file", self.id)))?;

        // Now write stories to file
        for (story_id, story) in &self.stories {
            let story_tag = story.encode();
            writer.write_all(&story_tag)
                .map_err(|_e| DbError::FileWriteError(format!("unable to write story: {} tag to file", *story_id)))?;
            writer.write_all(story.name.as_bytes())
                .map_err(|_e| DbError::FileWriteError(format!("unable to write story: {} name to file", story.name.as_str())))?;
            writer.write_all(story.description.as_bytes())
                .map_err(|_e| DbError::FileWriteError(format!("unable to write story: {} description to file", story.description.as_str())))?;
        }

        Ok(())
    }
    /// Adds a new story to the `Epic`. Returns a `Result<(), DbError>`, the `Ok` variant if successful,
    /// and `Err` variant if unsuccessful.
    pub fn add_story(&mut self, story: Story) -> Result<(), DbError> {
        if self.stories.contains_key(&story.id) {
            return Err(DbError::IdConflict(format!("a story with id: {} already exists for this epic", story.id)));
        } else {
            self.stories.insert(story.id, story);
            Ok(())
        }
    }
    /// Updates the status of `Epic`.
    pub fn update_status(&mut self, status: Status) {
        self.status = status;
    }

    /// Deletes a story from the `Epic`. Returns a `Result<(), DbError>`, the `Ok` variant if successful,
    /// and `Err` variant if unsuccessful.
    pub fn delete_story(&mut self, story_id: u32) -> Result<(), DbError> {
        if !self.stories.contains_key(&story_id) {
            return Err(DbError::IdConflict(format!("no story with id: {} present", story_id)));
        } else {
            self.stories.remove(&story_id);
            Ok(())
        }
    }
}

unsafe impl Send for Epic {}
unsafe impl Sync for Epic {}

impl BytesEncode for Epic {
    type Tag = EncodeTag;
    type DecodedTag = DecodeTag;
    fn encode(&self) -> Self::Tag {
        let mut encoded_bytes = [0_u8; 13];

        for i in 0..4 {
            encoded_bytes[i] = ((self.id >> i * 8) & 0xff) as u8;
        }
        let name_bytes_len = self.name.as_bytes().len();
        for i in 4..8 {
            encoded_bytes[i] = (((name_bytes_len as u32) >> (i % 4) * 8) & 0xff) as u8;
        }
        let description_bytes_len = self.description.as_bytes().len();
        for i in 8..12 {
            encoded_bytes[i] = (((description_bytes_len as u32) >> (i % 4) * 8) & 0xff) as u8;
        }
        // Read status byte
        encoded_bytes[12] = self.status.into();
        encoded_bytes
    }

    fn decode(tag: Self::Tag) -> Self::DecodedTag {
        // decode id
        let mut id = 0_u32;
        for i in 0..4 {
            id ^= (tag[i] as u32) << (i * 8);
        }

        // decode length of name
        let mut name_len = 0_u32;
        for i in 4..8 {
            name_len ^= (tag[i] as u32) << ((i % 4) * 8);
        }

        // decode length of description
        let mut description_len = 0_u32;
        for i in 8..12 {
            description_len ^= (tag[i] as u32) << ((i % 4) * 8);
        }

        // decode status byte
        let status_byte = tag[12];

        (id, name_len, description_len, status_byte)
    }
}



/// A struct that encapsulates all pertinent information and behaviors for a single story.
#[derive(Debug, PartialEq, Clone)]
pub struct Story {
    id: u32,
    name: String,
    description: String,
    status: Status,
}

impl Story {
    pub fn new(id: u32, name: String, description: String, status: Status) -> Story {
        Story {
            id,
            name,
            description,
            status,
        }
    }
}

unsafe impl Send for Story {}
unsafe impl Sync for Story {}

impl BytesEncode for Story {
    type Tag = EncodeTag;
    
    type DecodedTag = DecodeTag;

    fn encode(&self) -> Self::Tag {
        let mut encoded_bytes = [0_u8; 13];
        for i in 0..4 {
            encoded_bytes[i] = ((self.id >> (i * 8)) & 0xff) as u8;
        }
        let name_bytes_len = self.name.as_bytes().len();
        for i in 4..8 {
            encoded_bytes[i] = (((name_bytes_len as u32) >> (i % 4) * 8) & 0xff) as u8;
        }
        let description_bytes_len = self.description.as_bytes().len();
        for i in 8..12 {
            encoded_bytes[i] = (((description_bytes_len as u32) >> (i % 4) * 8) & 0xff) as u8;
        }
        let status_byte: u8 = self.status.into();
        encoded_bytes[12] = status_byte;
        encoded_bytes
    }

    fn decode(tag: Self::Tag) -> Self::DecodedTag {
        let mut id = 0;
        for i in 0..4 {
            id ^= (tag[i] as u32) << (i * 8);
        }
        let mut name_len = 0;
        for i in 4..8 {
            name_len ^= (tag[i] as u32) << ((i % 4) * 8);
        }
        let mut description_len = 0;
        for i in 8..12 {
            description_len ^= (tag[i] as u32) << ((i % 4) * 8);
        }
        let status_byte = tag[12];
        (id, name_len, description_len, status_byte)
    }
}

/// Represents the status of either a `Epic` or a `Story` struct
#[derive(Debug, Copy, Clone)]
pub enum Status {
    Open,
    InProgress,
    Resolved,
    Closed,
}

impl PartialEq for Status {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Status::Open, Status::Open)
            | (Status::InProgress, Status::InProgress)
            | (Status::Resolved, Status::Resolved)
            | (Status::Closed, Status::Closed) => true,
            _ => false
        }
    }
}

impl TryFrom<u8> for Status {
    type Error = DbError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Status::Open),
            1 => Ok(Status::InProgress),
            2 => Ok(Status::Resolved),
            3 => Ok(Status::Closed),
            _ => Err(DbError::ParseError(format!("unable to parse `Status` from byte {value}")))
        }
    }
}

impl Into<u8> for Status {
    fn into(self) -> u8 {
        match self {
            Status::Open => 0,
            Status::InProgress => 1,
            Status::Resolved => 2,
            Status::Closed => 3,
        }
    }
}

#[derive(Debug, Clone)]
pub enum DbError {
    FileLoadError(String),
    FileReadError(String),
    ParseError(String),
    FileWriteError(String),
    IdConflict(String),
}


type DecodeTag = (u32, u32, u32, u8);

impl TagDecoding for DecodeTag {}

type EncodeTag = [u8; 13];

impl TagEncoding for EncodeTag {}


/// Marker trait for types that represent tag encodings
pub trait TagEncoding {}

/// Marker trait for types that represent tag decodings
pub trait TagDecoding {}

/// Provides an interface to types that can be serialized and deserialized as a stream of bytes
pub trait BytesEncode {
    /// The type of tag encoding for the implementing type
    type Tag: TagEncoding;
    /// The type that `Self::Tag` gets decoded into
    type DecodedTag: TagDecoding;
    /// Required: encodes the type into a `Self::Tag`
    fn encode(&self) -> Self::Tag;
    /// Required: decodes `Self::Tag` into a `Self::DecodedTag`
    fn decode(tag: Self::Tag) -> Self::DecodedTag;
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn create_story_should_work() {
        let story = Story::new(1, String::from("Test Story 1"), String::from("A simple test story"), Status::Open);
        println!("{:?}", story);
        assert!(true)
    }
    #[test]
    fn story_encode_decode_should_work() {
        let story = Story::new(1, String::from("Test Story 1"), String::from("A simple test story"), Status::Open);
        // Attempt to encode
        let story_tag = story.encode();

        println!("{:?}", story_tag);

        assert_eq!(story_tag, [1, 0, 0, 0, 12, 0, 0, 0, 19, 0, 0, 0, 0]);

        // Attempt to decode
        let (story_id, story_name_len, story_description_len, story_status_byte) = <Story as BytesEncode>::decode(story_tag);

        assert_eq!(1, story_id);
        assert_eq!(12, story_name_len);
        assert_eq!(19, story_description_len);
        assert_eq!(0, story_status_byte);
    }

    #[test]
    fn create_epic_should_work() {
        let epic = Epic::new(
            1,
            String::from("Test Epic 1"),
            String::from("A simple test epic"),
            Status::Open,
            PathBuf::new(),
            HashMap::new());

        println!("{:?}", epic);

        assert!(true);
    }

    #[test]
    fn epic_encode_decode_should_work() {
        let epic = Epic::new(
            1,
            String::from("Test Epic 1"),
            String::from("A simple test epic"),
            Status::Open,
            PathBuf::new(),
            HashMap::new());

        // Attempt to encode the epic
        let epic_tag = epic.encode();

        println!("{:?}", epic_tag);

        assert_eq!(epic_tag, [1, 0, 0, 0, 11, 0, 0, 0, 18, 0, 0, 0, 0]);

        // Attempt to decode the encoded tag
        let (epic_id, epic_name_len, epic_description_len, epic_status_byte) = Epic::decode(epic_tag);

        assert_eq!(epic_id, 1);
        assert_eq!(epic_name_len, 11);
        assert_eq!(epic_description_len, 18);
        assert_eq!(epic_status_byte, 0);
    }

    #[test]
    fn test_epic_create_write_and_load_from_file() {
        let mut test_file_path = PathBuf::from("/Users/benjaminhaase/development/Personal/async_jira_cli/src/bin/test_database/test_epics");
        test_file_path.push("epic1.txt");

        let mut epic = Epic::new(
            1,
            String::from("Test Epic 1"),
            String::from("A simple test epic"),
            Status::Open,
            test_file_path.clone(),
            HashMap::new());

        println!("{:?}", epic);

        let write_result = epic.write();

        println!("{:?}", write_result);

        assert!(write_result.is_ok());

        // load the epic from the file
        let loaded_epic_result = Epic::load(test_file_path);
        println!("{:?}", loaded_epic_result);
        assert!(loaded_epic_result.is_ok());

        let loaded_epic = loaded_epic_result.unwrap();
        // The two epics must be equal
        assert_eq!(epic, loaded_epic);

    }

    #[test]
    fn test_epic_create_write_load_and_add_stories() {
        let mut test_file_path = PathBuf::from("/Users/benjaminhaase/development/Personal/async_jira_cli/src/bin/test_database/test_epics");
        test_file_path.push("epic2.txt");

        let mut epic = Epic::new(
            2,
            String::from("Test Epic 2"),
            String::from("A simple test epic"),
            Status::Open,
            test_file_path.clone(),
            HashMap::new());

        println!("{:?}", epic);

        let test_story1 = Story::new(3, String::from("A Test Story"), String::from("A simple test story"), Status::Open);
        let test_story2 = Story::new(4, String::from("A test Story"), String::from("A simple test story"), Status::Open);
        let test_story3 = Story::new(5, String::from("A different test Story"), String::from("Another simple test story"), Status::InProgress);

        assert!(epic.add_story(test_story1.clone()).is_ok());
        assert!(epic.add_story(test_story2.clone()).is_ok());
        assert!(epic.add_story(test_story3.clone()).is_ok());

        println!("{:?}", epic);

        assert!(epic.write().is_ok());

        // Test loading the epic from the file again and ensure they are equal
        let loaded_epic_result = Epic::load(test_file_path.clone());

        println!("{:?}", loaded_epic_result);

        assert!(loaded_epic_result.is_ok());

        let mut loaded_epic = loaded_epic_result.unwrap();

        assert_eq!(loaded_epic, epic);

        assert!(epic.add_story(test_story1.clone()).is_err());
        assert!(epic.add_story(test_story2.clone()).is_err());
        assert!(epic.add_story(test_story3.clone()).is_err());

        assert!(loaded_epic.add_story(test_story1.clone()).is_err());
        assert!(loaded_epic.add_story(test_story2.clone()).is_err());
        assert!(loaded_epic.add_story(test_story3.clone()).is_err());
    }

    #[test]
    fn test_epic_delete_story() {
        let mut test_file_path = PathBuf::from("/Users/benjaminhaase/development/Personal/async_jira_cli/src/bin/test_database/test_epics");
        test_file_path.push("epic3.txt");

        let mut epic = Epic::new(
            3,
            String::from("Test Epic 3"),
            String::from("A simple test epic"),
            Status::Open,
            test_file_path.clone(),
            HashMap::new());

        let test_story1 = Story::new(6, String::from("A Test Story"), String::from("A simple test story"), Status::Open);
        let test_story2 = Story::new(7, String::from("A test Story"), String::from("A simple test story"), Status::Open);
        let test_story3 = Story::new(8, String::from("A different test Story"), String::from("Another simple test story"), Status::InProgress);

        assert!(epic.add_story(test_story1.clone()).is_ok());
        assert!(epic.add_story(test_story2.clone()).is_ok());
        assert!(epic.add_story(test_story3.clone()).is_ok());

        println!("{:?}", epic);

        assert!(epic.write().is_ok());

        let loaded_epic_result = Epic::load(test_file_path.clone());

        assert!(loaded_epic_result.is_ok());

        let mut loaded_epic = loaded_epic_result.unwrap();

        println!("{:?}", loaded_epic);

        assert_eq!(loaded_epic, epic);

        assert!(epic.delete_story(6).is_ok());
        assert!(loaded_epic.delete_story(6).is_ok());

        println!("{:?}", epic);
        println!("{:?}", loaded_epic);

        assert_eq!(loaded_epic, epic);

        assert!(epic.delete_story(6).is_err());
        assert!(loaded_epic.delete_story(6).is_err());
    }

    #[test]
    fn test_epic_update_status() {
        let mut test_file_path = PathBuf::from("/Users/benjaminhaase/development/Personal/async_jira_cli/src/bin/test_database/test_epics");
        test_file_path.push("epic4.txt");

        let mut epic = Epic::new(
            4,
            String::from("Test Epic 4"),
            String::from("A simple test epic"),
            Status::Open,
            test_file_path.clone(),
            HashMap::new());

        println!("{:?}", epic);

        assert!(epic.write().is_ok());

        let loaded_epic_result = Epic::load(test_file_path.clone());

        println!("{:?}", loaded_epic_result);

        assert!(loaded_epic_result.is_ok());

        let mut loaded_epic = loaded_epic_result.unwrap();

        println!("{:?}", loaded_epic);

        assert_eq!(epic, loaded_epic);

        epic.update_status(Status::InProgress);

        assert_eq!(epic.status, Status::InProgress);

        loaded_epic.update_status(Status::InProgress);

        assert_eq!(loaded_epic, epic);
    }
}