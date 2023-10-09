//! Module that contains the structs that models the database.

use std::collections::HashMap;
use std::sync::RwLock;
use std::fs::{self, DirBuilder, ReadDir, File, OpenOptions};
use std::path::{Path, PathBuf};
use std::error::Error;
use std::io::{self, Seek, SeekFrom, Read, Write, BufRead, BufReader, BufWriter, ErrorKind};
use std::convert::TryFrom;

/// A top level abstraction for the database. Handles all of the reads and writes to the data base.
pub struct DbState {
    /// Holds the mapping from epic id's to `Epic`s
    epics: HashMap<u32, Epic>,
    epic_dir: String,
    db_dir: String,
    db_file_name: String,
    file_path: PathBuf,
}

impl DbState {
    /// Associated method for creating a new `DbState`.
    pub fn create(db_dir: String, db_file_name: String, epic_dir: String) -> DbState {
        let file_path = PathBuf::from(db_dir.as_str());
        file_path.push(db_file_name.as_str());
        DbState {
            epics: HashMap::new(),
            epic_dir,
            db_dir,
            db_file_name,
            file_path,
        }
    }

    /// Associated method for loading data already saved into a new `DbState`.
    pub fn load(db_dir: String, db_file_name: String, epic_dir: String) -> DbResult<DbState> {
        // Create file path
        let mut root_path = PathBuf::from(db_dir.as_str());
        root_path.push(db_file_name.as_str());

        // Load file
        let mut file = if let Ok(f) = OpenOptions::new().read(true).open(root_path) {
            BufReader::new(f)
        } else {
            return Err(DbError::FileLoadError(format!("unable to load file from {}, {}", db_dir, db_file_name)));
        };

        let mut epics = HashMap::new();
        let mut lines = file.lines();

        while let Some(line) = lines.next() {
            let line = line.map_err(|e| DbError::FileReadError(db_file_name))?;
            let (epic_id, epic_file_path) = DbState::parse_db_line(line)?;
            let epic = DbState::parse_epic(epic_file_path)?;
            epics.insert(epic_id, epic);
        }

        Ok(DbState {
            epics,
            epic_dir,
            db_dir,
            db_file_name,
            file_path: path,
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

    fn parse_epic(path: PathBuf) -> Result<Epic, DbError> {
        // Attempt to open the file
        let mut file = if let Ok(f) = OpenOptions::new().read(true).open(path) {
            BufReader::new(f)
        } else {
            return Err(DbError::FileLoadError(format!("unable to load file {:?}", path.to_str())))
        };

        // let mut cur

    }
}

/// A struct that encapsulates all pertinent information and behaviors for a single epic.
#[derive(Debug)]
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

    fn load(path: PathBuf) -> Result<Epic, DbError> {
        // Attempt to open the file
        let mut file = if let Ok(f) = OpenOptions::new().read(true).open(path) {
            BufReader::new(f)
        } else {
            return Err(DbError::FileLoadError(format!("unable to load file {:?}", path.to_str())))
        };

        // Read the bytes for the epic tag
        let mut epic_tag = [0_u8; 17];
        file.read_exact(&epic_tag)
            .map_err(|_e| DbError::FileReadError(format!("unable to read file {:?}", path.to_str())))?;

        // Extract data from epic tag
        let (id, name_len, description_len, status_byte) = <Epic as BytesEncode>::decode(epic_tag);

        let mut epic_name_bytes = vec![0_u8; name_len as usize];
        let mut epic_description_bytes = vec![0_u8; description_len as usize];
        // TODO: rewrite epic so file path is not saved in file
        // let mut epic_file_path_bytes = vec![0_u8; file_path_len as usize];

        // Read the bytes from the file
        file.read_exact(epic_name_bytes.as_mut_slice())
            .map_err(|_e| DbError::FileReadError(format!("unable to read epic: {id} name from file {:?}", path.as_str())))?;
        file.read_exact(epic_description_bytes.as_mut_slice())
            .map_err(|_e| DbError::FileReadError(format!("unable to read epic: {id} description from file {:?}", path.as_str())))?;
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
                match e {
                    ErrorKind::UnexpectedEof => break,
                    _ => return Err(DbError::FileReadError(format!("unable to read stories from {:?}", path.as_str()))),
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

            let story_description = String::from_utf8(story_des)
                .map_err(|_e| DbError::ParseError(format!("unable to parse story: {id} description as valid string")))?;

            // Insert parsed story into hashmap
            let story_status = Status::try_from(story_status_byte)?;

            let story = Story::new(
                id: story_id,
                name: story_name,
                description: story_description,
                status: story_status,
            );

            stories.insert(story_id, story);

        }

        // Create the Epic and return the result
        let epic_status = Status::try_from(status_byte)?;
        let epic = Epic::new(
            id,
            epic_name,
            epic_description,
            epic_status,
            file_path: path,
            stories
        );

        Ok(epic)
    }
}

unsafe impl Send for Epic {}
unsafe impl Sync for Epic {}

impl BytesEncode for Epic {
    type Tag = EpicTag;
    type DecodeTag = (u32, u32, u32, u8);
    fn encode(&self) -> Self::Tag {
        let mut encoded_bytes = [0_u8; 13];

        for i in 0..4 {
            encoded_bytes[i] = ((self.id >> i * 8) & 0xff) as u8;
        }

        for i in 4..8 {
            encoded_bytes[i] = (((self.name.len() as u32) >> (i % 4) * 8) & 0xff) as u8;
        }

        for i in 8..12 {
            encoded_bytes[i] = (((self.description.len() as u32) >> (i % 4) * 8) & 0xff) as u8;
        }
        //
        // for i in 12..16 {
        //     encoded_bytes[i] = (((self.file_path.len() as u32) >> (i % 4) * 8) & 0xff) as u8;
        // }

        // Read status byte
        encoded_bytes[16] = match self.status {
            Status::Open => 0,
            Status::InProgress => 1,
            Status::Resolved => 2,
            Status::Closed => 3,
        };

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

        // // decode length of path
        // let mut path_len = 0_u32;
        // for i in 12..16 {
        //     path_len ^= (tag[i] as u32) << ((i % 4) * 8);
        // }

        // decode status byte
        let status_byte = tag[13];

        (id, name_len, description_len, status_byte)
    }
}

type EpicTag = [u8; 13];

impl TagEncoding for EpicTag {}

type EpicDecodeTag = (u32, u32, u32, u8);

impl TagDecoding for EpicDecodeTag {}


/// A struct that encapsulates all pertinent information and behaviors for a single story.
#[derive(Debug)]
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
    type Tag = StoryTag;
    
    type DecodedTag = StoryDecodeTag;

    fn encode(&self) -> Self::Tag {
        let mut encoded_bytes = [0_u8; 13];

        for i in 0..4 {
            encoded_bytes[i] = ((self.id >> (i * 8)) & 0xff) as u8;
        }

        for i in 4..8 {
            encoded_bytes[i] = (((self.name.len() as u32) >> (i % 4) * 8) & 0xff) as u8;
        }

        for i in 8..12 {
            encoded_bytes[i] = (((self.description.len() as u32) >> (i % 4) * 8) & 0xff) as u8;
        }

        let status_byte: u8 = match self.status {
            Status::Open => 0,
            Status::InProgress => 1,
            Status::Closed => 3,
            Status::Resolved => 4
        };

        encoded_bytes[12] = status_byte;

        encoded_bytes as StoryTag
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

        let status_byte = tag[13];

        (id, name_len, description_len, status_byte)
    }
}

type StoryTag = [u8; 13];

impl TagEncoding for StoryTag {}

type StoryDecodeTag = (u32, u32, u32, u8);

impl TagDecoding for StoryDecodeTag {}

/// Represents the status of either a `Epic` or a `Story` struct
pub enum Status {
    Open,
    InProgress,
    Resolved,
    Closed,
}

impl TryFrom<u8> for Status {
    type Error = DbError::ParseError;

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

#[derive(Debug)]
pub enum DbError {
    FileLoadError(String),
    FileReadError(String),
    ParseError(String),
}



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

