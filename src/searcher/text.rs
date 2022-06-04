use serde::{Serialize, Deserialize};
use std::fmt::{self, Write};

/// A "String" as a sequence of u32s
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Text(pub Vec<u32>);
impl Text {
    pub fn from_str(str: &str) -> Self {
        let vec = str
            .chars()
            .map(|c| c as u32 )
            .collect();
        Self(vec)
    }

    pub fn from_slice(slice: &[u8], codepoint_diff: i32, bytes_per_char: u32) -> Self {
        match bytes_per_char {
            1 => Self::from_slice_1byte(slice, codepoint_diff),
            2 => Self::from_slice_2bytes(slice, codepoint_diff),
            _ => panic!("Invalid bytes_per_char {}. Must be 1, 2 or 4", bytes_per_char)
        }
    }

    pub fn from_slice_1byte(slice: &[u8], codepoint_diff: i32) -> Self {
        let mut vec = Vec::with_capacity(slice.len());
        for num in slice {
            let num = (*num as i32 - codepoint_diff) as u32;
            vec.push(num);
        }
        Self(vec)
    }

    pub fn from_slice_2bytes(slice: &[u8], codepoint_diff: i32) -> Self {
        let mut vec = Vec::with_capacity(slice.len());
        for chunk in slice.chunks(2) {
            let num = chunk[0] as u32 + ((chunk[1] as u32) << 8);
            let num = (num as i32 - codepoint_diff) as u32;
            vec.push(num);
        }
        Self(vec)
    }

    fn write_chars(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for char_u32 in &self.0 {
            let char_u32 = *char_u32;
            let char = match char::try_from(char_u32) {
                Ok(char) => char,
                Err(_) => '?'
            };
            if char_u32 >= 32 && char_u32 <= 126 {
                f.write_char(char)?;
            }
            else {
                match char {
                    '\n' | '\r' | '\t' | '\0' => f.write_char(' ')?,
                    _ => f.write_char('?')?
                }
            }
        }
        Ok(())
    }
}
impl fmt::Debug for Text {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_char('"')?;
        self.write_chars(f)?;
        f.write_char('"')?;
        Ok(())
    }
}

impl Serialize for Text {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Text {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where D: serde::Deserializer<'de> {
        let string = String::deserialize(deserializer)?;
        Ok(Self::from_str(&string))
    }
}

impl fmt::Display for Text {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.write_chars(f)?;
        Ok(())
    }
}

impl AsRef<[u32]> for Text {
    fn as_ref(&self) -> &[u32] { &self.0 }
}