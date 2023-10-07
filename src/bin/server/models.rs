//! Module that contains the structs that models the database.

use std::collections::HashMap;
use std::sync::RwLock;
use std::fs::{self, DirBuilder, ReadDir, File, OpenOptions};
use std::path::{Path, PathBuf};
use std::error::Error;


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
        let mut path = PathBuf::from(db_dir.as_str());
        path.push(db_file_name.as_str());

        // Load file
        let mut file = if let Ok(f) = OpenOptions::new().read(true).write(true).open(path) {
            BufReader::new(f)
        } else {
            return Err(DbError::FileLoadError(format!("{}, {}", db_dir, db_file_name)));
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
        todo!()
    }

    fn parse_epic(path: PathBuf) -> Epic {
        todo!()
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
    file_path: String,
    /// The mapping from unique story id's to the stories contained in the particular `Epic`
    stories: HashMap<u32, Story>
}

impl Epic {
    pub fn new(id: u32, name: String, description: String, file_path: String) -> Epic {
        Epic {
            id,
            name,
            description,
            status: Status::Open,
            file_path,
            stories: HashMap::new(),
        }
    }
}

unsafe impl Send for Epic {}
unsafe impl Sync for Epic {}

impl BytesEncode for Epic {
    type Tag = EpicTag;
    fn encode(&self) -> Self::Tag {
        let mut encoded_bytes = [0_u8; 17];

        for i in 0..4 {
            encoded_bytes[i] = ((self.id >> i * 8) & 0xff) as u8;
        }

        for i in 4..8 {
            encoded_bytes[i] = (((self.name.len() as u32) >> (i % 4) * 8) & 0xff) as u8;
        }

        for i in 8..12 {
            encoded_bytes[i] = (((self.description.len() as u32) >> (i % 4) * 8) & 0xff) as u8;
        }

        for i in 12..16 {
            encoded_bytes[i] = (((self.file_path.len() as u32) >> (i % 4) * 8) & 0xff) as u8;
        }

        // Read status byte
        encoded_bytes[16] = match self.status {
            Status::Open => 0,
            Status::InProgress => 1,
            Status::Resolved => 2,
            Status::Closed => 3,
        };

        encoded_bytes
    }
}

type EpicTag = [u8; 17];

impl TagEncoding for EpicTag {}

/// A struct that encapsulates all pertinent information and behaviors for a single story.
#[derive(Debug)]
pub struct Story {
    id: u32,
    name: String,
    description: String,
    status: Status,
}

impl Story {
    pub fn new(id: u32, name: String, description: String, file_path: String) -> Story {
        Story {
            id,
            name,
            description,
            status: Status::Open,
        }
    }
}

unsafe impl Send for Story {}
unsafe impl Sync for Story {}

impl BytesEncode for Story {
    type Tag = StoryTag;

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
}

type StoryTag = [u8; 13];

impl TagEncoding for StoryTag {}

/// Represents the status of either a `Epic` or a `Story` struct
pub enum Status {
    Open,
    InProgress,
    Resolved,
    Closed,
}

#[derive(Debug)]
pub enum DbError {
    FileLoadError(String),
    FileReadError(String),
}



/// Marker trait for types that represent tag encodings
pub trait TagEncoding {}

/// Provides an interface to types that can be serialized and deserialized as a stream of bytes
pub trait BytesEncode {
    /// The type of tag encoding for the implementing type
    type Tag: TagEncoding;

    /// Required: encodes the type into a `Self::Tag`
    fn encode(&self) -> Self::Tag;
}

