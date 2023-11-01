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
struct HomePage {
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
                        return Err(UserError::ReadFrameError("unable to read frame from response data".to_string()));
                    }
                    break;
                }
                _ => return Err(UserError::ReadFrameError("unable to read frame from response data".to_string())),
            }
            // Keep track of the current position,
            // used to ensure that we have read properly formatted data from the server
            cur_pos = cursor.position();
        }

        Ok(HomePage { epic_frames })
    }
}

impl Page for HomePage {
    fn print_page(&self) {
        println!("{:-^65}", "EPICS");
        print!("{:^13}|", "id");
        print!("{:^33}|", "name");
        print!("{:^16}|", "status");


    }
}

fn justify_text_with_ellipses(width: usize, text: &String) -> Result<String, &'static str> {
    assert!(width > 3, "cannot center text in an interval with width less than 3");
    let text_width = unicode_width::UnicodeWidthStr::width(text.as_str());
    if text_width  > width {
        let text_chars = text.chars().collect::<Vec<char>>();
        let mut abridged_text = vec![];
        let mut abridged_text_width = 0;
        let mut i = 0;
        while i < text_chars.len() {
            // Ensure we have a valid character
            match unicode_width::UnicodeWidthChar::width(text_chars[i]) {
                Some(char_width) => {
                    // Check that we can increase the width of the abridged text, otherwise break
                    if char_width + abridged_text_width + 3 <= width {
                        abridged_text_width += char_width;
                        abridged_text.push(text_chars[i]);
                        i += 1
                    } else {
                        break;
                    }
                }
                None => return Err("unable to justify text, invalid character detected")
            }
        }
        let abridged_text = String::from_iter(abridged_text) + "...";
        Ok(format!("{:<width$}", abridged_text))
    } else {
        Ok(format!("{:<width$}", text))
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_justify_text_with_ellipses() {
        let width = 33;

        let text = String::from("Hello, World!");
        let justified_text_result = justify_text_with_ellipses(width, &text);

        println!("{:?}", justified_text_result);

        assert!(justified_text_result.is_ok());

        let justified_text = justified_text_result.unwrap();

        assert_eq!(justified_text, format!("{:<33}", "Hello, World!"));

        let text = String::from("abcdefghijklmnopqrstuvwxyzasdfjkl;");
        let justified_text_result = justify_text_with_ellipses(33, &text);

        println!("{:?}", justified_text_result);

        assert!(justified_text_result.is_ok());

        let justified_text = justified_text_result.unwrap();

        assert_eq!(justified_text, "abcdefghijklmnopqrstuvwxyzasdf...");

    }
    #[test]
    fn test_print_page_homepage() {
        let home_page = HomePage { epic_frames: vec![] };
        home_page.print_page();
    }
}





