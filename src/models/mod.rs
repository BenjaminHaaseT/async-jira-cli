//! Module that contains structs needed to model the database.
//!
//! The structs contained in this module implement methods for  performing common CRUD operations
//! on the database.

use std::collections::HashMap;
// use std::fs::OpenOptions as StdOpenOptions;
use std::path::{Path, PathBuf};

use async_std::prelude::*;
use async_std::fs::OpenOptions;
use async_std::io::{BufRead, BufReader, BufWriter, ErrorKind, Read, Seek, Write};
use std::cmp::PartialEq;
use std::convert::{AsRef, Into, TryFrom};
use std::error::Error;
use std::fmt::Formatter;
// use std::io::{BufRead as StdBufRead, BufReader as StdBufReader, BufWriter as StdBufWriter, ErrorKind as StdErrorKind, Read as StdRead, Seek as StdSeek, Write as StdWrite};

use crate::utils::{AsBytes, BytesEncode, TagDecoding, TagEncoding};

pub mod prelude {
    pub use super::*;
}

/// A top level abstraction for the database, all reads and writes to/from the database
/// are performed via the `DbState` struct in some way.
///
/// A `DbState` acts as the conduit for reading and writing to the database. It can therefore
/// load an already existing database or be used to create a new one.
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
    ///
    ///
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
    pub async fn load(
        db_dir: String,
        db_file_name: String,
        epic_dir: String,
    ) -> Result<DbState, DbError> {
        // Create file path
        println!("Inside load db method..");
        let mut root_path = PathBuf::from(db_dir.clone());
        root_path.push(db_file_name.clone());

        // Load file
        let mut file = if let Ok(f) = OpenOptions::new().read(true).open(root_path.clone()).await {
            BufReader::new(f)
        } else {
            return Err(DbError::FileLoadError(format!(
                "unable to load file from {}, {}",
                db_dir, db_file_name
            )));
        };

        println!("loaded file successfully");

        let mut epics = HashMap::new();
        let mut lines = file.lines();
        let mut max_id = 0;

        let db_file_name_clone = db_file_name.clone();

        println!("Attempting to read db file...");
        while let Some(line) = lines.next().await {
            let line = line.map_err(|_| {
                DbError::FileReadError(format!(
                    "unable to read line from file: {}",
                    db_file_name_clone
                ))
            })?;
            let (epic_id, epic_file_path) = DbState::parse_db_line(line)?;
            let epic = Epic::load(epic_file_path, &mut max_id).await?;
            epics.insert(epic_id, epic);
        }
        println!("finished loading db....");
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
    // pub fn write(&self) -> Result<(), DbError> {
    //     let mut writer = if let Ok(f) = StdOpenOptions::new()
    //         .write(true)
    //         .truncate(true)
    //         .create(true)
    //         .open(&self.file_path)
    //     {
    //         std::io::BufWriter::new(f)
    //     } else {
    //         return Err(DbError::FileLoadError(format!(
    //             "unable to open/create associated file: {:?}",
    //             self.file_path.to_str()
    //         )));
    //     };
    //
    //     for (id, epic) in &self.epics {
    //         let epic_file_path_str =
    //             epic.file_path
    //                 .to_str()
    //                 .ok_or(DbError::InvalidUtf8Path(format!(
    //                     "file path for epic: {} not valid utf8",
    //                     id
    //                 )))?;
    //         let line = format!("{},{}\n", id, epic_file_path_str).into_bytes();
    //         writer.write(line.as_slice()).map_err(|_e| {
    //             DbError::FileWriteError(format!(
    //                 "unable to write epic: {id} info to file: {}",
    //                 self.db_file_name
    //             ))
    //         })?;
    //         epic.write()?;
    //     }
    //
    //     Ok(())
    // }

    /// Method for writing the contents of the `DbState` to its associated file. Will create a new
    /// file if an associated db file has not been created, otherwise it will overwrite the
    /// contents of the associated file.  Returns a `Result<(), DbError>`
    /// the `OK` variant if the write was successful, otherwise it returns the `Err` variant.
    pub async fn write_async(&self) -> Result<(), DbError> {
        let mut writer = if let Ok(f) = async_std::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .create(true)
            .open(&self.file_path)
            .await
        {
            async_std::io::BufWriter::new(f)
        } else {
            return Err(DbError::FileLoadError(format!(
                "unable to open or create associated file: {:?}",
                self.file_path.to_str()
            )));
        };

        for (id, epic) in &self.epics {
            let epic_file_path_str =
                epic.file_path
                    .to_str()
                    .ok_or(DbError::InvalidUtf8Path(format!(
                        "file path for epic: {} not valid utf8",
                        id
                    )))?;
            let line = format!("{},{}\n", id, epic_file_path_str).into_bytes();
            writer.write(line.as_slice()).await.map_err(|_e| {
                DbError::FileWriteError(format!(
                    "unable to write epic: {id} info to file: {}",
                    self.db_file_name
                ))
            })?;
            epic.write_async().await?;
        }

        writer.flush().await.map_err(|_| {
            DbError::FileWriteError(format!(
                "unable to flush buffer when writing to file: {}",
                self.db_file_name
            ))
        })?;

        Ok(())
    }

    /// Removes an epic with `id` from the `DbState`. Returns a `Result<Epic, DbError>`,
    /// the `Ok` variant if the delete was successful, otherwise the `Err` variant.
    pub fn delete_epic(&mut self, id: u32) -> Result<Epic, DbError> {
        self.epics
            .remove(&id)
            .ok_or(DbError::DoesNotExist(format!("no epic with id: {id}")))
    }

    /// Associated helper function. Handles reading a line of text from the db file.
    fn parse_db_line(line: String) -> Result<(u32, PathBuf), DbError> {
        let (id_str, path_str) = match line.find(',') {
            Some(idx) => (&line[..idx], &line[idx + 1..]),
            None => {
                return Err(DbError::FileReadError(format!(
                    "unable to parse data base line: {}",
                    line
                )))
            }
        };
        let id = id_str
            .parse::<u32>()
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
        self.epics.insert(
            id,
            Epic::new(
                id,
                name,
                description,
                Status::Open,
                epic_pathname,
                HashMap::new(),
            ),
        );
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
    pub fn add_story(
        &mut self,
        epic_id: u32,
        story_name: String,
        story_description: String,
    ) -> Result<(), DbError> {
        if !self.epics.contains_key(&epic_id) {
            return Err(DbError::DoesNotExist(
                "no epic with id: {epic_id}".to_string(),
            ));
        }
        self.last_unique_id += 1;
        let new_story = Story::new(
            self.last_unique_id,
            epic_id,
            story_name,
            story_description,
            Status::Open,
        );
        self.epics.get_mut(&epic_id).unwrap().add_story(new_story)
    }

    /// Method for deleting a `Story` with `story_id` from `Epic` with `epic_id`. Returns a `Result<u32, DbError>`
    /// the `Ok` variant if the `Story` was successfully deleted, otherwise the `Err` variant.
    pub fn delete_story(&mut self, epic_id: u32, story_id: u32) -> Result<u32, DbError> {
        if !self.epics.contains_key(&epic_id) {
            return Err(DbError::DoesNotExist(format!(
                "unable to delete story, no epic with id: {}",
                epic_id
            )));
        }
        match self.epics.get_mut(&epic_id).unwrap().delete_story(story_id) {
            Ok(_) => Ok(story_id),
            Err(e) => Err(e),
        }
    }

    /// Method to add a `Story` to an `Epic` contained in `self` with `epic_id`. The method can fail
    ///  if there is no `Epic` contained in `self` with `epic_id`.
    pub async fn add_story_async(
        &mut self,
        epic_id: u32,
        story_name: String,
        story_description: String,
    ) -> Result<u32, DbError> {
        if let Some(epic) = self.epics.get_mut(&epic_id) {
            self.last_unique_id += 1;
            let new_story = Story::new(
                self.last_unique_id,
                epic_id,
                story_name,
                story_description,
                Status::Open,
            );
            epic.add_story(new_story)?;
            epic.write_async().await?;
            Ok(self.last_unique_id)
        } else {
            Err(DbError::DoesNotExist(format!(
                "epic with id: {} does not exist",
                epic_id
            )))
        }
    }

    pub fn last_unique_id(&self) -> u32 {
        self.last_unique_id
    }
}

impl AsBytes for DbState {
    fn as_bytes(&self) -> Vec<u8> {
        let mut bytes = vec![];
        for (_epic_id, epic) in &self.epics {
            bytes.extend_from_slice(&epic.encode());
            bytes.extend_from_slice(epic.name.as_bytes());
            bytes.extend_from_slice(epic.description.as_bytes());
        }
        bytes
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
    /// The current status of the Epic
    status: Status,
    /// The file path the current epic is located in
    file_path: PathBuf,
    /// The mapping from unique story id's to the stories contained in the particular `Epic`
    stories: HashMap<u32, Story>,
}

impl Epic {
    /// Associated method for creating a new `Epic` from `id`, `name`, `description`, `status`, `file_path` and `stories`.
    pub fn new(
        id: u32,
        name: String,
        description: String,
        status: Status,
        file_path: PathBuf,
        stories: HashMap<u32, Story>,
    ) -> Epic {
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
    async fn load(path: PathBuf, max_id: &mut u32) -> Result<Epic, DbError> {
        // Attempt to open the file
        let mut file = if let Ok(f) = OpenOptions::new().read(true).open(path.clone()).await {
            BufReader::new(f)
        } else {
            return Err(DbError::FileLoadError(format!(
                "unable to load file {:?}",
                path.to_str()
            )));
        };

        // Read the bytes for the epic tag
        let mut epic_tag = [0_u8; 13];
        file.read_exact(&mut epic_tag).await.map_err(|_e| {
            DbError::FileReadError(format!("unable to read file {:?}", path.to_str()))
        })?;

        // Extract data from epic tag
        let (id, name_len, description_len, status_byte) = <Epic as BytesEncode>::decode(&epic_tag);

        let mut epic_name_bytes = vec![0_u8; name_len as usize];
        let mut epic_description_bytes = vec![0_u8; description_len as usize];
        // TODO: rewrite epic so file path is not saved in file
        // let mut epic_file_path_bytes = vec![0_u8; file_path_len as usize];

        // Read the bytes from the file
        file.read_exact(epic_name_bytes.as_mut_slice())
            .await
            .map_err(|_e| {
                DbError::FileReadError(format!(
                    "unable to read epic: {id} name from file {:?}",
                    path.to_str()
                ))
            })?;
        file.read_exact(epic_description_bytes.as_mut_slice())
            .await
            .map_err(|_e| {
                DbError::FileReadError(format!(
                    "unable to read epic: {id} description from file {:?}",
                    path.to_str()
                ))
            })?;
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
        let mut cur_story_tag = [0_u8; 17];
        // let mut max_id = 0;

        loop {
            // Match for any errors when reading the tag from the file
            // eof error needs to trigger break from the loop, all other others need to be propagated
            if let Err(e) = file.read_exact(&mut cur_story_tag).await {
                match e.kind() {
                    ErrorKind::UnexpectedEof => break,
                    _ => {
                        return Err(DbError::FileReadError(format!(
                            "unable to read stories from {:?}",
                            path.to_str()
                        )))
                    }
                }
            }

            // Decode story tag and read bytes from file, propagate errors when they occur
            let (story_id, _, story_name_len, story_description_len, story_status_byte) =
                <Story as BytesEncode>::decode(&cur_story_tag);
            let mut story_name_bytes = vec![0_u8; story_name_len as usize];
            let mut story_description_bytes = vec![0_u8; story_description_len as usize];

            file.read_exact(story_name_bytes.as_mut_slice())
                .await
                .map_err(|_e| {
                    DbError::FileReadError(format!("unable to read story: {id} name from file"))
                })?;

            file.read_exact(story_description_bytes.as_mut_slice())
                .await
                .map_err(|_e| {
                    DbError::FileReadError(format!(
                        "unable to read story: {id} description from file"
                    ))
                })?;

            let story_name = String::from_utf8(story_name_bytes).map_err(|_e| {
                DbError::ParseError(format!("unable to parse story: {id} name as valid string"))
            })?;

            let story_description = String::from_utf8(story_description_bytes).map_err(|_e| {
                DbError::ParseError(format!(
                    "unable to parse story: {id} description as valid string"
                ))
            })?;

            // Insert parsed story into hashmap
            let story_status = Status::try_from(story_status_byte)?;

            let story = Story::new(story_id, id, story_name, story_description, story_status);

            stories.insert(story_id, story);
            *max_id = u32::max(*max_id, story_id);
        }

        // Create the Epic and return the result
        let epic_status = Status::try_from(status_byte)?;
        let epic = Epic::new(id, epic_name, epic_description, epic_status, path, stories);
        *max_id = u32::max(id, *max_id);
        Ok(epic)
    }

    /// Writes the `Epic` to the file it is associated with, creates a new file if an associated
    /// file does not exist.
    // pub fn write(&self) -> Result<(), DbError> {
    //     // Attempt to open file
    //     let mut writer = if let Ok(f) = StdOpenOptions::new()
    //         .write(true)
    //         .create(true)
    //         .truncate(true)
    //         .open::<&Path>(self.file_path.as_ref())
    //     {
    //         std::io::BufWriter::new(f)
    //     } else {
    //         return Err(DbError::FileLoadError(format!(
    //             "unable to open file: {:?}",
    //             self.file_path.to_str()
    //         )));
    //     };
    //
    //     // Encode the tag of `self` and write epic data to file
    //     let epic_tag = self.encode();
    //     writer.write_all(&epic_tag).map_err(|_e| {
    //         DbError::FileWriteError(format!("unable to write epic: {} tag to file", self.id))
    //     })?;
    //     writer.write_all(self.name.as_bytes()).map_err(|_e| {
    //         DbError::FileWriteError(format!("unable to write epic: {} name to file", self.id))
    //     })?;
    //     writer
    //         .write_all(self.description.as_bytes())
    //         .map_err(|_e| {
    //             DbError::FileWriteError(format!(
    //                 "unable to write epic: {} description to file",
    //                 self.id
    //             ))
    //         })?;
    //
    //     // Now write stories to file
    //     for (story_id, story) in &self.stories {
    //         let story_tag = story.encode();
    //         writer.write_all(&story_tag).map_err(|_e| {
    //             DbError::FileWriteError(format!("unable to write story: {} tag to file", *story_id))
    //         })?;
    //         writer.write_all(story.name.as_bytes()).map_err(|_e| {
    //             DbError::FileWriteError(format!(
    //                 "unable to write story: {} name to file",
    //                 story.name.as_str()
    //             ))
    //         })?;
    //         writer
    //             .write_all(story.description.as_bytes())
    //             .map_err(|_e| {
    //                 DbError::FileWriteError(format!(
    //                     "unable to write story: {} description to file",
    //                     story.description.as_str()
    //                 ))
    //             })?;
    //     }
    //
    //     Ok(())
    // }

    /// An asynchronous version of `self.write()`.
    pub async fn write_async(&self) -> Result<(), DbError> {
        let mut writer = if let Ok(f) = async_std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open::<&Path>(self.file_path.as_ref())
            .await
        {
            async_std::io::BufWriter::new(f)
        } else {
            return Err(DbError::FileLoadError(format!(
                "unable to open file: {:?}",
                self.file_path.to_str()
            )));
        };

        // Encode the tag of `self` and write epic data to file
        let epic_tag = self.encode();
        writer.write_all(&epic_tag).await.map_err(|_e| {
            DbError::FileWriteError(format!("unable to write epic: {} tag to file", self.id))
        })?;
        writer.write_all(self.name.as_bytes()).await.map_err(|_e| {
            DbError::FileWriteError(format!("unable to write epic: {} name to file", self.id))
        })?;
        writer
            .write_all(self.description.as_bytes())
            .await
            .map_err(|_e| {
                DbError::FileWriteError(format!(
                    "unable to write epic: {} description to file",
                    self.id
                ))
            })?;

        // Now write stories to file
        for (story_id, story) in &self.stories {
            let story_tag = story.encode();
            writer.write_all(&story_tag).await.map_err(|_e| {
                DbError::FileWriteError(format!("unable to write story: {} tag to file", *story_id))
            })?;
            writer
                .write_all(story.name.as_bytes())
                .await
                .map_err(|_e| {
                    DbError::FileWriteError(format!(
                        "unable to write story: {} name to file",
                        story.name.as_str()
                    ))
                })?;
            writer
                .write_all(story.description.as_bytes())
                .await
                .map_err(|_e| {
                    DbError::FileWriteError(format!(
                        "unable to write story: {} description to file",
                        story.description.as_str()
                    ))
                })?;
        }

        writer.flush().await.map_err(|_| {
            DbError::FileWriteError(format!(
                "unable to flush writer to file: {:?}",
                self.file_path.to_str()
            ))
        })?;

        Ok(())
    }

    /// Adds a new story to the `Epic`. Returns a `Result<(), DbError>`, the `Ok` variant if successful,
    /// and `Err` variant if unsuccessful.
    pub fn add_story(&mut self, story: Story) -> Result<(), DbError> {
        if self.stories.contains_key(&story.id) {
            return Err(DbError::IdConflict(format!(
                "a story with id: {} already exists for this epic",
                story.id
            )));
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
    pub fn delete_story(&mut self, story_id: u32) -> Result<u32, DbError> {
        if !self.stories.contains_key(&story_id) {
            return Err(DbError::DoesNotExist(format!(
                "no story with id: {} present",
                story_id
            )));
        } else {
            self.stories.remove(&story_id);
            Ok(story_id)
        }
    }

    /// Method for updating a stories status. Returns `Result<(), DbError>`, the `Ok` variant if successful,
    /// and `Err` variant if unsuccessful.
    pub fn update_story_status(&mut self, story_id: u32, status: Status) -> Result<(), DbError> {
        if !self.stories.contains_key(&story_id) {
            Err(DbError::DoesNotExist(format!(
                "unable to update story status for id: {}",
                story_id
            )))
        } else {
            self.stories.get_mut(&story_id).unwrap().status = status;
            Ok(())
        }
    }

    /// Returns an `Option<&Story>`. If `self` contains a story with id `story_id` then the `Some` variant is returned,
    /// otherwise the `None` variant is returned.
    pub fn get_story(&self, story_id: u32) -> Option<&Story> {
        self.stories.get(&story_id)
    }

    /// Returns the id of `self.
    pub fn id(&self) -> u32 {
        self.id
    }

    /// Returns a shared reference to the name of `self`.
    pub fn name(&self) -> &String {
        &self.name
    }

    /// Returns a shared reference to the description of `self`.
    pub fn description(&self) -> &String {
        &self.description
    }

    /// Returns the status of `self`.
    pub fn status(&self) -> Status {
        self.status
    }
}

impl AsBytes for Epic {
    fn as_bytes(&self) -> Vec<u8> {
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
    type Tag = EpicEncodeTag;
    type DecodedTag = EpicDecodeTag;
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

    fn decode(tag: &Self::Tag) -> Self::DecodedTag {
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
    /// The id of `self`
    id: u32,
    /// The `Epic` id that contains the `Story`
    epic_id: u32,
    /// Name of the `Story`
    name: String,
    /// Description of the `Story`
    description: String,
    /// The current status of the `Story`
    status: Status,
}

impl Story {
    pub fn new(id: u32, epic_id: u32, name: String, description: String, status: Status) -> Story {
        Story {
            id,
            epic_id,
            name,
            description,
            status,
        }
    }

    /// Returns the id of `self`
    pub fn id(&self) -> u32 {
        self.id
    }

    /// Returns the epic id that contains `self`
    pub fn epic_id(&self) -> u32 { self.epic_id }

    /// Returns a shared reference to the name of `self`
    pub fn name(&self) -> &String {
        &self.name
    }

    /// Returns a shared reference to the description of `self`.
    pub fn description(&self) -> &String {
        &self.description
    }

    /// Returns the status of `self`
    pub fn status(&self) -> Status {
        self.status
    }
}

impl AsBytes for Story {
    fn as_bytes(&self) -> Vec<u8> {
        let mut bytes = vec![];
        bytes.extend_from_slice(&self.encode());
        bytes.extend_from_slice(self.name.as_bytes());
        bytes.extend_from_slice(self.description.as_bytes());
        bytes
    }
}

unsafe impl Send for Story {}
unsafe impl Sync for Story {}

impl BytesEncode for Story {
    type Tag = StoryEncodeTag;

    type DecodedTag = StoryDecodeTag;

    fn encode(&self) -> Self::Tag {
        let mut encoded_bytes = [0_u8; 17];
        for i in 0..4 {
            encoded_bytes[i] = ((self.id >> (i * 8)) & 0xff) as u8;
        }

        for i in 4..8 {
            encoded_bytes[i] = ((self.epic_id >> ((i % 4) * 8)) & 0xff) as u8;
        }

        let name_bytes_len = self.name.as_bytes().len();
        for i in 8..12 {
            encoded_bytes[i] = (((name_bytes_len as u32) >> (i % 4) * 8) & 0xff) as u8;
        }
        let description_bytes_len = self.description.as_bytes().len();
        for i in 12..16 {
            encoded_bytes[i] = (((description_bytes_len as u32) >> (i % 4) * 8) & 0xff) as u8;
        }
        let status_byte: u8 = self.status.into();
        encoded_bytes[16] = status_byte;
        encoded_bytes
    }

    fn decode(tag: &Self::Tag) -> Self::DecodedTag {
        let mut id = 0;
        for i in 0..4 {
            id ^= (tag[i] as u32) << (i * 8);
        }
        let mut epic_id = 0;
        for i in 4..8 {
            epic_id ^= (tag[i] as u32) << ((i % 4) * 8);
        }
        let mut name_len = 0;
        for i in 8..12 {
            name_len ^= (tag[i] as u32) << ((i % 4) * 8);
        }
        let mut description_len = 0;
        for i in 12..16 {
            description_len ^= (tag[i] as u32) << ((i % 4) * 8);
        }
        let status_byte = tag[16];
        (id, epic_id, name_len, description_len, status_byte)
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
            _ => false,
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
            _ => Err(DbError::ParseError(format!(
                "unable to parse `Status` from byte {value}"
            ))),
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

impl std::fmt::Display for Status {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Status::Open => write!(f, "{}", "OPEN"),
            Status::InProgress => write!(f, "{}", "IN PROGRESS"),
            Status::Resolved => write!(f, "{}", "RESOLVED"),
            Status::Closed => write!(f, "{}", "CLOSED"),
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
    InvalidUtf8Path(String),
}

impl std::fmt::Display for DbError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            DbError::FileLoadError(s) => write!(f, "{}", s),
            DbError::FileReadError(s) => write!(f, "{}", s),
            DbError::ParseError(s) => write!(f, "{}", s),
            DbError::FileWriteError(s) => write!(f, "{}", s),
            DbError::IdConflict(s) => write!(f, "{}", s),
            DbError::DoesNotExist(s) => write!(f, "{}", s),
            DbError::ConnectionError(s) => write!(f, "{}", s),
            DbError::InvalidUtf8Path(s) => write!(f, "{}", s),
        }
    }
}

impl std::error::Error for DbError {}

/// The decoded tag type that `Epic`s get decoded from. A tuple that represents
/// the id of the object, the length (in bytes) of its name and description, and a byte that
/// represents the status of the object.
pub type EpicDecodeTag = (u32, u32, u32, u8);

impl TagDecoding for EpicDecodeTag {}

impl TagDecoding for &EpicDecodeTag {}

/// The encoded tag type that `Epic`s get encoded into. An array of bytes,
/// that holds the encoded id of the object, the length (in bytes) of the name and description of
/// the object and a status byte that represents the object's status.
pub type EpicEncodeTag = [u8; 13];

impl TagEncoding for EpicEncodeTag {}

impl TagEncoding for &EpicEncodeTag {}

/// The decoded tag type that `Story`s get decoded from. A tuple that represents
/// the id of the object, the id of the `Epic` that contains the given `Story`,
/// the length (in bytes) of its name and description, and a byte that
/// represents the status of the object.
pub type StoryDecodeTag = (u32, u32, u32, u32, u8);

impl TagDecoding for StoryDecodeTag {}

impl TagDecoding for &StoryDecodeTag {}


/// The encoded tag type that `Story`s get encoded into. An array of bytes,
/// that holds the encoded id of the object, the id of the `Epic` that contains the given `Story`
/// the length (in bytes) of the name and description of
/// the object and a status byte that represents the object's status.
pub type StoryEncodeTag = [u8; 17];

impl TagEncoding for StoryEncodeTag {}

impl TagEncoding for &StoryEncodeTag {}



#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn create_story_should_work() {
        let story = Story::new(
            1,
            0,
            String::from("Test Story 1"),
            String::from("A simple test story"),
            Status::Open,
        );
        println!("{:?}", story);
        assert!(true)
    }
    #[test]
    fn story_encode_decode_should_work() {
        let story = Story::new(
            1,
            0,
            String::from("Test Story 1"),
            String::from("A simple test story"),
            Status::Open,
        );
        // Attempt to encode
        let story_tag = story.encode();

        println!("{:?}", story_tag);

        assert_eq!(story_tag, [1, 0, 0, 0, 0, 0, 0, 0, 12, 0, 0, 0, 19, 0, 0, 0, 0]);

        // Attempt to decode
        let (story_id, epic_id, story_name_len, story_description_len, story_status_byte) =
            <Story as BytesEncode>::decode(&story_tag);

        assert_eq!(1, story_id);
        assert_eq!(0, epic_id);
        assert_eq!(12, story_name_len);
        assert_eq!(19, story_description_len);
        assert_eq!(0, story_status_byte);
    }

    #[test]
    fn create_epic_should_work() {
        let epic = Epic::new(
            99,
            String::from("Test Epic 99"),
            String::from("A simple test epic"),
            Status::Open,
            PathBuf::new(),
            HashMap::new(),
        );

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
            HashMap::new(),
        );

        // Attempt to encode the epic
        let epic_tag = epic.encode();

        println!("{:?}", epic_tag);

        assert_eq!(epic_tag, [1, 0, 0, 0, 11, 0, 0, 0, 18, 0, 0, 0, 0]);

        // Attempt to decode the encoded tag
        let (epic_id, epic_name_len, epic_description_len, epic_status_byte) =
            Epic::decode(&epic_tag);

        assert_eq!(epic_id, 1);
        assert_eq!(epic_name_len, 11);
        assert_eq!(epic_description_len, 18);
        assert_eq!(epic_status_byte, 0);
    }

    #[test]
    fn test_epic_create_write_and_load_from_file() {
        let mut test_file_path = PathBuf::from("/Users/benjaminhaase/development/Personal/async_jira_cli/src/test_database/test_epics");
        test_file_path.push("epic99.txt");

        let mut epic = Epic::new(
            99,
            String::from("Test Epic 99"),
            String::from("A simple test epic"),
            Status::Open,
            test_file_path.clone(),
            HashMap::new(),
        );

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
        let mut test_file_path = PathBuf::from("/Users/benjaminhaase/development/Personal/async_jira_cli/src/test_database/test_epics");
        test_file_path.push("epic2.txt");

        let mut epic = Epic::new(
            100,
            String::from("Test Epic 2"),
            String::from("A simple test epic"),
            Status::Open,
            test_file_path.clone(),
            HashMap::new(),
        );

        println!("{:?}", epic);

        let test_story1 = Story::new(
            3,
            100,
            String::from("A Test Story"),
            String::from("A simple test story"),
            Status::Open,
        );
        let test_story2 = Story::new(
            4,
            100,
            String::from("A test Story"),
            String::from("A simple test story"),
            Status::Open,
        );
        let test_story3 = Story::new(
            5,
            100,
            String::from("A different test Story"),
            String::from("Another simple test story"),
            Status::InProgress,
        );

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
        let mut test_file_path = PathBuf::from("/Users/benjaminhaase/development/Personal/async_jira_cli/src/test_database/test_epics");
        test_file_path.push("epic101.txt");

        let mut epic = Epic::new(
            101,
            String::from("Test Epic 101"),
            String::from("A simple test epic"),
            Status::Open,
            test_file_path.clone(),
            HashMap::new(),
        );

        let test_story1 = Story::new(
            6,
            101,
            String::from("A Test Story"),
            String::from("A simple test story"),
            Status::Open,
        );
        let test_story2 = Story::new(
            7,
            101,
            String::from("A test Story"),
            String::from("A simple test story"),
            Status::Open,
        );
        let test_story3 = Story::new(
            8,
            101,
            String::from("A different test Story"),
            String::from("Another simple test story"),
            Status::InProgress,
        );

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
        let mut test_file_path = PathBuf::from("/Users/benjaminhaase/development/Personal/async_jira_cli/src/test_database/test_epics");
        test_file_path.push("epic102.txt");

        let mut epic = Epic::new(
            102,
            String::from("Test Epic 102"),
            String::from("A simple test epic"),
            Status::Open,
            test_file_path.clone(),
            HashMap::new(),
        );

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
            "/Users/benjaminhaase/development/Personal/async_jira_cli/src/bin/test_database"
                .to_string(),
            "test_db.txt".to_string(),
            "test_epics".to_string(),
        );

        println!("{:?}", db_state);
        assert!(true);
    }

    #[test]
    fn test_create_db_state_add_epics_write_and_read() {
        let mut db_state = DbState::new(
            "/Users/benjaminhaase/development/Personal/async_jira_cli/src/test_database"
                .to_string(),
            "test_db1.txt".to_string(),
            "test_epics1".to_string(),
        );

        assert_eq!(
            db_state.add_epic("Test Epic".to_string(), "A simple test epic".to_string()),
            1
        );
        assert_eq!(
            db_state.add_epic("Test Epic".to_string(), "A simple test epic".to_string()),
            2
        );
        assert_eq!(
            db_state.add_epic("Test Epic".to_string(), "A simple test epic".to_string()),
            3
        );

        // assert!(db_state.add_epic("Test Epic".to_string(), "A simple test epic".to_string()).is_err());

        println!("{:?}", db_state);

        assert!(db_state.write().is_ok());

        let mut db_state_prime_result = DbState::load(
            "/Users/benjaminhaase/development/Personal/async_jira_cli/src/test_database"
                .to_string(),
            "test_db1.txt".to_string(),
            "test_epics1".to_string(),
        );

        println!("{:?}", db_state_prime_result);

        assert!(db_state_prime_result.is_ok());

        let mut db_state_prime = db_state_prime_result.unwrap();

        assert_eq!(db_state, db_state_prime);
    }

    #[test]
    fn test_db_state_add_delete_epic() {
        let mut db_state = DbState::new(
            "/Users/benjaminhaase/development/Personal/async_jira_cli/src/test_database"
                .to_string(),
            "test_db2.txt".to_string(),
            "test_epics2".to_string(),
        );

        println!("{:?}", db_state);

        db_state.add_epic(
            "Test Epic".to_string(),
            "A specially added test epic".to_string(),
        );

        println!("{:?}", db_state);

        assert!(db_state.write().is_ok());

        let mut db_state = DbState::load(
            "/Users/benjaminhaase/development/Personal/async_jira_cli/src/test_database"
                .to_string(),
            "test_db2.txt".to_string(),
            "test_epics2".to_string(),
        )
        .expect("should load");

        println!("{:?}", db_state);

        let cur_max_id = db_state.last_unique_id;

        assert!(db_state.contains_epic(cur_max_id));

        assert!(db_state.delete_epic(cur_max_id).is_ok());

        assert!(db_state.delete_epic(cur_max_id).is_err());

        assert!(db_state.write().is_ok());

        let mut db_state_loaded = DbState::load(
            "/Users/benjaminhaase/development/Personal/async_jira_cli/src/test_database"
                .to_string(),
            "test_db2.txt".to_string(),
            "test_epics2".to_string(),
        )
        .expect("should load");

        println!("{:?}", db_state_loaded);

        assert!(!db_state_loaded.contains_epic(cur_max_id));
    }

    #[test]
    fn test_db_state_add_delete_story() {
        let mut db_state = DbState::load(
            "/Users/benjaminhaase/development/Personal/async_jira_cli/src/test_database".to_string(),
            "test_db.txt".to_string(),
            "test_epics".to_string(),
        ).expect("should load");

        println!("{:?}", db_state);

        let cur_epic_id = db_state.add_epic(
            "Test Epic".to_string(),
            "A test Epic for adding/deleting stories".to_string(),
        );

        println!("{:?}", cur_epic_id);

        assert!(db_state
            .add_story(
                cur_epic_id,
                "Test Story".to_string(),
                "A Test story for adding/deleting stories".to_string()
            )
            .is_ok());
        assert!(db_state
            .add_story(
                cur_epic_id,
                "Test Story".to_string(),
                "A Test story for adding/deleting stories".to_string()
            )
            .is_ok());
        assert!(db_state
            .add_story(
                cur_epic_id,
                "Test Story".to_string(),
                "A Test story for adding/deleting stories".to_string()
            )
            .is_ok());

        println!("{:?}", db_state);

        assert!(db_state.write().is_ok());

        let mut db_state = DbState::load(
            "/Users/benjaminhaase/development/Personal/async_jira_cli/src/test_database"
                .to_string(),
            "test_db.txt".to_string(),
            "test_epics".to_string(),
        )
        .expect("should load");

        println!("{:?}", db_state);

        assert!(db_state.contains_epic(cur_epic_id));

        let cur_story_id = db_state.last_unique_id;

        assert!(db_state.delete_story(cur_epic_id, cur_story_id).is_ok());

        assert!(db_state.delete_story(cur_epic_id, cur_story_id).is_err());
    }

    #[test]
    fn test_story_as_bytes() {
        let story = Story::new(1, 101,"S1".to_string(), "TS1".to_string(), Status::Open);

        let encoding = story.encode();
        println!("{:?}", encoding);

        let bytes = story.as_bytes();
        println!("{:?}", bytes);

        assert_eq!(
            bytes,
            vec![1, 0, 0, 0, 101, 0, 0, 0, 2, 0, 0, 0, 3, 0, 0, 0, 0, 83, 49, 84, 83, 49]
        );
    }

    #[test]
    fn test_epic_as_bytes() {
        let mut epic = Epic::new(
            129,
            "E129".to_string(),
            "TE129".to_string(),
            Status::Open,
            PathBuf::new(),
            HashMap::new(),
        );

        let encoding = epic.encode();
        println!("{:?}", encoding);

        let bytes = epic.as_bytes();
        println!("{:?}", bytes);

        assert_eq!(
            bytes,
            vec![129, 0, 0, 0, 4, 0, 0, 0, 5, 0, 0, 0, 0, 69, 49, 50, 57, 84, 69, 49, 50, 57]
        );

        let story = Story::new(1, 129,"S1".to_string(), "TS1".to_string(), Status::Open);

        let _ = epic.add_story(story);

        let bytes = epic.as_bytes();

        println!("{:?}", bytes);

        assert_eq!(
            bytes,
            vec![
                129, 0, 0, 0, 4, 0, 0, 0, 5, 0, 0, 0, 0, 69, 49, 50, 57, 84, 69, 49, 50, 57, 1, 0,
                0, 0, 129, 0, 0, 0, 2, 0, 0, 0, 3, 0, 0, 0, 0, 83, 49, 84, 83, 49
            ]
        );
    }

    #[test]
    fn test_async_db_state_as_bytes() {
        let mut db_state = DbState::new("".to_string(), "".to_string(), "".to_string());
        let mut epic = Epic::new(
            129,
            "E129".to_string(),
            "TE129".to_string(),
            Status::Open,
            PathBuf::new(),
            HashMap::new(),
        );

        let epic_bytes = epic.as_bytes();
        db_state.epics.insert(epic.id, epic);

        let bytes = db_state.as_bytes();

        println!("{:?}", bytes);

        assert_eq!(
            bytes,
            vec![129, 0, 0, 0, 4, 0, 0, 0, 5, 0, 0, 0, 0, 69, 49, 50, 57, 84, 69, 49, 50, 57]
        );
        println!("{}", bytes.starts_with(epic_bytes.as_slice()));

        let new_epic = Epic::new(
            130,
            "E130".to_string(),
            "TE130".to_string(),
            Status::Open,
            PathBuf::new(),
            HashMap::new(),
        );

        let new_epic_bytes = new_epic.as_bytes();
        db_state.epics.insert(new_epic.id, new_epic);

        let bytes = db_state.as_bytes();

        println!("{:?}", bytes);

        assert!(bytes.starts_with(epic_bytes.as_slice()) || bytes.ends_with(epic_bytes.as_slice()));
        assert!(
            bytes.starts_with(new_epic_bytes.as_slice())
                || bytes.ends_with(new_epic_bytes.as_slice())
        );
    }

    #[test]
    fn test_async_db_state_read_and_write() {
        use async_std::task;
        let mut db_state = DbState::new(
            "/Users/benjaminhaase/development/Personal/async_jira_cli/src/test_database"
                .to_string(),
            "test_async_db1.txt".to_string(),
            "async_test_epics1".to_string(),
        );

        assert_eq!(
            db_state.add_epic("Test Epic".to_string(), "A simple test Epic".to_string()),
            1
        );
        assert_eq!(
            db_state.add_epic("Test Epic".to_string(), "A simple test Epic".to_string()),
            2
        );
        assert_eq!(
            db_state.add_epic("Test Epic".to_string(), "A simple test Epic".to_string()),
            3
        );

        println!("{:?}", db_state);

        let handle = task::block_on(db_state.write_async());

        println!("{:?}", handle);

        assert!(handle.is_ok());

        let mut db_state_loaded_res = DbState::load(
            "/Users/benjaminhaase/development/Personal/async_jira_cli/src/test_database"
                .to_string(),
            "test_async_db1.txt".to_string(),
            "async_test_epics1".to_string(),
        );

        assert!(db_state_loaded_res.is_ok());

        assert_eq!(db_state, db_state_loaded_res.unwrap());
    }

    #[test]
    fn test_async_db_state_read_write_delete_epic() {
        use async_std::task;
        let mut db_state = DbState::new(
            "/Users/benjaminhaase/development/Personal/async_jira_cli/src/test_database"
                .to_string(),
            "test_async_db2.txt".to_string(),
            "async_test_epics2".to_string(),
        );

        assert_eq!(
            db_state.add_epic("Test Epic".to_string(), "A simple test Epic".to_string()),
            1
        );
        assert_eq!(
            db_state.add_epic("Test Epic".to_string(), "A simple test Epic".to_string()),
            2
        );
        assert_eq!(
            db_state.add_epic("Test Epic".to_string(), "A simple test Epic".to_string()),
            3
        );
        assert_eq!(
            db_state.add_epic("Test Epic".to_string(), "A simple test Epic".to_string()),
            4
        );

        println!("{:?}", db_state);

        let handle = task::block_on(db_state.write_async());

        println!("{:?}", handle);

        assert!(handle.is_ok());

        let mut db_state_loaded_res = DbState::load(
            "/Users/benjaminhaase/development/Personal/async_jira_cli/src/test_database"
                .to_string(),
            "test_async_db2.txt".to_string(),
            "async_test_epics2".to_string(),
        );

        assert!(db_state_loaded_res.is_ok());

        assert_eq!(db_state, db_state_loaded_res.unwrap());

        assert!(db_state.contains_epic(4));

        assert!(db_state.delete_epic(4).is_ok());

        assert!(!db_state.contains_epic(4));

        println!("{:?}", db_state);

        let handle = task::block_on(db_state.write_async());

        println!("{:?}", handle);

        assert!(handle.is_ok());

        let db_state_loaded_res = DbState::load(
            "/Users/benjaminhaase/development/Personal/async_jira_cli/src/test_database"
                .to_string(),
            "test_async_db2.txt".to_string(),
            "async_test_epics2".to_string(),
        );

        assert!(db_state_loaded_res.is_ok());

        let db_state_loaded = db_state_loaded_res.unwrap();

        assert!(!db_state_loaded.contains_epic(4));
    }

    #[test]
    fn test_async_db_read_write_add_story() {
        use async_std::task;
        let mut db_state = DbState::new(
            "/Users/benjaminhaase/development/Personal/async_jira_cli/src/test_database"
                .to_string(),
            "test_async_db3.txt".to_string(),
            "async_test_epics3".to_string(),
        );

        assert_eq!(
            db_state.add_epic("Test Epic".to_string(), "A simple test Epic".to_string()),
            1
        );
        assert_eq!(
            db_state.add_epic("Test Epic".to_string(), "A simple test Epic".to_string()),
            2
        );
        assert_eq!(
            db_state.add_epic("Test Epic".to_string(), "A simple test Epic".to_string()),
            3
        );
        assert_eq!(
            db_state.add_epic("Test Epic".to_string(), "A simple test Epic".to_string()),
            4
        );

        println!("{:?}", db_state);

        assert!(db_state.contains_epic(4));

        let handle = task::block_on(db_state.add_story_async(
            4,
            "A simple test story".to_string(),
            "A simple test story decsription".to_string(),
        ));

        println!("{:?}", handle);

        assert!(handle.is_ok());

        println!("{:?}", db_state);

        assert!(task::block_on(db_state.write_async()).is_ok());

        let mut db_state_loaded_res = DbState::load(
            "/Users/benjaminhaase/development/Personal/async_jira_cli/src/test_database"
                .to_string(),
            "test_async_db3.txt".to_string(),
            "async_test_epics3".to_string(),
        );

        assert!(db_state_loaded_res.is_ok());

        let db_state_loaded = db_state_loaded_res.unwrap();

        assert!(db_state_loaded.contains_epic(4));

        let epic = db_state_loaded.get_epic(4).unwrap();

        assert!(epic.get_story(5).is_some());

        // Now try deleting the story
        let epic = db_state.get_epic_mut(4).unwrap();

        assert!(epic.delete_story(5).is_ok());

        let handle = task::block_on(epic.write_async());

        println!("{:?}", handle);

        assert!(handle.is_ok());

        let handle = task::block_on(db_state.write_async());

        println!("{:?}", handle);

        assert!(handle.is_ok());

        // Ensure changes persist
        let mut db_state_loaded_res = DbState::load(
            "/Users/benjaminhaase/development/Personal/async_jira_cli/src/test_database"
                .to_string(),
            "test_async_db3.txt".to_string(),
            "async_test_epics3".to_string(),
        );

        assert!(db_state_loaded_res.is_ok());

        let db_state_loaded = db_state_loaded_res.unwrap();

        assert!(db_state_loaded.contains_epic(4));

        let epic = db_state_loaded.get_epic(4).unwrap();

        assert!(epic.get_story(5).is_none());
    }
}
