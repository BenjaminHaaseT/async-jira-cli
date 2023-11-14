//! Provides the necessary components for creating a smoothing CLI for clients interacting
//! with the server.
//!

use std::io::BufRead;
use std::marker::Unpin;
use async_std::io::{Write, WriteExt, ReadExt, Read};
use async_jira_cli::response::prelude::*;
use async_jira_cli::utils::prelude::*;
use crate::UserError;

mod frame;
mod pages;

use pages::prelude::*;

// use frame::prelude::*;
// use pages::prelude::*;

pub mod prelude {
    pub use super::*;
}

/// Implements the functionality for creating a CLI that a client can use to interact with
/// the server.
///
/// The struct can take correctly parsed `Response` tags read from a `TcpStream` and display
/// the appropriate prompt to the user on stdout via `parse_response` method. Internally,
/// an `Interface` will keep a stack of pages that can be navigated through by the client, and
/// will display appropriate messages when errors arise.
pub struct Interface<R, T>
where
    R: BufRead,
    T: WriteExt + ReadExt + Unpin,
{
    page_stack: Vec<Box<dyn Page<R>>>,
    connection_stream: T,
    client_input: R,
}

// TODO: 1) move response conditional blocks into separate functions
// TODO: 2) Refactor design such that the interface has ownership connection to the stream
// TODO: 3) implement a parse_option method for taking a user selected option input from stdin
//          and parsing it correctly i.e. implementing the logic needed for the users given selection
impl<R, T> Interface<R, T>
where
    R: BufRead,
    T: WriteExt + ReadExt + Unpin,
{
    /// Creates a new `Interface`
    pub fn new(connection_stream: T, client_input: R) -> Self {
        Interface {
            page_stack: Vec::new() ,
            connection_stream,
            client_input,
        }
    }

    pub async fn run(&mut self) -> Result<(), UserError> {
        let mut tag_buf = [0u8; 18];

        'outer: loop {
            self.connection_stream.read_exact(&mut tag_buf)
                .await
                .map_err(|_| UserError::ServerConnection(String::from("unable to read correctly formatted response from server")))?;

            self.parse_response(&tag_buf).await?;

            'inner: loop {
                let mut user_option = String::new();
                self.client_input.read_to_string(&mut user_option)
                    .map_err(|_| UserError::ParseRequestOption)?;

                let action_result = self.parse_request_option(user_option.as_str()).await;

                match action_result {
                    Ok(action) => {
                        match action {
                            Action::Quit =>  {
                                println!("program terminated");
                                break 'outer;
                            }
                            Action::PreviousPage => {
                                // Ensure that we have a page to go back to, if client has selected
                                // previous page there should always be a page to go back to
                                assert!(self.page_stack.len() > 1, "cannot navigate to previous page");
                                self.page_stack.pop();
                                self.page_stack[self.page_stack.len() - 1].print_page();
                                // continue with inner loop
                                continue 'inner;
                            }
                            Action::RequestParsed(req) => {
                                // We need to send the request to the server
                                self.connection_stream.write_all(req.as_slice())
                                    .await
                                    .map_err(|_| UserError::ServerConnection(String::from("unable to write request to server")))?;
                                break 'inner;
                            }
                            Action::Refresh => {
                                self.page_stack[self.page_stack.len() - 1].print_page();
                                // continue with inner loop
                                continue 'inner;
                            }
                        }
                    }
                    Err(e) => {
                        println!("{e}");
                        // TODO: log error as well
                        eprintln!("{e}");
                        println!();
                        // We always want to give the client another attempt when ever an error occurs
                        // from parsing a request
                        self.page_stack[self.page_stack.len() - 1].print_page();
                        // continue with inner loop
                        continue 'inner;
                    }
                }
            }
        }

        Ok(())
    }

    /// Attempt to read a response from the stream. If response read correctly, then parse frames
    /// from the read data and create a new page and ad page to page_stack, or display a message and a current page depending
    /// on the logic.
    pub async fn parse_response(&mut self, tag_buf: &[u8; 18]) -> Result<(), UserError> {
        let (type_and_flag, epic_id, story_id, data_len) = Response::decode(tag_buf);

        // Check the type of response and then parse accordingly
        if type_and_flag & 1 != 0 {
            self.parse_successful_connection_response(data_len).await?;

        } else if type_and_flag & 2 != 0 {
            // This is a suspicious case, it should not happen so return an internal server error to the client
            // TODO: Add logging for this event as well
            return Err(UserError::InternalServerError);
        } else if type_and_flag & 4 != 0 {
            self.parse_add_epic_response(epic_id, data_len).await?;

        } else if type_and_flag & 8 != 0 {
            self.parse_delete_epic_response(epic_id, data_len).await?;

        } else if type_and_flag & 16 != 0 {
            self.parse_get_epic_response(epic_id, data_len).await?;

        } else if type_and_flag & 32 != 0 {
            self.parse_update_epic_status_response(epic_id, data_len).await?;

        } else if type_and_flag & 64 != 0 {
            self.parse_epic_does_not_exist_response(epic_id, data_len).await?;

        } else if type_and_flag & 128 != 0 {
            self.parse_get_story_response(story_id, data_len).await?;

        } else if type_and_flag & 256 != 0 {
            self.parse_story_does_not_exist_response(story_id, data_len).await?;

        } else if type_and_flag & 512 != 0 {
            self.parse_add_story_response(story_id, data_len).await?;

        } else if type_and_flag & 1024 != 0 {
            self.parse_delete_story_response(story_id, data_len).await?;

        } else if type_and_flag & 2048 != 0 {
            self.parse_update_story_status_response(story_id, data_len).await?;

        } else if type_and_flag & 4096 != 0 {
            println!("request not parsed, please try again");
            println!();

            assert!(!self.page_stack.is_empty());

            self.page_stack[self.page_stack.len() - 1].print_page();
        } else {
            return Err(UserError::ParseResponseError(String::from("unable to parse response from server correctly")));
        }

        Ok(())
    }

    async fn parse_request_option(&mut self, option: &str) -> Result<Action, UserError> {
        assert!(!self.page_stack.is_empty(), "pages should have been loaded prior to calling `self.parse_request_option()`");
        let top_pos = self.page_stack.len() - 1;
        let mut top_page = &mut self.page_stack[top_pos];
        top_page.parse_request(option, &mut self.client_input)
    }

    async fn parse_successful_connection_response(&mut self, data_len: u64) -> Result<(), UserError> {
        let mut data = vec![0; data_len as usize];
        self.connection_stream.read_exact(data.as_mut_slice())
            .await
            .map_err(|_|
                UserError::ParseResponseError(format!("unable to parse response from server correctly"))
            )?;

        println!("connected to database successfully");
        println!();

        // Attempt to create homepage
        let homepage = Box::new(HomePage::try_create(data)?);

        // Display homepage if successful
        <HomePage as Page<R>>::print_page(&homepage);
        println!();

        // Save homepage to `self.page_stack`
        self.page_stack.push(homepage);
        Ok(())
    }

    async fn parse_add_epic_response(&mut self, epic_id: u32, data_len: u64) -> Result<(), UserError> {
        // Read updated homepage data
        let mut data = vec![0; data_len as usize];
        self.connection_stream.read_exact(data.as_mut_slice())
            .await
            .map_err(|_|
                UserError::ParseResponseError(format!("unable to parse response from server correctly"))
            )?;

        // Attempt to create the new homepage
        let homepage = Box::new(HomePage::try_create(data)?);

        // display success message to user
        println!("epic added successfully, epic id: {}", epic_id);
        println!();

        // Display homepage if successful
        <HomePage as Page<R>>::print_page(&homepage);
        println!();

        self.page_stack = vec![homepage];
        Ok(())
    }

    async fn parse_delete_epic_response(&mut self, epic_id: u32, data_len: u64) -> Result<(), UserError> {
        let mut data = vec![0; data_len as usize];
        self.connection_stream.read_exact(data.as_mut_slice())
            .await
            .map_err(|_|
                UserError::ParseResponseError(format!("unable to parse response from server correctly"))
            )?;
        // Attempt to create homepage
        let homepage = Box::new(HomePage::try_create(data)?);

        println!("epic with id: {} was deleted successfully", epic_id);
        println!();

        // Display homepage if successful
        <HomePage as Page<R>>::print_page(&homepage);
        println!();

        self.page_stack = vec![homepage];
        Ok(())
    }

    async fn parse_get_epic_response(&mut self, epic_id: u32, data_len: u64) -> Result<(), UserError> {
        let mut data = vec![0; data_len as usize];
        self.connection_stream.read_exact(data.as_mut_slice())
            .await
            .map_err(|_| UserError::ParseResponseError(String::from("unable to parse response from server correctly")))?;
        let epic_detail_page = Box::new(EpicDetailPage::try_create(data)?);

        println!("get epic with id: {} successful", epic_id);
        println!();

        <EpicDetailPage as Page<R>>::print_page(&epic_detail_page);

        self.page_stack.push(epic_detail_page);
        Ok(())
    }

    async fn parse_update_epic_status_response(&mut self, epic_id: u32, data_len: u64) -> Result<(), UserError> {
        let mut data = vec![0; data_len as usize];
        self.connection_stream.read_exact(data.as_mut_slice())
            .await
            .map_err(|_| UserError::ParseResponseError(String::from("unable to parse response from server correctly")))?;

        let epic_detail_page = Box::new(EpicDetailPage::try_create(data)?);

        println!("updated status of epic with id: {} successful", epic_id);
        println!();

        <EpicDetailPage as Page<R>>::print_page(&epic_detail_page);

        assert_eq!(self.page_stack.len(), 2);
        self.page_stack.pop();
        self.page_stack.push(epic_detail_page);
        Ok(())
    }

    async fn parse_epic_does_not_exist_response(&mut self, epic_id: u32, data_len: u64) -> Result<(), UserError> {
        let mut data = vec![0; data_len as usize];
        self.connection_stream.read_exact(data.as_mut_slice())
            .await
            .map_err(|_| UserError::ParseResponseError(String::from("unable to parse response from server correctly")))?;

        let homepage = Box::new(HomePage::try_create(data)?);

        println!("epic with id: {} does not exist", epic_id);
        println!();

        <HomePage as Page<R>>::print_page(&homepage);
        assert_eq!(self.page_stack.len(), 1);
        self.page_stack = vec![homepage];
        Ok(())
    }

    async fn parse_get_story_response(&mut self, story_id: u32, data_len: u64) -> Result<(), UserError> {
        let mut data = vec![0; data_len as usize];
        self.connection_stream.read_exact(data.as_mut_slice())
            .await
            .map_err(|_| UserError::ParseResponseError(String::from("unable to parse response from server correctly")))?;

        let story_detail_page = Box::new(StoryDetailPage::try_create(data)?);

        println!("get story with id: {} successful", story_id);
        println!();

        <StoryDetailPage as Page<R>>::print_page(&story_detail_page);
        self.page_stack.push(story_detail_page);
        Ok(())
    }

    async fn parse_story_does_not_exist_response(&mut self, story_id: u32, data_len: u64) -> Result<(), UserError> {
        let mut data = vec![0; data_len as usize];
        self.connection_stream.read_exact(data.as_mut_slice())
            .await
            .map_err(|_| UserError::ParseResponseError(String::from("unable to parse response from server correctly")))?;

        let epic_detail_page = Box::new(EpicDetailPage::try_create(data)?);

        println!("story with id: {} does not exist", story_id);
        println!();

        <EpicDetailPage as Page<R>>::print_page(&epic_detail_page);
        assert!(!self.page_stack.is_empty());

        while self.page_stack.len() > 1 {
            self.page_stack.pop();
        }

        self.page_stack.push(epic_detail_page);
        Ok(())
    }

    async fn parse_add_story_response(&mut self, story_id: u32, data_len: u64) -> Result<(), UserError> {
        let mut data = vec![0; data_len as usize];
        self.connection_stream.read_exact(data.as_mut_slice())
            .await
            .map_err(|_| UserError::ParseResponseError(String::from("unable to parse response from server correctly")))?;

        let epic_detail_page = Box::new(EpicDetailPage::try_create(data)?);

        println!("story with id: {} was added successfully", story_id);
        println!();

        <EpicDetailPage as Page<R>>::print_page(&epic_detail_page);
        assert!(!self.page_stack.is_empty());

        while self.page_stack.len() > 1 {
            self.page_stack.pop();
        }

        self.page_stack.push(epic_detail_page);
        Ok(())
    }

    async fn parse_delete_story_response(&mut self, story_id: u32, data_len: u64) -> Result<(), UserError> {
        let mut data = vec![0; data_len as usize];
        self.connection_stream.read_exact(data.as_mut_slice())
            .await
            .map_err(|_| UserError::ParseResponseError(String::from("unable to parse response from server correctly")))?;

        let epic_detail_page = Box::new(EpicDetailPage::try_create(data)?);

        println!("story with id: {} was deleted successfully", story_id);
        println!();

        <EpicDetailPage as Page<R>>::print_page(&epic_detail_page);
        assert!(!self.page_stack.is_empty());
        while self.page_stack.len() > 1 {
            self.page_stack.pop();
        }

        self.page_stack.push(epic_detail_page);
        Ok(())
    }

    async fn parse_update_story_status_response(&mut self, story_id: u32, data_len: u64) -> Result<(), UserError> {
        let mut data = vec![0; data_len as usize];
        self.connection_stream.read_exact(data.as_mut_slice())
            .await
            .map_err(|_| UserError::ParseResponseError(String::from("unable to parse response from server correctly")))?;

        let story_detail_page = Box::new(StoryDetailPage::try_create(data)?);

        println!("updated status of story with id: {} was successful", story_id);
        println!();

        <StoryDetailPage as Page<R>>::print_page(&story_detail_page);
        assert!(!self.page_stack.is_empty());
        while self.page_stack.len() > 2 {
            self.page_stack.pop();
        }

        self.page_stack.push(story_detail_page);
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use async_std::io::{Cursor, Seek, SeekFrom};
    use async_std::task::block_on;
    use std::io::{BufRead, BufReader, stdin};
    use futures::AsyncSeekExt;
    use async_jira_cli::models::prelude::*;
    use frame::prelude::*;
    #[test]
    fn test_interface_new() {
        // To simulate reading and writing with the interface
        let connection_stream: Cursor<Vec<u8>> = Cursor::new(vec![]);
        let client_input = BufReader::new(stdin());
        let interface = Interface::new(connection_stream, client_input);
        println!("Interface created successfully");
    }

    #[test]
    fn test_interface_parse_successful_connection_response() {
        // Set up the mock database
        let mut db_handle = DbState::new(
            String::default(),
            String::default(),
            String::default(),
        );

        db_handle.add_epic(String::from("Test Epic 1"), String::from("A good first epic for testing purposes"));
        db_handle.add_epic(String::from("Test Epic 2"), String::from("A good second epic for testing purposes"));
        db_handle.add_epic(String::from("Test Epic 3"), String::from("A good third epic for testing purposes"));
        let db_bytes = db_handle.as_bytes();

        // Set up mock connection stream, and client input
        let client_input = BufReader::new(stdin());
        let mut connection_stream: Cursor<Vec<u8>> = Cursor::new(vec![]);

        // Write the mock response to the connection stream
        let connection_response = Response::ClientAddedOk(db_bytes.clone());

        assert!(block_on(connection_stream.write_all(connection_response.as_bytes().as_slice())).is_ok());

        // Tag buffer for reading response
        let mut tag_buf = [0u8; 18];

        assert!(block_on(connection_stream.seek(SeekFrom::Start(0))).is_ok());

        // simulate reading the response from the server
        assert!(block_on(connection_stream.read_exact(&mut tag_buf)).is_ok());

        // simulate deserializing the response
        let (type_and_flag, epic_id, story_id, data_len) = Response::decode(&tag_buf);

        // Build the interface
        let mut interface = Interface::new(connection_stream, client_input);

        // Ensure response is handled correctly
        assert!(block_on(interface.parse_successful_connection_response(data_len)).is_ok());
    }

    #[test]
    fn test_interface_parse_add_epic_response() {
        // Set up the mock database
        let mut db_handle = DbState::new(
            String::default(),
            String::default(),
            String::default(),
        );

        db_handle.add_epic(String::from("Test Epic 1"), String::from("A good first epic for testing purposes"));
        db_handle.add_epic(String::from("Test Epic 2"), String::from("A good second epic for testing purposes"));
        db_handle.add_epic(String::from("Test Epic 3"), String::from("A good third epic for testing purposes"));
        db_handle.add_epic(String::from("Test Epic 4"), String::from("A good fourth epic for testing purposes"));
        let last_added_id = db_handle.last_unique_id();
        assert!(db_handle.contains_epic(last_added_id));

        let db_bytes = db_handle.as_bytes();

        // Set up mock connection stream, and client input
        let client_input = BufReader::new(stdin());
        let mut connection_stream: Cursor<Vec<u8>> = Cursor::new(vec![]);

        // Write the mock response to the connection stream
        let add_epic_response = Response::AddedEpicOk(last_added_id, db_bytes);

        assert!(block_on(connection_stream.write_all(add_epic_response.as_bytes().as_slice())).is_ok());

        // Tag buffer for reading response
        let mut tag_buf = [0u8; 18];

        assert!(block_on(connection_stream.seek(SeekFrom::Start(0))).is_ok());

        // simulate reading the response from the server
        assert!(block_on(connection_stream.read_exact(&mut tag_buf)).is_ok());

        // simulate deserializing the response
        let (type_and_flag, epic_id, story_id, data_len) = Response::decode(&tag_buf);

        // Build the interface
        let mut interface = Interface::new(connection_stream, client_input);

        // Ensure response is handled correctly
        assert!(block_on(interface.parse_add_epic_response(epic_id, data_len)).is_ok());
        assert_eq!(epic_id, last_added_id);
    }

    #[test]
    fn test_interface_parse_delete_epic_response() {
        // Set up the mock database
        let mut db_handle = DbState::new(
            String::default(),
            String::default(),
            String::default(),
        );

        db_handle.add_epic(String::from("Test Epic 1"), String::from("A good first epic for testing purposes"));
        db_handle.add_epic(String::from("Test Epic 2"), String::from("A good second epic for testing purposes"));
        db_handle.add_epic(String::from("Test Epic 3"), String::from("A good third epic for testing purposes"));
        db_handle.add_epic(String::from("Test Epic 4"), String::from("A good fourth epic for testing purposes"));
        let last_added_id = db_handle.last_unique_id();
        assert!(db_handle.contains_epic(last_added_id));

        // delete last added epic
        assert!(db_handle.delete_epic(last_added_id).is_ok());

        let db_bytes = db_handle.as_bytes();

        // Set up mock connection stream, and client input
        let client_input = BufReader::new(stdin());
        let mut connection_stream: Cursor<Vec<u8>> = Cursor::new(vec![]);

        // Write the mock response to the connection stream
        let add_epic_response = Response::DeletedEpicOk(last_added_id, db_bytes);

        assert!(block_on(connection_stream.write_all(add_epic_response.as_bytes().as_slice())).is_ok());

        // Tag buffer for reading response
        let mut tag_buf = [0u8; 18];

        assert!(block_on(connection_stream.seek(SeekFrom::Start(0))).is_ok());

        // simulate reading the response from the server
        assert!(block_on(connection_stream.read_exact(&mut tag_buf)).is_ok());

        // simulate deserializing the response
        let (type_and_flag, epic_id, story_id, data_len) = Response::decode(&tag_buf);

        // Build the interface
        let mut interface = Interface::new(connection_stream, client_input);

        // Ensure response is handled correctly
        assert!(block_on(interface.parse_delete_epic_response(epic_id, data_len)).is_ok());
        assert_eq!(epic_id, last_added_id);
    }

    #[test]
    fn test_interface_get_epic_response() {
        use std::path::PathBuf;
        use std::collections::HashMap;
        // Create the mock epic with stories
        let mut epic = Epic::new(
            64,
            String::from("Test Epic"),
            String::from("A good epic for testing purposes"),
            Status::Open,
            PathBuf::default(),
            HashMap::new()
        );
        let test_story1 = Story::new(
            97,
            64,
            String::from("Test Story 1"),
            String::from("A good first story for testing purposes"),
            Status::Open
        );
        let test_story2 = Story::new(
            98,
            64,
            String::from("Test Story 2"),
            String::from("A good second story for testing purposes"),
            Status::Open
        );
        let test_story3 = Story::new(
            99,
            64,
            String::from("Test Story 3"),
            String::from("A good third story for testing purposes"),
            Status::Open
        );

        assert!(epic.add_story(test_story1).is_ok());
        assert!(epic.add_story(test_story2).is_ok());
        assert!(epic.add_story(test_story3).is_ok());

        // Create the mock response
        let get_epic_response = Response::GetEpicOk(64, epic.as_bytes());

        // Set up mock connection stream, and client input
        let client_input = BufReader::new(stdin());
        let mut connection_stream: Cursor<Vec<u8>> = Cursor::new(vec![]);

        assert!(block_on(connection_stream.write_all(get_epic_response.as_bytes().as_slice())).is_ok());

        let mut tag_buf = [0u8; 18];

        // Move cursor position back to start
        assert!(block_on(connection_stream.seek(SeekFrom::Start(0))).is_ok());

        // Simulate reading response tag from server
        assert!(block_on(connection_stream.read_exact(&mut tag_buf)).is_ok());

        // simulate deserializing the response
        let (type_and_flag, epic_id, story_id, data_len) = Response::decode(&tag_buf);

        // Build the interface
        let mut interface = Interface::new(connection_stream, client_input);

        // Ensure response is handled correctly
        assert!(block_on(interface.parse_get_epic_response(epic_id, data_len)).is_ok());
        assert_eq!(epic_id, 64);
    }

    #[test]
    fn test_interface_parse_update_epic_status_response() {
        use std::path::PathBuf;
        use std::collections::HashMap;
        // Create the mock epic with stories
        let mut epic = Epic::new(
            64,
            String::from("Test Epic"),
            String::from("A good epic for testing purposes"),
            Status::Open,
            PathBuf::default(),
            HashMap::new()
        );
        let test_story1 = Story::new(
            97,
            64,
            String::from("Test Story 1"),
            String::from("A good first story for testing purposes"),
            Status::Open
        );
        let test_story2 = Story::new(
            98,
            64,
            String::from("Test Story 2"),
            String::from("A good second story for testing purposes"),
            Status::Open
        );
        let test_story3 = Story::new(
            99,
            64,
            String::from("Test Story 3"),
            String::from("A good third story for testing purposes"),
            Status::Open
        );

        assert!(epic.add_story(test_story1).is_ok());
        assert!(epic.add_story(test_story2).is_ok());
        assert!(epic.add_story(test_story3).is_ok());

        // Update epic's status
        epic.update_status(Status::InProgress);

        // Create the mock response
        let update_epic_status_response = Response::EpicStatusUpdateOk(64, epic.as_bytes());

        // Set up mock connection stream, and client input
        let client_input = BufReader::new(stdin());
        let mut connection_stream: Cursor<Vec<u8>> = Cursor::new(vec![]);

        assert!(block_on(connection_stream.write_all(update_epic_status_response.as_bytes().as_slice())).is_ok());

        let mut tag_buf = [0u8; 18];

        // Move cursor position back to start
        assert!(block_on(connection_stream.seek(SeekFrom::Start(0))).is_ok());

        // Simulate reading response tag from server
        assert!(block_on(connection_stream.read_exact(&mut tag_buf)).is_ok());

        // simulate deserializing the response
        let (type_and_flag, epic_id, story_id, data_len) = Response::decode(&tag_buf);

        // Build the interface
        let mut interface = Interface::new(connection_stream, client_input);
        // Create pages to simulate real situation
        interface.page_stack.push(Box::new(HomePage {epic_frames: vec![]}));
        interface.page_stack.push(Box::new(
            EpicDetailPage {
                frame: EpicFrame {
                    id: 64,
                    name: String::from("Test Epic"),
                    description: String::from("A good epic for testing purposes"),
                    status: Status::Open
                },
                story_frames: vec![]}));

        // Ensure response is handled correctly
        assert!(block_on(interface.parse_update_epic_status_response(epic_id, data_len)).is_ok());
        assert_eq!(epic_id, 64);
    }

    #[test]
    fn test_interface_parse_epic_does_not_exist_response() {
        // Set up the mock database
        let mut db_handle = DbState::new(
            String::default(),
            String::default(),
            String::default(),
        );

        db_handle.add_epic(String::from("Test Epic 1"), String::from("A good first epic for testing purposes"));
        db_handle.add_epic(String::from("Test Epic 2"), String::from("A good second epic for testing purposes"));
        db_handle.add_epic(String::from("Test Epic 3"), String::from("A good third epic for testing purposes"));
        db_handle.add_epic(String::from("Test Epic 4"), String::from("A good fourth epic for testing purposes"));

        let db_bytes = db_handle.as_bytes();

        // Set up mock connection stream, and client input
        let client_input = BufReader::new(stdin());
        let mut connection_stream: Cursor<Vec<u8>> = Cursor::new(vec![]);

        // Ensure the epic with id 2353 does not exist
        assert!(!db_handle.contains_epic(2353));

        // Write the mock response to the connection stream
        let epic_does_not_exist_response = Response::EpicDoesNotExist(2353, db_handle.as_bytes());

        assert!(block_on(connection_stream.write_all(epic_does_not_exist_response.as_bytes().as_slice())).is_ok());

        // Tag buffer for reading response
        let mut tag_buf = [0u8; 18];

        assert!(block_on(connection_stream.seek(SeekFrom::Start(0))).is_ok());

        // simulate reading the response from the server
        assert!(block_on(connection_stream.read_exact(&mut tag_buf)).is_ok());

        // simulate deserializing the response
        let (type_and_flag, epic_id, story_id, data_len) = Response::decode(&tag_buf);

        // Build the interface
        let mut interface = Interface::new(connection_stream, client_input);
        interface.page_stack.push(Box::new(HomePage { epic_frames: vec![] }));

        // Ensure response is handled correctly
        assert_eq!(epic_id, 2353);
        assert!(block_on(interface.parse_epic_does_not_exist_response(epic_id, data_len)).is_ok());
    }

    #[test]
    fn test_interface_get_story_response() {
        use std::path::PathBuf;
        use std::collections::HashMap;
        // Create the mock epic with stories
        let mut epic = Epic::new(
            64,
            String::from("Test Epic"),
            String::from("A good epic for testing purposes"),
            Status::Open,
            PathBuf::default(),
            HashMap::new()
        );
        let test_story1 = Story::new(
            97,
            64,
            String::from("Test Story 1"),
            String::from("A good first story for testing purposes"),
            Status::Open
        );
        let test_story2 = Story::new(
            98,
            64,
            String::from("Test Story 2"),
            String::from("A good second story for testing purposes"),
            Status::Open
        );
        let test_story3 = Story::new(
            99,
            64,
            String::from("Test Story 3"),
            String::from("A good third story for testing purposes"),
            Status::Open
        );

        assert!(epic.add_story(test_story1).is_ok());
        assert!(epic.add_story(test_story2).is_ok());
        assert!(epic.add_story(test_story3).is_ok());

        // Ensure epic contains story with id 99
        assert!(epic.get_story(99).is_some());

        // Create the mock response
        let get_story_response = Response::GetStoryOk(99, epic.get_story(99).unwrap().as_bytes());

        // Set up mock connection stream, and client input
        let client_input = BufReader::new(stdin());
        let mut connection_stream: Cursor<Vec<u8>> = Cursor::new(vec![]);

        assert!(block_on(connection_stream.write_all(get_story_response.as_bytes().as_slice())).is_ok());

        let mut tag_buf = [0u8; 18];

        // Move cursor position back to start
        assert!(block_on(connection_stream.seek(SeekFrom::Start(0))).is_ok());

        // Simulate reading response tag from server
        assert!(block_on(connection_stream.read_exact(&mut tag_buf)).is_ok());

        // simulate deserializing the response
        let (type_and_flag, epic_id, story_id, data_len) = Response::decode(&tag_buf);

        // Build the interface
        let mut interface = Interface::new(connection_stream, client_input);
        // Create pages to simulate real situation
        interface.page_stack.push(Box::new(HomePage {epic_frames: vec![]}));
        interface.page_stack.push(Box::new(
            EpicDetailPage {
                frame: EpicFrame {
                    id: 64,
                    name: String::from("Test Epic"),
                    description: String::from("A good epic for testing purposes"),
                    status: Status::Open
                },
                story_frames: vec![]}));

        // Ensure response is handled correctly
        assert!(block_on(interface.parse_get_story_response(story_id, data_len)).is_ok());
        assert_eq!(story_id, 99);
    }


}