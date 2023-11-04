//! Provides the necessary components for creating a smoothing CLI for clients interacting
//! with the server.
//!

use async_std::net::TcpStream;
use async_std::io::{ReadExt, Read};
use async_jira_cli::response::prelude::*;
use async_jira_cli::utils::prelude::*;
use crate::UserError;

mod frame;
mod pages;

use pages::prelude::*;

// use frame::prelude::*;
// use pages::prelude::*;

/// Implements the functionality for creating a CLI that a client can use to interact with
/// the server.
///
/// The struct can take correctly parsed `Response` tags read from a `TcpStream` and display
/// the appropriate prompt to the user on stdout via `parse_response` method. Internally,
/// an `Interface` will keep a stack of pages that can be navigated through by the client, and
/// will display appropriate messages when errors arise.
pub struct Interface {
    page_stack: Vec<Box<dyn Page>>,
}

impl Interface {
    // Attempt to read a response from the stream. If response read correctly, then parse frames
    // from the read data and create a new page and ad page to page_stack, or display a message and a current page depending
    // on the logic.
    pub async fn parse_response(&mut self, tag_buf: &[u8; 18], stream: &TcpStream) -> Result<(), UserError> {
        let mut stream = stream;
        let (type_and_flag, epic_id, story_id, data_len) = Response::decode(tag_buf);

        // Check the type of response and then parse accordingly
        if type_and_flag & 1 != 0 {
            let mut data = vec![0; data_len as usize];
            stream.read_exact(data.as_mut_slice())
                .await
                .map_err(|_|
                UserError::ParseResponseError(format!("unable to parse response from server correctly"))
            )?;
            // Attempt to create homepage
            let homepage = Box::new(HomePage::try_create(data)?);
            // Display homepage if successful
            homepage.print_page();
            println!();
            // Save homepage to `self.page_stack`
            self.page_stack.push(homepage);
        } else if type_and_flag & 2 != 0 {
            // This is a suspicious case, it should not happen so return an internal server error to the client
            return Err(UserError::InternalServerError);
        } else if type_and_flag & 4 != 0 {
            // Read updated homepage data
            let mut data = vec![0; data_len as usize];
            stream.read_exact(data.as_mut_slice())
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
            homepage.print_page();
            println!();

            self.page_stack = vec![homepage];
        } else if type_and_flag & 8 != 0 {
            // update the Homepage
            let mut data = vec![0; data_len as usize];
            stream.read_exact(data.as_mut_slice())
                .await
                .map_err(|_|
                    UserError::ParseResponseError(format!("unable to parse response from server correctly"))
                )?;
            // Attempt to create homepage
            let homepage = Box::new(HomePage::try_create(data)?);

            println!("epic with id: {} was deleted successfully", epic_id);
            println!();

            // Display homepage if successful
            homepage.print_page();
            println!();

            self.page_stack = vec![homepage];
        } else if type_and_flag & 16 != 0 {
            let mut data = vec![0; data_len as usize];
            stream.read_exact(data.as_mut_slice())
                .await
                .map_err(|_| UserError::ParseResponseError(String::from("unable to parse response from server correctly")))?;
            let epic_detail_page = Box::new(EpicDetailPage::try_create(data)?);

            println!("get epic with id: {} successful", epic_id);
            println!();

            epic_detail_page.print_page();

            self.page_stack.push(epic_detail_page);

        } else if type_and_flag & 32 != 0 {
            let mut data = vec![0; data_len as usize];
            stream.read_exact(data.as_mut_slice())
                .await
                .map_err(|_| UserError::ParseResponseError(String::from("unable to parse response from server correctly")))?;

            let epic_detail_page = Box::new(EpicDetailPage::try_create(data)?);

            println!("updated status of epic with id: {} successful", epic_id);

            assert!(!self.page_stack.is_empty());
            self.page_stack.pop();
            self.page_stack.push(epic_detail_page);

        } else if type_and_flag & 64 != 0 {
            let mut data = vec![0; data_len as usize];
            stream.read_exact(data.as_mut_slice())
                .await
                .map_err(|_| UserError::ParseResponseError(String::from("unable to parse response from server correctly")))?;

            let homepage = Box::new(HomePage::try_create(data)?);

            println!("epic with id: {} does not exist", epic_id);
            println!();

            homepage.print_page();
            self.page_stack = vec![homepage];
        } else if type_and_flag & 128 != 0 {
            let mut data = vec![0; data_len as usize];
            stream.read_exact(data.as_mut_slice())
                .await
                .map_err(|_| UserError::ParseResponseError(String::from("unable to parse response from server correctly")))?;

            let story_detail_page = Box::new(StoryDetailPage::try_create(data)?);

            println!("get story with id: {} successful", story_id);
            println!();

            story_detail_page.print_page();
            self.page_stack.push(story_detail_page);
        } else if type_and_flag & 256 != 0 {
            let mut data = vec![0; data_len as usize];
            stream.read_exact(data.as_mut_slice())
                .await
                .map_err(|_| UserError::ParseResponseError(String::from("unable to parse response from server correctly")))?;

            let epic_detail_page = Box::new(StoryDetailPage::try_create(data)?);

            println!("story with id: {} does not exist", story_id);
            println!();

            epic_detail_page.print_page();
            assert!(!self.page_stack.is_empty());

            while self.page_stack.len() > 1 {
                self.page_stack.pop();
            }

            self.page_stack.push(epic_detail_page);
        } else if type_and_flag & 512 != 0 {
            let mut data = vec![0; data_len as usize];
            stream.read_exact(data.as_mut_slice())
                .await
                .map_err(|_| UserError::ParseResponseError(String::from("unable to parse response from server correctly")))?;

            let epic_detail_page = Box::new(StoryDetailPage::try_create(data)?);

            println!("story with id: {} was added successfully", story_id);
            println!();

            epic_detail_page.print_page();

            self.page_stack.push(epic_detail_page);
        } else if type_and_flag & 1024 != 0 {
            let mut data = vec![0; data_len as usize];
            stream.read_exact(data.as_mut_slice())
                .await
                .map_err(|_| UserError::ParseResponseError(String::from("unable to parse response from server correctly")))?;

            let epic_detail_page = Box::new(StoryDetailPage::try_create(data)?);

            println!("story with id: {} was deleted successfully", story_id);
            println!();

            epic_detail_page.print_page();

            while self.page_stack.len() > 1 {
                self.page_stack.pop();
            }

            self.page_stack.push(epic_detail_page);
        } else if type_and_flag & 2048 != 0 {
            let mut data = vec![0; data_len as usize];
            stream.read_exact(data.as_mut_slice())
                .await
                .map_err(|_| UserError::ParseResponseError(String::from("unable to parse response from server correctly")))?;

            let story_detail_page = Box::new(StoryDetailPage::try_create(data)?);

            println!("updated status of story with id: {} was successful", story_id);
            println!();

            story_detail_page.print_page();

            while self.page_stack.len() > 2 {
                self.page_stack.pop();
            }

            self.page_stack.push(story_detail_page);
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
}