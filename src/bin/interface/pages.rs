//! Contains helpful structs and traits for creating a page based CLI

use crate::interface::frame::prelude::*;
use crate::UserError;
use async_jira_cli::models::prelude::*;
use async_jira_cli::utils::prelude::*;
use async_jira_cli::events::prelude::*;
// use async_std::io::{ReadExt, BufRead};
use std::io::{Read, BufRead, Cursor, Seek, ErrorKind};
// use async_std::io::prelude::BufReadExt;
use unicode_width;

pub mod prelude {
    pub use super::*;
}

// TODO: 1) implement parse option methods for pages
//          Method should accept an option represented as a &str and a handle on Stdin
//          Then either attempt to build a valid request or print an error message to the console

/// The `Page` trait allows for different types of CLI interface pages to share
/// the functionality of any typical CLI page i.e. `print_page`. The main purpose is to allow
/// different types of pages to be held in a single data structure as trait objects.
pub trait Page<R: BufRead> {
    /// Required method, prints the page's contents to the console in a way that is formatted 
    fn print_page(&self);
    /// Required method, takes `request_option` and `input_reader` and attempts to create 
    /// an `Action`. Returns `Result`, the `Ok` variant if the request was parsed successfully, otherwise
    /// the `Err` variant.
    fn parse_request(&self, request_option: &str, input_reader: R) -> Result<Action, UserError>;
}

/// Represents the possible states that prompt the `Interface` to update.
#[derive(Debug, PartialEq)]
pub enum Action {
    Quit,
    PreviousPage,
    Refresh,
    RequestParsed(Vec<u8>)
}

impl Action {
    pub fn is_quit(&self) -> bool {
        *self == Action::Quit
    }

    pub fn is_previous_page(&self) -> bool {
        *self == Action::PreviousPage
    }

    pub fn is_refresh(&self) -> bool {
        *self == Action::Refresh
    }

    pub fn is_request_parsed(&self) -> bool {
        !(self.is_quit() || self.is_previous_page() || self.is_refresh())
    }
}

/// Represents the first page that the user will see.
#[derive(Debug)]
pub struct HomePage {
    epic_frames: Vec<EpicFrame>
}

impl HomePage {
    /// Associated method  for accepting a request option then performing the appropriate logic for constructing
    /// the request, i.e. prompting the user for input or displaying an error message etc...
    // pub async fn parse_request<R: BufRead + ReadExt + Unpin>(request_option: &str, input_reader: R) -> Result<Option<Vec<u8>>, UserError> {
    //     if request_option.to_lowercase() == "q" {
    //         return Ok(None);
    //     } else if request_option.to_lowercase() == "c" {
    //         // User has selected to add a new epic to the database
    //         let mut input_reader = input_reader;
    //         let mut epic_name = String::new();
    // 
    //         println!("epic name:");
    // 
    //         let _ = input_reader.read_line(&mut epic_name)
    //             .await
    //             .map_err(|_| UserError::ParseInputError)?;
    // 
    //         // Remove new line character from name
    //         epic_name = epic_name.trim_end_matches('\n').to_string();
    // 
    //         // Ensure the length of the name can fit inside 8 bytes
    //         if epic_name.as_bytes().len() > u32::MAX as usize {
    //             return Err(UserError::InvalidInput(String::from("Invalid input, epic's name is to large")));
    //         }
    //         println!();
    // 
    //         let mut epic_description = String::new();
    // 
    //         println!("epic description: ");
    // 
    //         let _ = input_reader.read_line(&mut epic_description)
    //             .await
    //             .map_err(|_| UserError::ParseInputError)?;
    // 
    //         // Remove the new line character from description
    //         epic_description = epic_description.trim_end_matches('\n').to_string();
    // 
    //         // Ensure the length of the description can fit inside 8 bytes
    //         if epic_description.as_bytes().len()  > u32::MAX as usize {
    //             return Err(UserError::InvalidInput(String::from("Invalid input, epic's description is to large")));
    //         }
    // 
    //         // Get the bytes for the request from the tag, name and description
    //         let mut request_bytes = Event::add_epic_tag(&epic_name, &epic_description).to_vec();
    //         request_bytes.extend_from_slice(epic_name.as_bytes());
    //         request_bytes.extend_from_slice(epic_description.as_bytes());
    //         Ok(Some(request_bytes))
    //     } else if let Ok(id) = request_option.parse::<u32>() {
    //         // User has selected to request details for an epic
    //         let request_bytes = Event::get_epic_tag(id).to_vec();
    //         Ok(Some(request_bytes))
    //     } else {
    //         Err(UserError::InvalidRequest)
    //     }
    // }

    /// Associated method, attempts to create a new `HomePage` struct from `data`.
    /// Returns a `Result`, the `Ok` variant if creating was successful, otherwise `Err`.
    pub fn try_create(mut data: Vec<u8>) -> Result<Self, UserError> {
        let data_len = data.len() as u64;
        let mut cursor = Cursor::new(data);
        let mut tag_buf = [0u8; 13];
        let mut cur_pos = cursor.position();
        let mut epic_frames = vec![];

        loop {
            // Ensure we can read properly a properly formatted Epic tag
            match cursor.read_exact(&mut tag_buf) {
                Ok(_) => {
                    let epic_frame = EpicFrame::try_from_reader(&tag_buf, &mut cursor)?;
                    epic_frames.push(epic_frame);
                }
                Err(e) if e.kind() == ErrorKind::UnexpectedEof => {
                    // Check that we have reached the end of the cursor, and that the current position
                    // is equal to the cursor's current position. If either of these conditions is not true
                    // we know the data was not formatted correctly and we should propagate an error.
                    if cur_pos != cursor.position() || cursor.position() != data_len {
                        return Err(UserError::ReadFrameError("unable to read frame from response data, data may not be formatted correctly".to_string()));
                    }
                    break;
                }
                _ => return Err(UserError::ReadFrameError("unable to read EpicFrame from response data, data may not be formatted correctly".to_string())),
            }
            // Keep track of the current position,
            // used to ensure that we have read properly formatted data from the server
            cur_pos = cursor.position();
        }

        Ok(HomePage { epic_frames })
    }

    /// A helper function for displaying a single line in the implementation of `print_page`.
    fn print_line(epic_frame: &EpicFrame) {
        print!("{:<13}|", epic_frame.id);
        print!(" {:<32}|", justify_text_with_ellipses(32, &epic_frame.name));
        println!(" {:<15}", epic_frame.status)
    }
}

impl<R: std::io::BufRead> Page<R> for HomePage {
    fn print_page(&self) {
        println!("{:-^65}", "EPICS");
        print!("{:^13}|", "id");
        print!("{:^33}|", "name");
        println!("{:^16}", "status");
        for epic_frame in &self.epic_frames {
            HomePage::print_line(epic_frame);
        }
        println!();
        println!();
        println!("[q] quit | [c] create epic | [:id:] navigate to epic");
        println!();
    }

    fn parse_request(&self, request_option: &str, input_reader: R) -> Result<Action, UserError> {
        if request_option.to_lowercase() == "q" {
            return Ok(Action::Quit);
        } else if request_option.to_lowercase() == "c" {
            // User has selected to add a new epic to the database
            let mut input_reader = input_reader;
            let mut epic_name = String::new();

            println!("epic name:");

            let _ = input_reader.read_line(&mut epic_name)
                .map_err(|_| UserError::ParseInputError)?;

            // Remove new line character from name
            let epic_name = epic_name.trim_end_matches('\n').to_string();

            // Ensure the length of the name can fit inside 8 bytes
            if epic_name.as_bytes().len() > u32::MAX as usize {
                return Err(UserError::InvalidInput(String::from("Invalid input, epic's name is to large")));
            }
            println!();

            let mut epic_description = String::new();

            println!("epic description: ");

            let _ = input_reader.read_line(&mut epic_description)
                .map_err(|_| UserError::ParseInputError)?;

            // Remove the new line character from description
            let epic_description = epic_description.trim_end_matches('\n').to_string();

            // Ensure the length of the description can fit inside 8 bytes
            if epic_description.as_bytes().len()  > u32::MAX as usize {
                return Err(UserError::InvalidInput(String::from("Invalid input, epic's description is to large")));
            }

            // Get the bytes for the request from the tag, name and description
            let mut request_bytes = Event::add_epic_tag(&epic_name, &epic_description).to_vec();
            request_bytes.extend_from_slice(epic_name.as_bytes());
            request_bytes.extend_from_slice(epic_description.as_bytes());
            Ok(Action::RequestParsed(request_bytes))
        } else if let Ok(id) = request_option.parse::<u32>() {
            // User has selected to request details for an epic
            let request_bytes = Event::get_epic_tag(id).to_vec();
            Ok(Action::RequestParsed(request_bytes))
        } else {
            Err(UserError::InvalidRequest)
        }
    }
}

/// A page for displaying the details of a specific Epic.
#[derive(Debug)]
pub struct EpicDetailPage {
    frame: EpicFrame,
    story_frames: Vec<StoryFrame>,
}

impl EpicDetailPage {
    /// Attempts to create a new `EpicDetailPage` from `data`. Returns a `Result`, the `Ok` variant
    /// if creating was successful, otherwise `Err`.
    pub fn try_create(data: Vec<u8>) -> Result<Self, UserError> {
        let data_len = data.len() as u64;
        let mut cursor = Cursor::new(data);
        let mut epic_tag_buf = [0u8; 13];
        let mut cur_pos = cursor.position();

        // Attempt to parse the first epic frame from the stream of bytes
        let frame = match cursor.read_exact(&mut epic_tag_buf) {
            Ok(_) => EpicFrame::try_from_reader(&epic_tag_buf, &mut cursor)?,
            Err(_) => return Err(UserError::ReadFrameError(String::from("unable to read EpicFrame from response data, data may not be formatted correctly"))),
        };

        // Reset current position of cursor
        cur_pos = cursor.position();
        let mut story_frames = vec![];
        let mut story_tag_buf = [0u8; 17];

        loop {
            match cursor.read_exact(&mut story_tag_buf) {
                Ok(_) => {
                    let story_frame = StoryFrame::try_from_reader(&story_tag_buf, &mut cursor)?;
                    story_frames.push(story_frame);
                }
                Err(e) if e.kind() == ErrorKind::UnexpectedEof => {
                    // Check that we have reached the end of the cursor, and that the current position
                    // is equal to the cursor's current position. If either of these conditions is not true
                    // we know the data was not formatted correctly and we should propagate an error.
                    if cur_pos != cursor.position() || cursor.position() != data_len {
                        return Err(UserError::ReadFrameError("error reading frame data from stream, data may not be formatted correctly".to_string()));
                    }
                    break;
                }
                Err(_) => return Err(UserError::ReadFrameError(String::from("unable to read frame tag from response data, data may not be formatted correctly"))),
            }

            cur_pos = cursor.position();
        }

        Ok(EpicDetailPage { frame, story_frames })
    }

    /// Attempts to create a vector of bytes that encodes the request received from user input.
    /// Takes `request_option` which represents the user request and `input_reader` which is the
    /// asynchronous reader that the clients input will be read from. Returns a `Result`, the `Ok` variant if
    /// the request was created successfully, otherwise
    // pub async fn parse_request<R: BufRead + ReadExt + Unpin>(&self, request_option: &str, input_reader: R) -> Result<Option<Vec<u8>>, UserError> {
    //     if request_option.to_lowercase() == "p" {
    //         Ok(None)
    //     } else if request_option.to_lowercase() == "u" {
    //         // Update epic's status in this case
    //         let mut input_reader = input_reader;
    //         let mut new_status_buf = String::new();
    //
    //         // read the new status from the user, use a validation loop
    //         let new_status = loop {
    //             self.print_status_update_menu();
    //             match input_reader.read_line(&mut new_status_buf).await {
    //                 Ok(n) if n > 0 => {
    //                     // Remove new line character from buffer
    //                     new_status_buf = new_status_buf.trim_end().to_string();
    //                     match new_status_buf.parse::<u8>() {
    //                         Ok(s) if (0..4u8).contains(&s) => {
    //                             break s;
    //                         }
    //                         Ok(s) => {
    //                             println!("invalid status selection, please try again");
    //                             eprintln!("invalid status selection, please try again");
    //                         }
    //                         Err(_) => {
    //                             println!("unable to parse status selection, please try again");
    //                             eprintln!("unable to parse status selection, please try again");
    //                         }
    //                     }
    //                 }
    //                 Ok(n) => {
    //                     println!("no input entered, please try again");
    //                     eprintln!("no input entered, please try again");
    //                 }
    //                 Err(_) => {
    //                     println!("unable to read input, please try again");
    //                     eprintln!("unable to read input, please try again");
    //                 }
    //             }
    //             new_status_buf.clear();
    //         };
    //         // Create the request bytes
    //         let request_bytes = Event::get_update_epic_status_tag(self.frame.id, new_status).to_vec();
    //         Ok(Some(request_bytes))
    //     } else if request_option.to_lowercase() == "d" {
    //         let mut input_reader = input_reader;
    //
    //         // Delete the epic
    //         println!("are you sure you want to delete epic {}? (y|n)", self.frame.id);
    //         let mut user_final_choice = String::new();
    //
    //         let request_bytes = loop {
    //             match input_reader.read_line(&mut user_final_choice).await {
    //                 Ok(n) if n > 0 => {
    //                     match user_final_choice.trim_end().to_lowercase().as_str() {
    //                         "y" => break Event::delete_epic_tag(self.frame.id).to_vec(),
    //                         "n" => break vec![],
    //                         _ => {
    //                             println!("please choose a valid option");
    //                             println!("are you sure you want to delete epic {}? (y|n)", self.frame.id);
    //                         }
    //                     }
    //                 }
    //                 Ok(n) => {
    //                     println!("no input entered, please choose a valid option");
    //                     eprintln!("no input entered, please choose a valid option");
    //                 }
    //                 Err(e) => {
    //                     println!("unable to read input, please try again");
    //                     eprintln!("unable to read input, please try again");
    //                 }
    //             }
    //             user_final_choice.clear();
    //         };
    //
    //         Ok(Some(request_bytes))
    //     } else if request_option.to_lowercase() == "c" {
    //         // Create a new story
    //         let mut input_reader = input_reader;
    //         let mut story_name = String::new();
    //
    //         println!("story name:");
    //
    //         input_reader.read_line(&mut story_name)
    //             .await
    //             .map_err(|_| UserError::ParseInputError)?;
    //
    //         // remove new line character at the end
    //         story_name = story_name.trim_end_matches('\n').to_string();
    //
    //         // ensure the length of story_name in bytes can fit within 8 bytes
    //         if story_name.as_bytes().len()  > u32::MAX as usize {
    //             return Err(UserError::InvalidInput(String::from("invalid input, story's name is too large")));
    //         }
    //
    //         println!();
    //
    //         let mut story_description = String::new();
    //         println!("story description: ");
    //
    //         input_reader.read_line(&mut story_description)
    //             .await
    //             .map_err(|_| UserError::ParseInputError)?;
    //
    //         // remove new line character at the end
    //         story_description = story_description.trim_end_matches('\n').to_string();
    //
    //         if story_description.as_bytes().len() > u32::MAX as usize {
    //             return Err(UserError::InvalidInput(String::from("invalid input, story's description is too large")));
    //         }
    //
    //         let mut request_bytes = Event::add_story_tag(self.frame.id, &story_name, &story_description).to_vec();
    //         request_bytes.extend_from_slice(story_name.as_bytes());
    //         request_bytes.extend_from_slice(story_description.as_bytes());
    //
    //         Ok(Some(request_bytes))
    //     } else if let Ok(story_id) = request_option.parse::<u32>() {
    //         // navigate to story detail page
    //         let request_bytes = Event::get_story_tag(self.frame.id, story_id).to_vec();
    //         Ok(Some(request_bytes))
    //     } else {
    //         Err(UserError::InvalidRequest)
    //     }
    // }

    /// A helper function for displaying a single line in the implementation of `print_page`.
    fn print_line(story_frame: &StoryFrame) {
        print!("{:<13}|", story_frame.id);
        print!(" {:<32}|", justify_text_with_ellipses(32, &story_frame.name));
        println!(" {:<15}", story_frame.status);
    }

    /// A helper function for displaying the status selection menu
    fn print_status_update_menu(&self) {
        println!("please select the status you would like to update epic {} with", self.frame.id);
        println!("{:<11} - 0", "Open");
        println!("{:<11} - 1", "In Progress");
        println!("{:<11} - 2", "Resolved");
        println!("{:<11} - 3", "Closed");
    }
}

impl<R: std::io::BufRead> Page<R> for EpicDetailPage {
    fn print_page(&self) {
        println!("{:-^65}", "EPIC");
        print!("{:^6}| ", "id");
        print!("{:^14}| ", "name");
        print!("{:^29}| ", "description");
        println!("{:^12}", "status");

        print!("{:<6}| ", self.frame.id);
        print!("{:^13} | ", justify_text_with_ellipses(12, &self.frame.name));
        print!("{:^28} | ", justify_text_with_ellipses(27, &self.frame.description));
        println!("{:^10}", self.frame.status);
        println!();

        println!("{:-^65}", "STORIES");
        print!("{:^13}|", "id");
        print!("{:^33}|", "name");
        println!("{:^16}", "status");

        for story_frame in &self.story_frames {
            EpicDetailPage::print_line(story_frame);
        }

        println!();
        println!();
        println!("[p] previous | [u] update epic | [d] delete epic | [c] create story | [:id:] navigate to story");
        println!();
    }

    fn parse_request(&self, request_option: &str, input_reader: R) -> Result<Action, UserError> {
        if request_option.to_lowercase() == "p" {
            Ok(Action::PreviousPage)
        } else if request_option.to_lowercase() == "u" {
            // Update epic's status in this case
            let mut input_reader = input_reader;
            let mut new_status_buf = String::new();

            // read the new status from the user, use a validation loop
            let new_status = loop {
                self.print_status_update_menu();
                match input_reader.read_line(&mut new_status_buf) {
                    Ok(n) if n > 0 => {
                        // Remove new line character from buffer
                        new_status_buf = new_status_buf.trim_end().to_string();
                        match new_status_buf.parse::<u8>() {
                            Ok(s) if (0..4u8).contains(&s) => {
                                break s;
                            }
                            Ok(s) => {
                                println!("invalid status selection, please try again");
                                eprintln!("invalid status selection, please try again");
                            }
                            Err(_) => {
                                println!("unable to parse status selection, please try again");
                                eprintln!("unable to parse status selection, please try again");
                            }
                        }
                    }
                    Ok(n) => {
                        println!("no input entered, please try again");
                        eprintln!("no input entered, please try again");
                    }
                    Err(_) => {
                        println!("unable to read input, please try again");
                        eprintln!("unable to read input, please try again");
                    }
                }
                new_status_buf.clear();
            };
            // Create the request bytes
            let request_bytes = Event::get_update_epic_status_tag(self.frame.id, new_status).to_vec();
            Ok(Action::RequestParsed(request_bytes))
        } else if request_option.to_lowercase() == "d" {
            let mut input_reader = input_reader;

            // Delete the epic
            println!("are you sure you want to delete epic {}? (y|n)", self.frame.id);
            let mut user_final_choice = String::new();

            let request_bytes = loop {
                match input_reader.read_line(&mut user_final_choice) {
                    Ok(n) if n > 0 => {
                        match user_final_choice.trim_end().to_lowercase().as_str() {
                            "y" => break Event::delete_epic_tag(self.frame.id).to_vec(),
                            "n" => return Ok(Action::Refresh),
                            _ => {
                                println!("please choose a valid option");
                                println!("are you sure you want to delete epic {}? (y|n)", self.frame.id);
                            }
                        }
                    }
                    Ok(_n) => {
                        println!("no input entered, please choose a valid option");
                        eprintln!("no input entered, please choose a valid option");
                    }
                    Err(_e) => {
                        println!("unable to read input, please try again");
                        eprintln!("unable to read input, please try again");
                    }
                }
                user_final_choice.clear();
            };

            Ok(Action::RequestParsed(request_bytes))
        } else if request_option.to_lowercase() == "c" {
            // Create a new story
            let mut input_reader = input_reader;
            let mut story_name = String::new();

            println!("story name:");

            input_reader.read_line(&mut story_name)
                .map_err(|_| UserError::ParseInputError)?;

            // remove new line character at the end
            story_name = story_name.trim_end_matches('\n').to_string();

            // ensure the length of story_name in bytes can fit within 8 bytes
            if story_name.as_bytes().len()  > u32::MAX as usize {
                return Err(UserError::InvalidInput(String::from("invalid input, story's name is too large")));
            }

            println!();

            let mut story_description = String::new();
            println!("story description: ");

            input_reader.read_line(&mut story_description)
                .map_err(|_| UserError::ParseInputError)?;

            // remove new line character at the end
            story_description = story_description.trim_end_matches('\n').to_string();

            if story_description.as_bytes().len() > u32::MAX as usize {
                return Err(UserError::InvalidInput(String::from("invalid input, story's description is too large")));
            }

            let mut request_bytes = Event::add_story_tag(self.frame.id, &story_name, &story_description).to_vec();
            request_bytes.extend_from_slice(story_name.as_bytes());
            request_bytes.extend_from_slice(story_description.as_bytes());

            Ok(Action::RequestParsed(request_bytes))
        } else if let Ok(story_id) = request_option.parse::<u32>() {
            // navigate to story detail page
            let request_bytes = Event::get_story_tag(self.frame.id, story_id).to_vec();
            Ok(Action::RequestParsed(request_bytes))
        } else {
            Err(UserError::InvalidRequest)
        }
    }
}

/// Represents page to display the details of a single story
#[derive(Debug)]
pub struct StoryDetailPage {
    /// The frame of the `Story` that the `StoryDetailPage` is representing
    story_frame: StoryFrame,
}

impl StoryDetailPage {
    /// Attempts to create a new `StoryDetailPage` from `data`
    pub fn try_create(data: Vec<u8>) -> Result<Self, UserError> {
        let data_len = data.len() as u64;
        let mut cursor = Cursor::new(data);
        let mut tag_buf = [0u8; 17];

        let story_frame = match cursor.read_exact(&mut tag_buf) {
            Ok(_) => StoryFrame::try_from_reader(&tag_buf, &mut cursor)?,
            Err(_) => return Err(UserError::ReadFrameError(String::from("unable to read story frame tag from response data, data may not have been formatted correctly"))),
        };

        // Ensure that we have read the exact number of bytes needed, otherwise there was a formatting error
        if cursor.position() != data_len {
            return Err(UserError::ReadFrameError(String::from("error reading frame from response data, data may not have been formatted correctly")));
        }

        Ok(StoryDetailPage { story_frame })
    }

    /// Attempts to create a vector of bytes that encodes the request received from user input.
    /// Takes `request_option` which represents the user request and `input_reader` which is the
    /// asynchronous reader that the clients input will be read from. Returns a `Result`, the `Ok` variant if
    /// the request was created successfully, otherwise returns `Err`
    // pub async fn parse_request<R: ReadExt + Unpin>(&self, request_option: &str, input_reader: R) -> Result<Option<Vec<u8>>, UserError> {
    //     if request_option.to_lowercase() == "p" {
    //         Ok(None)
    //     } else if request_option.to_lowercase() == "u" {
    //         let mut input_reader = input_reader;
    //         let mut status_buf = String::new();
    //
    //         // Validation loop for getting new status from the user
    //         let new_status = loop {
    //             self.print_status_update_menu();
    //             match input_reader.read_to_string(&mut status_buf).await {
    //                 Ok(_n) => {
    //                     // Remove new line character from buffer
    //                     match status_buf.trim_end().parse::<u8>() {
    //                         Ok(s) if (0..4).contains(&s) => break s,
    //                         Ok(_s)  => {
    //                             println!("error parsing input as a valid status, please try again");
    //                             eprintln!("error parsing input as a valid status, please try again");
    //                         }
    //                         Err(e) => {
    //                             println!("error parsing input, please try again");
    //                             eprintln!("error parsing input, please try again");
    //                         }
    //                     }
    //                 }
    //                 Err(_) => {
    //                     println!("error reading input, please try again");
    //                     eprintln!("error reading input, please try again");
    //                 }
    //             }
    //             status_buf.clear();
    //         };
    //
    //         let request_bytes = Event::get_update_story_status_tag(self.story_frame.epic_id, self.story_frame.id, new_status).to_vec();
    //         Ok(Some(request_bytes))
    //     } else if request_option.to_lowercase() == "d" {
    //         let mut user_final_choice = String::new();
    //         let mut input_reader = input_reader;
    //         println!("are you sure you want to delete story {}? (y|n)", self.story_frame.id);
    //         let request_bytes = loop {
    //             match input_reader.read_to_string(&mut user_final_choice).await {
    //                 Ok(n) if n > 0 => {
    //                     match user_final_choice.trim_end() {
    //                         "y" => break Event::get_delete_story_tag(self.story_frame.epic_id, self.story_frame.id).to_vec(),
    //                         "n" => break vec![],
    //                         _ => {
    //                             println!("please choose a valid option");
    //                             println!("are you sure you want to delete story {}? (y|n)", self.story_frame.id);
    //                         }
    //                     }
    //                 }
    //                 Ok(n) => {
    //                     println!("no input entered, please try again");
    //                     eprintln!("no input entered, please try again");
    //                 }
    //                 Err(e) => {
    //                     println!("error reading input, please try again");
    //                     eprintln!("error reading input, please try again");
    //                 }
    //             }
    //             user_final_choice.clear();
    //         };
    //         Ok(Some(request_bytes))
    //     } else {
    //         Err(UserError::InvalidRequest)
    //     }
    // }

    /// A helper function for displaying the status selection menu
    fn print_status_update_menu(&self) {
        println!("please select the status you would like to update story {} with", self.story_frame.id);
        println!("{:<11} - 0", "Open");
        println!("{:<11} - 1", "In Progress");
        println!("{:<11} - 2", "Resolved");
        println!("{:<11} - 3", "Closed");
    }
}

impl<R: std::io::BufRead> Page<R> for StoryDetailPage {
    fn print_page(&self) {
        println!("{:-^65}", "Story");
        print!("{:^6}|", "id");
        print!("{:^14}|", "name");
        print!("{:^29}|", "description");
        println!("{:^12}", "status");
        // display story details
        print!("{:<6}| ", self.story_frame.id);
        print!("{:^12} | ", justify_text_with_ellipses(12, &self.story_frame.name));
        print!("{:^27} | ", justify_text_with_ellipses(27, &self.story_frame.description));
        println!("{:^10}", self.story_frame.status);
        println!();

        println!("[p] previous | [u] update story | [d] delete story");
    }

    fn parse_request(&self, request_option: &str, input_reader: R) -> Result<Action, UserError> {
        if request_option.to_lowercase() == "p" {
            Ok(Action::PreviousPage)
        } else if request_option.to_lowercase() == "u" {
            let mut input_reader = input_reader;
            let mut status_buf = String::new();

            // Validation loop for getting new status from the user
            let new_status = loop {
                self.print_status_update_menu();
                match input_reader.read_to_string(&mut status_buf) {
                    Ok(_n) => {
                        // Remove new line character from buffer
                        match status_buf.trim_end().parse::<u8>() {
                            Ok(s) if (0..4).contains(&s) => break s,
                            Ok(_s)  => {
                                println!("error parsing input as a valid status, please try again");
                                eprintln!("error parsing input as a valid status, please try again");
                            }
                            Err(e) => {
                                println!("error parsing input, please try again");
                                eprintln!("error parsing input, please try again");
                            }
                        }
                    }
                    Err(_) => {
                        println!("error reading input, please try again");
                        eprintln!("error reading input, please try again");
                    }
                }
                status_buf.clear();
            };

            let request_bytes = Event::get_update_story_status_tag(self.story_frame.epic_id, self.story_frame.id, new_status).to_vec();
            Ok(Action::RequestParsed(request_bytes))
        } else if request_option.to_lowercase() == "d" {
            let mut user_final_choice = String::new();
            let mut input_reader = input_reader;
            println!("are you sure you want to delete story {}? (y|n)", self.story_frame.id);
            let request_bytes = loop {
                match input_reader.read_to_string(&mut user_final_choice) {
                    Ok(n) if n > 0 => {
                        match user_final_choice.trim_end() {
                            "y" => break Event::get_delete_story_tag(self.story_frame.epic_id, self.story_frame.id).to_vec(),
                            "n" => return Ok(Action::Refresh),
                            _ => {
                                println!("please choose a valid option");
                                println!("are you sure you want to delete story {}? (y|n)", self.story_frame.id);
                            }
                        }
                    }
                    Ok(_n) => {
                        println!("no input entered, please try again");
                        eprintln!("no input entered, please try again");
                    }
                    Err(_e) => {
                        println!("error reading input, please try again");
                        eprintln!("error reading input, please try again");
                    }
                }
                user_final_choice.clear();
            };
            Ok(Action::RequestParsed(request_bytes))
        } else {
            Err(UserError::InvalidRequest)
        }
    }
}

/// Helper function to justify `text` in `width`. If `text` does not fit in `width`, a new string
/// will be created that contains as many characters that can fit within `width` followed by
/// an ellipses. When a String that contains a control character is passed in as a parameter (e.g `\n`), the control
/// character will be replaced with the unicode unknown symbol i.e U+fffd.
fn justify_text_with_ellipses(width: usize, text: &String) -> String {
    assert!(width > 3, "cannot center text in an interval with width less than 3");
    let text_width = unicode_width::UnicodeWidthStr::width(text.as_str());
    if text_width  > width {
        let text_chars = text.chars().collect::<Vec<char>>();
        let mut abridged_text = vec![];
        let mut abridged_text_width = 0;
        let mut i = 0;

        while i < text_chars.len() {
            // Ensure we have a valid character that is not a control character,
            // otherwise replace it with unknown.
            let  (char_width, char) = if let Some(char_width) = unicode_width::UnicodeWidthChar::width(text_chars[i]) {
                (char_width, text_chars[i])
            } else {
                (1, std::char::from_u32(0xfffd).unwrap())
            };

            if char_width + abridged_text_width + 3 <= width {
                abridged_text_width += char_width;
                abridged_text.push(text_chars[i]);
                i += 1;
            } else {
                break;
            }
        }

        let abridged_text = String::from_iter(abridged_text) + "...";
        format!("{:<width$}", abridged_text)
    } else {
        format!("{:<width$}", text)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::io::{Cursor, Read, BufRead};
    use async_std::task::block_on;
    use uuid::Uuid;

    #[test]
    fn test_justify_text_with_ellipses() {
        let width = 33;

        let text = String::from("Hello, World!");
        let justified_text = justify_text_with_ellipses(width, &text);

        println!("{:?}", justified_text);

        assert_eq!(justified_text, format!("{:<33}", "Hello, World!"));

        let text = String::from("abcdefghijklmnopqrstuvwxyzasdfjkl;");
        let justified_text = justify_text_with_ellipses(33, &text);

        println!("{:?}", justified_text);

        assert_eq!(justified_text, "abcdefghijklmnopqrstuvwxyzasdf...");

    }

    #[test]
    fn test_create_homepage() {
        let homepage_res = HomePage::try_create(vec![]);

        println!("{:?}", homepage_res);
        assert!(homepage_res.is_ok());

        let mut test_db = DbState::new("test_dir".to_string(), "test_db".to_string(), "test_epic_dir".to_string());
        let _ = test_db.add_epic("A good test epic".to_string(), "A good test epic for testing".to_string());
        let _ = test_db.add_epic("Another good test epic".to_string(), "A good test epic for testing".to_string());

        let db_bytes = test_db.as_bytes();

        let homepage_res = HomePage::try_create(db_bytes);

        println!("{:?}", homepage_res);
        assert!(homepage_res.is_ok());

        let homepage = homepage_res.unwrap();

        <HomePage as Page<Cursor<Vec<u8>>>>::print_page(&homepage);
    }

    #[test]
    fn test_print_page_homepage() {
        let mut home_page = HomePage { epic_frames: vec![] };
        <HomePage as Page<Cursor<Vec<u8>>>>::print_page(&home_page);

        home_page.epic_frames.push(
            EpicFrame {
                id: 0,
                name: String::from("Test Epic"),
                description: String::from("Another good epic for testing"),
                status: Status::Open,
            }
        );

        home_page.epic_frames.push(
            EpicFrame {
                id: 1,
                name: String::from("Test Epic"),
                description: String::from("Another good epic for testing"),
                status: Status::Resolved,
            }
        );

        home_page.epic_frames.push(
            EpicFrame {
                id: 3,
                name: String::from("Test Epic"),
                description: String::from("Another good epic for testing"),
                status: Status::InProgress,
            }
        );

        <HomePage as Page<Cursor<Vec<u8>>>>::print_page(&home_page);
    }

    #[test]
    fn test_print_page_epic_detail_page() {
        let mut epic_detail_page = EpicDetailPage {
            frame: EpicFrame {
                id: 0,
                name: "Test Frame".to_string(),
                description: "Test description".to_string(),
                status: Status::Open,
            },
            story_frames: vec![]
        };

        // epic_detail_page.print_page();
        <EpicDetailPage as Page<Cursor<Vec<u8>>>>::print_page(&epic_detail_page);

        epic_detail_page.story_frames.push(StoryFrame { id: 1, epic_id: 0, name: "Test Story".to_string(), description: "A simple test Story".to_string(), status: Status::Open });
        epic_detail_page.story_frames.push(StoryFrame { id: 33, epic_id: 0, name: "Another Test Story".to_string(), description: "Another simple test Story".to_string(), status: Status::InProgress });

        <EpicDetailPage as Page<Cursor<Vec<u8>>>>::print_page(&epic_detail_page);
    }

    #[test]
    fn test_try_create_epic_detail_page() {
        use std::collections::HashMap;
        use std::path::PathBuf;

        let mut test_epic = Epic::new(
            99,
            String::from("A Good Test Epic"),
            String::from("A good epic for testing"),
            Status::Open,
            PathBuf::from("test_epic_file_path"),
            HashMap::new()
        );

        let test_story1 = Story::new(107, 99, String::from("Test Story"), String::from("Another good test story"), Status::Open);
        let test_story2 = Story::new(108, 99, String::from("Test Story"), String::from("Another good test story"), Status::Closed);

        assert!(test_epic.add_story(test_story1).is_ok());
        assert!(test_epic.add_story(test_story2).is_ok());

        let epic_detail_page_res = EpicDetailPage::try_create(test_epic.as_bytes());

        println!("{:?}", epic_detail_page_res);
        assert!(epic_detail_page_res.is_ok());

        let epic_detail_page = epic_detail_page_res.unwrap();

        <EpicDetailPage as Page<Cursor<Vec<u8>>>>::print_page(&epic_detail_page);
    }

    #[test]
    fn test_try_create_story_detail_page() {
        let test_story = Story::new(0, 1,String::from("A test story"), String::from("A good story for testing purposes"), Status::Open);
        let story_bytes = test_story.as_bytes();

        let test_story_detail_page_res = StoryDetailPage::try_create(story_bytes);

        println!("{:?}", test_story_detail_page_res);
        assert!(test_story_detail_page_res.is_ok());

        let test_story_detail_page = test_story_detail_page_res.unwrap();

        println!("{:?}", test_story_detail_page);
        assert_eq!(test_story_detail_page.story_frame.id, 0);
        assert_eq!(test_story_detail_page.story_frame.name, String::from("A test story"));
        assert_eq!(test_story_detail_page.story_frame.description, String::from("A good story for testing purposes"));
        assert_eq!(test_story_detail_page.story_frame.status, Status::Open);
    }

    #[test]
    fn test_print_page_story_detail_page() {
        let test_story = Story::new(0, 1,String::from("A test story"), String::from("A good story for testing purposes"), Status::Open);
        let story_bytes = test_story.as_bytes();

        let test_story_detail_page_res = StoryDetailPage::try_create(story_bytes);

        println!("{:?}", test_story_detail_page_res);
        assert!(test_story_detail_page_res.is_ok());

        let test_story_detail_page = test_story_detail_page_res.unwrap();

        <StoryDetailPage as Page<Cursor<Vec<u8>>>>::print_page(&test_story_detail_page);
    }

    #[test]
    fn test_home_page_parse_request() {
        use async_std::io::{ReadExt, BufRead};
        let mut homepage = HomePage {epic_frames: vec![] };
        homepage.epic_frames.push(
            EpicFrame {
                id: 0,
                name: String::from("Test Epic"),
                description: String::from("Another good epic for testing"),
                status: Status::Open,
            }
        );

        homepage.epic_frames.push(
            EpicFrame {
                id: 1,
                name: String::from("Test Epic"),
                description: String::from("Another good epic for testing"),
                status: Status::Resolved,
            }
        );

        homepage.epic_frames.push(
            EpicFrame {
                id: 3,
                name: String::from("Test Epic"),
                description: String::from("Another good epic for testing"),
                status: Status::InProgress,
            }
        );
        // Test quit option, should return Ok(Action::Quit)
        println!("Testing quit...");
        let user_input = "q";
        let mut request_input = Cursor::new(vec![1u8, 2, 3, 4, 5]);
        // let request = block_on(HomePage::parse_request(user_input, &mut input_reader));
        let request = homepage.parse_request(user_input, request_input);
        println!("{:?}", request);
        assert!(request.is_ok());

        let request = request.unwrap();
        println!("{:?}", request);
        assert_eq!(request, Action::Quit);

        let user_input = "Q";
        let mut request_input = Cursor::new(vec![1u8, 2, 3, 4, 5]);
        let request = homepage.parse_request(user_input, request_input);
        println!("{:?}", request);
        assert!(request.is_ok());

        let request = request.unwrap();
        println!("{:?}", request);
        assert_eq!(request, Action::Quit);

        // Test create option, should return Ok(Action::RequestParsed(Vec<u8>)),
        println!("Testing epic creation...");
        let user_input = "c";
        let mut input_reader = Cursor::new(String::from("Test Epic\nA good epic for testing purposes\n").into_bytes());
        let request = homepage.parse_request(user_input, &mut input_reader);

        println!("{:?}", request);
        assert!(request.is_ok());

        let request = request.unwrap();
        println!("{:?}", request);
        assert!(request.is_request_parsed());

        let Action::RequestParsed(request_bytes) = request else { panic!("request should be parsed") };

        // Attempt to parse the encoded request
        let mut request_cursor = async_std::io::Cursor::new(request_bytes);
        let mut tag_buf = [0u8; 13];
        let res = block_on(request_cursor.read_exact(&mut tag_buf));
        println!("{:?}", res);
        assert!(res.is_ok());

        // let Ok(Event::AddEpic {peer_id, epic_name, epic_description}) =
        //     block_on(Event::try_create_add_epic(Uuid::new_v4(), &tag_buf, request_cursor))
        let Ok(Event::AddEpic {peer_id, epic_name, epic_description}) =
            block_on(Event::try_create_add_epic(Uuid::new_v4(), &tag_buf, request_cursor))
        else { panic!("event was not created correctly") };

        assert_eq!(epic_name.as_str(), "Test Epic");
        assert_eq!(epic_description.as_str(), "A good epic for testing purposes");

        let user_input = "C";
        let mut input_reader = Cursor::new(String::from("Test Epic\nA good epic for testing purposes").into_bytes());
        let request = homepage.parse_request(user_input, &mut input_reader);

        println!("{:?}", request);
        assert!(request.is_ok());

        let request = request.unwrap();
        println!("{:?}", request);
        assert!(request.is_request_parsed());

        // Attempt to parse the encoded request
        let Action::RequestParsed(request_bytes) = request else { panic!("request should be parsed") };
        let mut request_cursor = async_std::io::Cursor::new(request_bytes);
        let mut tag_buf = [0u8; 13];
        let res = block_on(request_cursor.read_exact(&mut tag_buf));
        println!("{:?}", res);
        assert!(res.is_ok());

        let Ok(Event::AddEpic {peer_id, epic_name, epic_description}) =
            block_on(Event::try_create_add_epic(Uuid::new_v4(), &tag_buf, request_cursor))
            else { panic!("event was not created correctly") };

        assert_eq!(epic_name.as_str(), "Test Epic");
        assert_eq!(epic_description.as_str(), "A good epic for testing purposes");

        // Test getting epic request
        println!("Testing get epic option...");
        let user_input = "97";
        let mut request_input = Cursor::new(vec![0u8]);
        let request = homepage.parse_request(user_input, &mut request_input);

        println!("{:?}", request);
        assert!(request.is_ok());

        let request = request.unwrap();
        println!("{:?}", request);
        assert!(request.is_request_parsed());

        let Action::RequestParsed(request_bytes) = request else { panic!("request should be parsed") };
        let mut request_cursor = async_std::io::Cursor::new(request_bytes);
        let mut tag_buf = [0u8; 13];
        let res = block_on(request_cursor.read_exact(&mut tag_buf));

        println!("{:?}", res);
        assert!(res.is_ok());

        let Ok(Event::GetEpic {peer_id, epic_id}) = block_on(Event::try_create_get_epic(Uuid::new_v4(), &tag_buf))
                            else { panic!("event was not created correctly") };

        assert_eq!(epic_id, 97);

        // Testing invalid input option
        let user_input = "djowalk";
        let mut request_input = Cursor::new(Vec::new());
        let request = homepage.parse_request(user_input, &mut request_input);
        println!("{:?}", request);
        assert!(request.is_err());
    }

    #[test]
    fn test_epic_detail_page_parse_request() {
        use std::path::PathBuf;
        use std::collections::HashMap;
        use async_std::io::{ReadExt, BufRead};

        // Create epic for creating the epic detail page
        let epic = Epic::new(
            0,
            String::from("Test Epic"),
            String::from("A goo epic for testing purposes"),
            Status::Open,
            PathBuf::new(),
            HashMap::new()
        );

        // Represents a response from the server
        // let mut response_cursor = Cursor::new(epic.as_bytes());

        // attempt to parse the epic detail page from the `response`
        let epic_detail_page_res = EpicDetailPage::try_create(epic.as_bytes());
        println!("{:?}", epic_detail_page_res);
        assert!(epic_detail_page_res.is_ok());

        let epic_detail_page = epic_detail_page_res.unwrap();

        println!("testing previous option...");
        // Some empty request data
        let user_input = "p";
        let mut request_input = Cursor::new(vec![]);
        let request = epic_detail_page.parse_request(user_input, &mut request_input);
        println!("{:?}", request);
        assert!(request.is_ok());
        assert!(request.unwrap().is_previous_page());

        println!("testing update option...");
        let user_input = "u";
        let mut request_input = Cursor::new(String::from("3\n").into_bytes());
        let request = epic_detail_page.parse_request(user_input, &mut request_input);

        println!("{:?}", request);
        assert!(request.is_ok());

        let request = request.unwrap();
        println!("{:?}", request);
        let Action::RequestParsed(request_bytes) = request else { panic!("request should be parsed") };

        // Simulate reading the request from the user
        let mut request_cursor = async_std::io::Cursor::new(request_bytes);
        // read the tag from the request
        let mut tag_buf = [0; 13];
        assert!(block_on(request_cursor.read_exact(&mut tag_buf)).is_ok());
        // Attempt to parse an event from the request
        let Ok(Event::UpdateEpicStatus {peer_id, epic_id, status}) =
            block_on(Event::try_create_update_epic_status(Uuid::new_v4(), &tag_buf))
            else { panic!("unable to parse request correctly") };

        assert_eq!(epic_id, 0);
        assert_eq!(status, Status::Closed);

        println!("testing delete epic option...");
        let user_input = "d";
        let mut request_input = Cursor::new(String::from("y\n").into_bytes());
        let request = epic_detail_page.parse_request(user_input, &mut request_input);

        println!("{:?}", user_input);
        assert!(request.is_ok());

        let request = request.unwrap();
        println!("{:?}", request);
        assert!(request.is_request_parsed());

        let Action::RequestParsed(request_bytes) = request else { panic!("request should be parsed") };
        let mut request_cursor = async_std::io::Cursor::new(request_bytes);
        let mut tag_buf = [0; 13];
        assert!(block_on(request_cursor.read_exact(&mut tag_buf)).is_ok());
        let Ok(Event::DeleteEpic {peer_id, epic_id}) = block_on(Event::try_create_delete_epic(Uuid::new_v4(), &tag_buf)) else { panic!("unable to parse request correctly") };

        assert_eq!(epic_id, 0);

        println!("testing delete epic option with cancel...");
        let user_input = "d";
        let mut request_input = Cursor::new(String::from("n\n").into_bytes());
        let request = epic_detail_page.parse_request(user_input, &mut request_input);

        println!("{:?}", request);
        assert!(request.is_ok());

        let request = request.unwrap();
        println!("{:?}", request);
        assert!(request.is_refresh());

        println!("testing add story option...");
        let user_input = "c";
        let mut request_input = Cursor::new(String::from("Test Story\nA good story for testing purposes\n").into_bytes());
        let request = epic_detail_page.parse_request(user_input, &mut request_input);

        println!("{:?}", request);
        assert!(request.is_ok());

        let request = request.unwrap();
        println!("{:?}", request);
        assert!(request.is_request_parsed());

        let Action::RequestParsed(request_bytes) = request else { panic!("request should be parsed") };
        let mut request_cursor = async_std::io::Cursor::new(request_bytes);
        let mut tag_buf=  [0; 13];
        assert!(block_on(request_cursor.read_exact(&mut tag_buf)).is_ok());
        let Ok(Event::AddStory {peer_id, epic_id, story_name, story_description}) = block_on(Event::try_create_add_story(Uuid::new_v4(), &tag_buf, &mut request_cursor)) else { panic!("unable to parse request correctly") };

        assert_eq!(epic_id, 0);
        assert_eq!(story_name, String::from("Test Story"));
        assert_eq!(story_description, String::from("A good story for testing purposes"));

        println!("testing get story detail option...");
        let user_input = "97";
        let mut request_input = Cursor::new(vec![]);
        let request = epic_detail_page.parse_request(user_input, &mut request_input);

        println!("{:?}", request);
        assert!(request.is_ok());

        let request = request.unwrap();
        println!("{:?}", request);
        assert!(request.is_request_parsed());

        let Action::RequestParsed(request_bytes) = request else { panic!("request should be parsed") };
        let mut request_cursor = async_std::io::Cursor::new(request_bytes);
        let mut tag_buf = [0; 13];
        assert!(block_on(request_cursor.read_exact(&mut tag_buf)).is_ok());
        let Ok(Event::GetStory {peer_id, epic_id, story_id}) =
            block_on(Event::try_create_get_story(Uuid::new_v4(), &tag_buf))
            else { panic!("unable to parse event correctly") };

        assert_eq!(epic_id, 0);
        assert_eq!(story_id, 97);

        println!("testing bogus request option...");
        let user_option = "du98fd9e";
        let mut user_input = Cursor::new(String::from("basdfoiub\n"));
        let request = epic_detail_page.parse_request(user_option, &mut user_input);
        println!("{:?}", request);
        assert!(request.is_err());
    }

    #[test]
    fn test_story_detail_page_parse_request() {
        use async_std::io::ReadExt;
        let test_story = Story::new(97, 1, String::from("Test story"), String::from("A good story for testing purposes"), Status::InProgress);
        let story_detail_page_res = StoryDetailPage::try_create(test_story.as_bytes());

        println!("{:?}", story_detail_page_res);
        assert!(story_detail_page_res.is_ok());

        let story_detail_page = story_detail_page_res.unwrap();

        println!("testing previous option...");
        let user_input = "p";
        let mut request_input = Cursor::new(vec![]);
        let request = story_detail_page.parse_request(user_input, &mut request_input);

        println!("{:?}", request);
        assert!(request.is_ok());

        let request = request.unwrap();
        println!("{:?}", request);
        assert!(request.is_previous_page());

        println!("testing update option...");
        let user_input = "u";
        let mut request_input = Cursor::new(String::from("2\n").into_bytes());
        let request = story_detail_page.parse_request(user_input, &mut request_input);

        println!("{:?}", request);
        assert!(request.is_ok());

        let request = request.unwrap();
        println!("{:?}", request);
        assert!(request.is_request_parsed());

        let Action::RequestParsed(request_bytes) = request else { panic!("request should be parsed") };
        let mut request_data = async_std::io::Cursor::new(request_bytes);
        let mut tag_buf = [0; 13];
        assert!(block_on(request_data.read_exact(&mut tag_buf)).is_ok());

        let Ok(Event::UpdateStoryStatus { peer_id, epic_id,story_id, status}) =
            block_on(Event::try_create_update_story_status(Uuid::new_v4(), &tag_buf))
            else { panic!("unable to parse event correctly") };

        assert_eq!(epic_id, 1);
        assert_eq!(story_id, 97);
        assert_eq!(status, Status::try_from(2).expect("status should be created"));

        println!("testing delete option...");
        let user_input = "d";
        let mut request_input = Cursor::new(String::from("y\n").into_bytes());
        let request = story_detail_page.parse_request(user_input, &mut request_input);

        println!("{:?}", request);
        assert!(request.is_ok());

        let request = request.unwrap();
        println!("{:?}", request);
        assert!(request.is_request_parsed());

        let Action::RequestParsed(request_bytes) = request else { panic!("request should be parsed") };
        let mut request_data = async_std::io::Cursor::new(request_bytes);
        let mut tag_buf = [0; 13];
        assert!(block_on(request_data.read_exact(&mut tag_buf)).is_ok());

        let Ok(Event::DeleteStory { peer_id, epic_id, story_id}) =
            block_on(Event::try_create_delete_story(Uuid::new_v4(), &tag_buf))
            else { panic!("unable to parse event correctly") };

        assert_eq!(epic_id, 1);
        assert_eq!(story_id, 97);

        let user_input = "d";
        let mut request_input = Cursor::new(String::from("n\n").into_bytes());
        let request = story_detail_page.parse_request(user_input, &mut request_input);

        println!("{:?}", request);
        assert!(request.is_ok());

        let request = request.unwrap();
        println!("{:?}", request);
        assert!(request.is_refresh());

        println!("testing garbage input...");
        let user_input = "asdojie038";
        let mut request_input = Cursor::new(vec![2, 34, 5, 1]);

        let request = story_detail_page.parse_request(user_input, &mut request_input);

        println!("{:?}", request);
        assert!(request.is_err());
    }
}





