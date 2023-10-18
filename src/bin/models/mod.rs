//! Module that contains the structs that models the database.

use std::collections::HashMap;
use std::fs::{self, DirBuilder, ReadDir, File, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::Arc;
// use std::ffi::{OsStr, OsString};

#[cfg(target_os = "unix")]
use std::os::ffi::{OsStrExt, OsStringExt};

#[cfg(target_os = "windows")]
use std::os::ffi::{OsStrExt, OsStringExt};

use std::error::Error;
use std::io::{self, Seek, SeekFrom, Read, Write, BufRead, BufReader, BufWriter, ErrorKind};
use std::convert::{TryFrom, Into, AsRef};
use std::cmp::PartialEq;
use async_std::{
    sync::RwLock,
    prelude::*,
    task,
};

pub mod prelude {
    pub use super::*;
}

/// A top level abstraction for the database. Handles all of the reads and writes to the data base.
#[derive(Debug, PartialEq)]
pub struct DbState {
    /// Holds the mapping from epic id's to `Epic`s
    epics: HashMap<u32, Epic>,
    /// A string representing the path to the database directory
    db_dir: String,
    /// A string representing the database file name e.g. db.txt
    db_file_name: String,
    /// A string representing the database epics directory
    epic_dir: String,
    /// A `PathBuf` of the absolute path to the database file
    file_path: PathBuf,
    /// For giving items a unique id
    last_unique_id: u32,
}

impl DbState {
    /// Associated method for creating a new `DbState`.
    pub fn new(db_dir: String, db_file_name: String, epic_dir: String) -> DbState {
        let mut file_path = PathBuf::from(db_dir.as_str());
        file_path.push(db_file_name.as_str());
        DbState {
            epics: HashMap::new(),
            db_dir,
            db_file_name,
            epic_dir,
            file_path,
            last_unique_id: 0,
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
        let mut max_id = 0;

        while let Some(line) = lines.next() {
            let line = line.map_err(|e| DbError::FileReadError(db_file_name.clone()))?;
            let (epic_id, epic_file_path) = DbState::parse_db_line(line)?;
            let epic = Epic::load(epic_file_path, &mut max_id)?;
            epics.insert(epic_id, epic);

        }

        Ok(DbState {
            epics,
            epic_dir,
            db_dir,
            db_file_name,
            file_path: root_path,
            last_unique_id: max_id,
        })
    }

    /// Method for writing the contents of the `DbState` to its associated file. Will create a new
    /// file if an associated db file has not been created, otherwise it will overwrite the
    /// contents of the associated file.  Returns a `Result<(), DbError>`
    /// the `OK` variant if the write was successful, otherwise it returns the `Err` variant.
    pub fn write(&self) -> Result<(), DbError> {
        let mut writer = if let Ok(f) = OpenOptions::new().write(true).truncate(true).create(true).open(&self.file_path) {
            BufWriter::new(f)
        } else {
            return Err(DbError::FileLoadError(format!("unable to open/create associated file: {:?}", self.file_path.to_str())))
        };

        for (id, epic) in &self.epics {
            // TODO: make general for all operating systems
            let epic_file_path_string = format!("{}/{}/epic{id}.txt", &self.db_dir, &self.epic_dir);
            let line = format!("{},{}\n", id, epic_file_path_string).into_bytes();
            writer.write(line.as_slice())
                .map_err(
                    |_e| DbError::FileWriteError(format!("unable to write epic: {id} info to file: {}", self.db_file_name)))?;
            epic.write()?;
        }

        Ok(())
    }

    /// Removes an epic with `id` from the `DbState`. Returns a `Result<Epic, DbError>`,
    /// the `Ok` variant if the delete was successful, otherwise the `Err` variant.
    pub fn delete_epic(&mut self, id: u32) -> Result<Epic, DbError> {
        self.epics.remove(&id).ok_or(DbError::DoesNotExist(format!("no epic with id: {id}")))
    }

    /// Associated helper function. Handles reading a line of text from the db file.
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

    /// Method to create a new `Epic` and add it to the `DbState`. Returns a `Result<(), DbError>,
    /// The `Ok` variant if the `Epic` was added successfully, otherwise it returns the `Err` variant.
    pub fn add_epic(&mut self, name: String, description: String) -> u32 {
        self.last_unique_id += 1;
        let id = self.last_unique_id;
        let epic_fname = format!("epic{id}.txt");
        let mut epic_pathname = PathBuf::from(&self.db_dir);
        epic_pathname.push(self.epic_dir.clone());
        epic_pathname.push(epic_fname);
        self.epics.insert(id, Epic::new(id, name, description, Status::Open, epic_pathname, HashMap::new()));
        self.last_unique_id
    }

    /// Method to get a mutable reference to an `Epic` contained in the `DbState`, for writing. Returns an `Option<&mut Epic>`.
    /// The `Some` variant if the `Epic` is contained in the `DbState`, otherwise it returns the `None` variant.
    pub fn get_epic_mut(&mut self, id: u32) -> Option<&mut Epic> {
        self.epics.get_mut(&id)
    }

    /// Returns a `bool`, true if the `DbState` contains an `Epic` with `id`, false otherwise
    pub fn contains_epic(&self, id: u32) -> bool {
        self.epics.contains_key(&id)
    }

    /// Method to get a shared reference to an `Epic` contained in the `DbState`. Returns an `Option<&Epic>`.
    /// The `Some` variant if the `Epic` is contained in the `DbState`, otherwise it returns the `None` variant.
    pub fn get_epic(&self, id: u32) -> Option<&Epic> {
        self.epics.get(&id)
    }

    /// Method for adding a new story to `Epic` with `epic_id`. Returns a `Result<(), DbError>`,
    /// The `Ok` variant if `Story` was successfully added otherwise the `Err` variant.
    pub fn add_story(&mut self, epic_id: u32, story_name: String, story_description: String) -> Result<(), DbError> {
        if !self.epics.contains_key(&epic_id) {
            return Err(DbError::DoesNotExist("no epic with id: {epic_id}".to_string()));
        }
        self.last_unique_id += 1;
        let new_story = Story::new(self.last_unique_id, story_name, story_description, Status::Open);
        self.epics.get_mut(&epic_id).unwrap().add_story(new_story)
    }

    /// Method for deleting a `Story` with `story_id` from `Epic` with `epic_id`. Returns a `Result<u32, DbError>`
    /// the `Ok` variant if the `Story` was successfully deleted, otherwise the `Err` variant.
    pub fn delete_story(&mut self, epic_id: u32, story_id: u32) -> Result<u32, DbError> {
        if !self.epics.contains_key(&epic_id) {
            return Err(DbError::DoesNotExist(format!("unable to delete story, no epic with id: {}", epic_id)));
        }
        match self.epics.get_mut(&epic_id).unwrap().delete_story(story_id) {
            Ok(_) => Ok(story_id),
            Err(e) => Err(e)
        }
    }
}

/// A top level asynchronous abstraction for the database. Handles all of the reads and writes to the data base.
/// Essentially an asynchronous version of `DbState`
#[derive(Debug)]
pub struct AsyncDbState {
    /// Holds the mapping from epic id's to `Epic`s
    epics: HashMap<u32, Arc<RwLock<Epic>>>,
    /// A string representing the path to the database directory
    db_dir: String,
    /// A string representing the database file name e.g. db.txt
    db_file_name: String,
    /// A string representing the database epics directory
    epic_dir: String,
    /// A `PathBuf` of the absolute path to the database file
    file_path: PathBuf,
    /// For giving items a unique id
    last_unique_id: u32,
}

impl AsyncDbState {
    /// Associated method for creating a new `AsyncDbState`.
    pub fn new(db_dir: String, db_file_name: String, epic_dir: String) -> AsyncDbState {
        let mut file_path = PathBuf::from(db_dir.as_str());
        file_path.push(db_file_name.as_str());
        AsyncDbState {
            epics: HashMap::new(),
            db_dir,
            db_file_name,
            epic_dir,
            file_path,
            last_unique_id: 0,
        }
    }
    /// Associated helper function. Handles reading a line of text from the db file.
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
    /// Associated method for loading a `AsyncDbState` from `db_dir`, `db_file_name` and `epic_dir`.
    pub fn load(db_dir: String, db_file_name: String, epic_dir: String) -> Result<AsyncDbState, DbError> {
        // Create file path
        let mut root_path = PathBuf::from(db_dir.as_str());
        root_path.push(db_file_name.as_str());

        // Load file
        let mut file = if let Ok(f) = std::fs::OpenOptions::new().read(true).open(root_path.clone()) {
            BufReader::new(f)
        } else {
            return Err(DbError::FileLoadError(format!("unable to load file from {}, {}", db_dir.clone(), db_file_name.clone())));
        };

        let mut epics = HashMap::new();
        let mut lines = file.lines();
        let mut max_id = 0;

        while let Some(line) = lines.next() {
            let line = line.map_err(|e| DbError::FileReadError(db_file_name.clone()))?;
            let (epic_id, epic_file_path) = AsyncDbState::parse_db_line(line)?;
            let epic = Epic::load(epic_file_path, &mut max_id)?;
            epics.insert(epic_id, Arc::new(RwLock::new(epic)));

        }

        Ok(AsyncDbState {
            epics,
            epic_dir,
            db_dir,
            db_file_name,
            file_path: root_path,
            last_unique_id: max_id,
        })
    }
    /// Method for writing the contents of the `AsyncDbState` to its associated file. Will create a new
    /// file if an associated db file has not been created, otherwise it will overwrite the
    /// contents of the associated file.  Returns a `Result<(), DbError>`
    /// the `OK` variant if the write was successful, otherwise it returns the `Err` variant.
    ///
    /// The method is async, since it needs to lock each of the `Epics` behind the `RwLock`.
    pub async fn write(&self) -> Result<(), DbError> {
        let mut writer = if let Ok(f) = async_std::fs::OpenOptions::new().write(true).truncate(true).open(&self.file_path).await {
            async_std::io::BufWriter::new(f)
        } else {
            return Err(DbError::FileLoadError(format!("unable to open or create associated file: {:?}", self.file_path.to_str())))
        };

        for (id, epic) in &self.epics {
            // TODO: make general for all operating systems
            let epic_file_path_string = format!("{}/{}/epic{id}.txt", &self.db_dir, &self.epic_dir);
            let line = format!("{},{}\n", id, epic_file_path_string).into_bytes();
            writer.write(line.as_slice())
                .await
                .map_err(
                    |_e| DbError::FileWriteError(format!("unable to write epic: {id} info to file: {}", self.db_file_name)))?;
            let mut epic_lock = epic.write().await;
            epic_lock.write_async().await?;
        }

        Ok(())
    }
    /// Method to create a new `Epic` and add it to the `AsyncDbState`. Returns a `Result<(), DbError>,
    /// The `Ok` variant if the `Epic` was added successfully, otherwise it returns the `Err` variant.
    pub fn add_epic(&mut self, name: String, description: String) -> u32 {
        self.last_unique_id += 1;
        let id = self.last_unique_id;
        let epic_fname = format!("epic{id}.txt");
        let mut epic_pathname = PathBuf::from(&self.db_dir);
        epic_pathname.push(self.epic_dir.clone());
        epic_pathname.push(epic_fname);
        self.epics.insert(id, Arc::new(RwLock::new(Epic::new(id, name, description, Status::Open, epic_pathname, HashMap::new()))));
        self.last_unique_id
    }
    /// Method to check if an `Epic` with `epic_id` contains in the `AsyncDbState`.
    pub fn contains_epic(&self, epic_id: u32) -> bool {
        self.epics.contains_key(&epic_id)
    }
    /// Method to get an `Arc<RwLock<Epic>>>` from the `AsyncDbState`. Returns an `Option`,
    /// The `Some` variant if the `Epic` is contained in the database otherwise the `None` variant.
    pub fn get_epic(&self, epic_id: u32) -> Option<Arc<RwLock<Epic>>> {
        match self.epics.get(&epic_id) {
            Some(epic) => Some(Arc::clone(epic)),
            None => None
        }
    }
    /// Method to delete an `Epic` from the database, note this method only logically deletes the `Epic`,
    /// Since the `Epic`s are behind an `Arc` there may be other threads still reading or writing to the epic.
    pub fn delete_epic(&mut self, epic_id: u32) -> Result<Arc<RwLock<Epic>>, DbError> {
        self.epics.remove(&epic_id)
            .ok_or(DbError::DoesNotExist(format!("unable to delete epic with id: {}, epic not contained in database", epic_id)))
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
    pub fn load(path: PathBuf, max_id: &mut u32) -> Result<Epic, DbError> {
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
        // let mut max_id = 0;

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
            *max_id = u32::max(*max_id, story_id);
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
        *max_id = u32::max(id, *max_id);
        Ok(epic)
    }
    /// Writes the `Epic` to the file it is associated with, creates a new file if an associated
    /// file does not exist.
    pub fn write(&self) -> Result<(), DbError> {
        // Attempt to open file
        let mut writer = if let Ok(f) = OpenOptions::new().write(true).create(true).truncate(true).open::<&Path>(self.file_path.as_ref()) {
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
    /// An asynchronous version of `self.write()`.
    pub async fn write_async(&self) -> Result<(), DbError> {
        let mut writer = if let Ok(f) = async_std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open::<&Path>(self.file_path.as_ref()).await {
            async_std::io::BufWriter::new(f)
        } else {
            return Err(DbError::FileLoadError(format!("unable to open file: {:?}", self.file_path.to_str())))
        };

        // Encode the tag of `self` and write epic data to file
        let epic_tag = self.encode();
        writer.write_all(&epic_tag)
            .await
            .map_err(|_e| DbError::FileWriteError(format!("unable to write epic: {} tag to file", self.id)))?;
        writer.write_all(self.name.as_bytes())
            .await
            .map_err(|_e| DbError::FileWriteError(format!("unable to write epic: {} name to file", self.id)))?;
        writer.write_all(self.description.as_bytes())
            .await
            .map_err(|_e| DbError::FileWriteError(format!("unable to write epic: {} description to file", self.id)))?;

        // Now write stories to file
        for (story_id, story) in &self.stories {
            let story_tag = story.encode();
            writer.write_all(&story_tag)
                .await
                .map_err(|_e| DbError::FileWriteError(format!("unable to write story: {} tag to file", *story_id)))?;
            writer.write_all(story.name.as_bytes())
                .await
                .map_err(|_e| DbError::FileWriteError(format!("unable to write story: {} name to file", story.name.as_str())))?;
            writer.write_all(story.description.as_bytes())
                .await
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
            return Err(DbError::DoesNotExist(format!("no story with id: {} present", story_id)));
        } else {
            self.stories.remove(&story_id);
            Ok(())
        }
    }

    /// Method for updating a stories status. Returns `Result<(), DbError>`, the `Ok` variant if successful,
    /// and `Err` variant if unsuccessful.
    pub fn update_story_status(&mut self, story_id: u32, status: Status) -> Result<(), DbError> {
        if !self.stories.contains_key(&story_id) {
            Err(DbError::DoesNotExist(format!("unable to update story status for id: {}", story_id)))
        } else {
            self.stories.get_mut(&story_id).unwrap().status = status;
            Ok(())
        }
    }

    /// Method for getting a byte representation of the current state of `self`. Useful for writing to a tcp stream.
    pub fn as_bytes(&self) -> Vec<u8> {
        let mut bytes = vec![];
        bytes.extend_from_slice(&self.encode());
        bytes.extend_from_slice(self.name.as_bytes());
        bytes.extend_from_slice(self.description.as_bytes());
        for (_, story) in &self.stories {
            bytes.extend_from_slice(&story.encode());
            bytes.extend_from_slice(story.name.as_bytes());
            bytes.extend_from_slice(story.description.as_bytes());
        }
        bytes
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
    DoesNotExist(String),
    ConnectionError(String),
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
        let mut dummy_id = 0;
        // load the epic from the file
        let loaded_epic_result = Epic::load(test_file_path, &mut dummy_id);
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
        let mut dummy_id = 0;
        // Test loading the epic from the file again and ensure they are equal
        let loaded_epic_result = Epic::load(test_file_path.clone(), &mut dummy_id);

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
        let mut dummy_id = 0;
        let loaded_epic_result = Epic::load(test_file_path.clone(), &mut dummy_id);

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
        let mut dummy_id = 0;
        let loaded_epic_result = Epic::load(test_file_path.clone(), &mut dummy_id);

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

    #[test]
    fn test_create_db_state_should_work() {
        let db_state = DbState::new(
            "/Users/benjaminhaase/development/Personal/async_jira_cli/src/bin/test_database".to_string(),
            "test_db.txt".to_string(),
            "test_epics".to_string()
        );

        println!("{:?}", db_state);
        assert!(true);
    }

    #[test]
    fn test_create_db_state_add_epics_write_and_read() {
        let mut db_state = DbState::new(
            "/Users/benjaminhaase/development/Personal/async_jira_cli/src/bin/test_database".to_string(),
            "test_db.txt".to_string(),
            "test_epics".to_string()
        );

        assert_eq!(db_state.add_epic("Test Epic".to_string(), "A simple test epic".to_string()), 1);
        assert_eq!(db_state.add_epic("Test Epic".to_string(), "A simple test epic".to_string()), 2);
        assert_eq!(db_state.add_epic("Test Epic".to_string(), "A simple test epic".to_string()), 3);

        // assert!(db_state.add_epic("Test Epic".to_string(), "A simple test epic".to_string()).is_err());

        println!("{:?}", db_state);

        assert!(db_state.write().is_ok());

        let mut db_state_prime_result = DbState::load(
            "/Users/benjaminhaase/development/Personal/async_jira_cli/src/bin/test_database".to_string(),
            "test_db.txt".to_string(),
            "test_epics".to_string()
        );

        println!("{:?}", db_state_prime_result);

        assert!(db_state_prime_result.is_ok());

        let mut db_state_prime = db_state_prime_result.unwrap();

        assert_eq!(db_state, db_state_prime);
    }

    #[test]
    fn test_db_state_add_delete_epic() {
        let mut db_state = DbState::load(
            "/Users/benjaminhaase/development/Personal/async_jira_cli/src/bin/test_database".to_string(),
            "test_db.txt".to_string(),
            "test_epics".to_string()
        ).expect("should load");

        println!("{:?}", db_state);

        db_state.add_epic(
            "Test Epic".to_string(),
            "A specially added test epic".to_string()
        );

        println!("{:?}", db_state);

        assert!(db_state.write().is_ok());

        let mut db_state = DbState::load(
            "/Users/benjaminhaase/development/Personal/async_jira_cli/src/bin/test_database".to_string(),
            "test_db.txt".to_string(),
            "test_epics".to_string()
        ).expect("should load");

        println!("{:?}", db_state);

        assert!(db_state.contains_epic(4));

        assert!(db_state.delete_epic(4).is_ok());

        assert!(db_state.delete_epic(17).is_err());

        assert!(db_state.write().is_ok());

        let mut db_state = DbState::load(
            "/Users/benjaminhaase/development/Personal/async_jira_cli/src/bin/test_database".to_string(),
            "test_db.txt".to_string(),
            "test_epics".to_string()
        ).expect("should load");

        println!("{:?}", db_state);

        assert!(!db_state.contains_epic(4));
    }

    #[test]
    fn test_db_state_add_delete_story() {
        let mut db_state = DbState::load(
            "/Users/benjaminhaase/development/Personal/async_jira_cli/src/bin/test_database".to_string(),
            "test_db.txt".to_string(),
            "test_epics".to_string()
        ).expect("should load");

        println!("{:?}", db_state);

        let cur_id = db_state.add_epic("Test Epic".to_string(), "A test Epic for adding/deleting stories".to_string());

        println!("{:?}", cur_id);

        assert!(db_state.add_story(cur_id, "Test Story".to_string(), "A Test story for adding/deleting stories".to_string()).is_ok());
        assert!(db_state.add_story(cur_id, "Test Story".to_string(), "A Test story for adding/deleting stories".to_string()).is_ok());
        assert!(db_state.add_story(cur_id, "Test Story".to_string(), "A Test story for adding/deleting stories".to_string()).is_ok());

        println!("{:?}", db_state);

        assert!(db_state.write().is_ok());

        let mut db_state = DbState::load(
            "/Users/benjaminhaase/development/Personal/async_jira_cli/src/bin/test_database".to_string(),
            "test_db.txt".to_string(),
            "test_epics".to_string()
        ).expect("should load");

        println!("{:?}", db_state);

        assert!(db_state.contains_epic(cur_id));

        let cur_story_id = db_state.last_unique_id;

        assert!(db_state.delete_story(cur_id, cur_story_id).is_ok());

        assert!(db_state.delete_story(cur_id, cur_story_id).is_err());
    }
}