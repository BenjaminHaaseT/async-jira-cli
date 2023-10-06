//! Module that contains the structs that models the database.

use std::collections::HashMap;
use std::sync::RwLock;


/// A top level abstraction for the database. Handles all of the reads and writes to the data base.
pub struct DbState {
    /// Holds the mapping from epic id's to `Epic`s
    epics: HashMap<u32, Epic>,
    db_dir: String,
    db_file_name: String,
}

impl DbState {
    /// Associated method for creating a new `DbState`
    pub fn new(db_dir: String, db_file_name: String) -> DbState {
        DbState {
            epics: HashMap::new(),
            db_dir,
            db_file_name
        }
    }
    /// Associated method for loading data already saved into a new `DbState`
    pub fn load(db_dir: String, db_file_name: String) -> DbState {

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
    /// The filename where the epic is stored, note it is not the full path
    file_path: String,
    /// Holds the current status of the Epic
    status: Status,
    /// The mapping from unique story id's to the stories contained in the particular `Epic`
    stories: HashMap<u32, Story>
}

impl Epic {
    pub fn new(id: u32, name: String, description: String, file_path: String) -> Epic {
        Epic {
            id,
            name,
            description,
            file_path,
            status: Status::Open,
            stories: HashMap::new(),
        }
    }
}

unsafe impl Send for Epic {}
unsafe impl Sync for Epic {}

/// A struct that encapsulates all pertinent information and behaviors for a single story.
#[derive(Debug)]
pub struct Story {
    id: u32,
    name: String,
    description: String,
    status: Status,
    file_path: String,
}

impl Story {
    pub fn new(id: u32, name: String, description: String, file_path: String) -> Story {
        Story {
            id,
            name,
            description,
            status: Status::Open,
            file_path
        }
    }
}

unsafe impl Send for Story {}
unsafe impl Sync for Story {}

type StoryTag = [u8; 17];

impl TagEncoding for StoryTag {}

/// Represents the status of either a `Epic` or a `Story` struct
pub enum Status {
    Open,
    InProgress,
    Resolved,
    Closed,
}

/// Marker trait for types that represent tag encodings
pub trait TagEncoding {}

/// Provides an interface to types that can be serialized and deserialized as a stream of bytes
pub trait BytesEncode {
    /// The type of tag encoding for the implementing type
    type Tag: TagEncoding;

    /// Required: encodes the type into a `Self::Tag`
    fn encode(&self) -> Self::Tag;

    /// Required: decodes a `Self::Tag` into a `Self`.
    fn decode(tag: Self::Tag) -> Self;
}

