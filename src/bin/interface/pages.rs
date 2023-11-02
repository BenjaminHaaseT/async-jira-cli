//! Contains helpful structs and traits for creating a page based CLI

use crate::interface::frame::prelude::*;
use crate::UserError;
use async_jira_cli::models::prelude::*;
use async_jira_cli::utils::prelude::*;
use std::io::{Read, Cursor, Seek, ErrorKind};
use unicode_width;

pub mod prelude {
    pub use super::*;
}

/// The `Page` trait allows for different types of CLI interface pages to share
/// the functionality of any typical CLI page i.e. `print_page`. The main purpose is to allow
/// different types of pages to be held in a single data structure as trait objects.
pub trait Page {
    fn print_page(&self);
}

/// Represents the first page that the user will see.
#[derive(Debug)]
pub struct HomePage {
    epic_frames: Vec<EpicFrame>
}

impl HomePage {
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
                _ => return Err(UserError::ReadFrameError("unable to read frame from response data, data may not be formatted correctly".to_string())),
            }
            // Keep track of the current position,
            // used to ensure that we have read properly formatted data from the server
            cur_pos = cursor.position();
        }

        Ok(HomePage { epic_frames })
    }

    fn print_line(epic_frame: &EpicFrame) {
        print!("{:<13}|", epic_frame.id);
        print!(" {:<32}|", justify_text_with_ellipses(32, &epic_frame.name));
        println!(" {:<15}", epic_frame.status)
    }
}

impl Page for HomePage {
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
}

/// A page for displaying the details of a specific Epic.
#[derive(Debug)]
pub struct EpicDetailPage {
    frame: EpicFrame,
    story_frames: Vec<StoryFrame>,
}

impl EpicDetailPage {
    pub fn try_create(data: Vec<u8>) -> Result<Self, UserError> {
        let data_len = data.len() as u64;
        let mut cursor = Cursor::new(data);
        let mut tag_buf = [0u8; 13];
        let mut cur_pos = cursor.position();

        // Attempt to parse the first epic frame from the stream of bytes
        let frame = match cursor.read_exact(&mut tag_buf) {
            Ok(_) => EpicFrame::try_from_reader(&tag_buf, &mut cursor)?,
            Err(_) => return Err(UserError::ReadFrameError(String::from("unable to read frame from response data, data may not be formatted correctly"))),
        };

        // Reset current position of cursor
        cur_pos = cursor.position();
        let mut story_frames = vec![];

        loop {
            match cursor.read_exact(&mut tag_buf) {
                Ok(_) => {
                    let story_frame = StoryFrame::try_from_reader(&tag_buf, &mut cursor)?;
                    story_frames.push(story_frame);
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
                Err(_) => return Err(UserError::ReadFrameError(String::from("unable to read frame from response data, data may not be formatted correctly"))),
            }

            cur_pos = cursor.position();
        }

        Ok(EpicDetailPage { frame, story_frames })
    }

    fn print_story_line(story_frame: &StoryFrame) {
        print!("{:<13}|", story_frame.id);
        print!(" {:<32}|", justify_text_with_ellipses(32, &story_frame.name));
        println!(" {:<15}", story_frame.status);
    }
}

impl Page for EpicDetailPage {
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
            EpicDetailPage::print_story_line(story_frame);
        }

        println!();
        println!();
        println!("[p] previous | [u] update epic | [d] delete epic | [c] create story | [:id:] navigate to story");
        println!();
    }
}


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

        homepage.print_page();
    }

    #[test]
    fn test_print_page_homepage() {
        let mut home_page = HomePage { epic_frames: vec![] };
        home_page.print_page();

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

        home_page.print_page();
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

        epic_detail_page.print_page();

        epic_detail_page.story_frames.push(StoryFrame { id: 1, name: "Test Story".to_string(), description: "A simple test Story".to_string(), status: Status::Open });
        epic_detail_page.story_frames.push(StoryFrame { id: 33, name: "Another Test Story".to_string(), description: "Another simple test Story".to_string(), status: Status::InProgress });

        epic_detail_page.print_page();
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

        let test_story1 = Story::new(107, String::from("Test Story"), String::from("Another good test story"), Status::Open);
        let test_story2 = Story::new(108, String::from("Test Story"), String::from("Another good test story"), Status::Closed);

        assert!(test_epic.add_story(test_story1).is_ok());
        assert!(test_epic.add_story(test_story2).is_ok());

        let epic_detail_page_res = EpicDetailPage::try_create(test_epic.as_bytes());

        println!("{:?}", epic_detail_page_res);
        assert!(epic_detail_page_res.is_ok());

        let epic_detail_page = epic_detail_page_res.unwrap();

        epic_detail_page.print_page();
    }
}





