use std::num::ParseIntError;
use thiserror::Error;

#[derive(Clone)]
pub struct FileEntry {
    pub name: Option<String>,
    pub size: Option<u64>,
    pub len: Option<usize>,
    pub next: Option<u64>,
}

#[derive(Error, Debug)]
pub enum ParseError {
    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error(transparent)]
    ParseIntError(#[from] ParseIntError),
}

impl Default for FileEntry {
    fn default() -> Self {
        Self {
            name: None,
            size: None,
            len: None,
            next: None,
        }
    }
}

impl FileEntry {
    pub fn from_str(str: &str) -> Result<FileEntry, ParseError> {
        let mut out = FileEntry::default();
        if str.is_empty() {
            return Ok(out);
        }
        for line in str.split("\n") {
            if line.starts_with("#") {
                continue;
            }

            let mut assignment = line.split("=");
            let key = assignment
                .next()
                .ok_or(ParseError::InvalidInput(str.into()))?;
            let val = assignment
                .next()
                .ok_or(ParseError::InvalidInput(str.into()))?;

            match key {
                "name" => out.name = Some(val.into()),
                "size" => out.size = Some(val.parse()?),
                "len" => out.len = Some(val.parse()?),
                "next" => out.next = Some(val.parse()?),
                _ => {}
            }
        }
        Ok(out)
    }
}
