use std::io::Read;
use std::fmt::{self, Write, Display};
use std::ops::Range;
use circle_buffer::CircleBuffer;


/// Searches for a set of phrases.
pub struct Finder<'a, R: Read> {
    phrases: &'a [Phrase],              // Phrases to search for
    phrase_skip_counters: Vec<usize>,   // Skip counter parallel to phrases
    reader: &'a mut R,                  // Input to search
    current_byte: usize,                // Current position of the stream we're in. Similar to file position.
    context: CircleBuffer<u8>,          // Buffer that bytes from input will be sent to / searched in
    window_size: usize,                 // Size of the window into the context
    window_right: usize,                // Last index + 1 of the window
    flush_counter: usize                // How many extra times we need to slice the window to the right at the end of the file
}

impl<'a, R: Read> Iterator for Finder<'a, R> {
    type Item = PhraseInstanceGroup;
    
    fn next(&mut self) -> Option<Self::Item> {

        let mut phrase_instances = Vec::new();

        // Reads in next char.
        // If it's not None...
        let mut next = self.next_char();
        while let Some(char) = next {

            // Put the char into the circle buffer and search for phrases in it
            phrase_instances.clear();
            self.context.push(char);
            self.find_phrases(&mut phrase_instances);
            self.current_byte += 1;

            // If at least once instance was found, return it as a group
            if !phrase_instances.is_empty() {
                return Some(PhraseInstanceGroup(phrase_instances));
            }

            // Otherwise, keep searching
            next = self.next_char();
        }

        // EOF. Flush the remainder of the window
        while self.flush_counter > 0 {
            phrase_instances.clear();
            self.context.push(0);
            self.find_phrases(&mut phrase_instances);
            self.current_byte += 1;
            self.flush_counter -= 1;
            if !phrase_instances.is_empty() {
                return Some(PhraseInstanceGroup(phrase_instances));
            }
        }

        // Done
        None
    }
}

impl<'a, R: Read> Finder<'a, R> {
    pub fn new(
        phrases: &'a [Phrase],
        context_size: usize,
        window_size: usize,
        reader: &'a mut R
    ) -> Self {
        if context_size % 4 != 0 {
            panic!("Context size must be divisible by 4");
        }
        if window_size > context_size {
            panic!("Window size must be <= context_size");
        }

        let ws = window_size;
        let hws = ws / 2;
        let c_mid = context_size/2;
        let w_left = if c_mid > hws  { c_mid - hws } else { 0 };
        let w_right = w_left + window_size;
        let w_right = if w_right > context_size { context_size } else { w_right };

        Self {
            phrases,
            phrase_skip_counters: vec![0; phrases.len()],
            context: CircleBuffer::with_capacity(context_size),
            window_size,
            window_right: w_right,
            reader,
            current_byte: 0,
            flush_counter: context_size - w_right
        }
    }

    pub fn get_context_range(&self) -> Range<usize> {
        Range {
            start: self.current_byte - self.context.len(),
            end: self.current_byte
        }
    }

    pub fn get_window_range(&self) -> Range<usize> {
        let (w_left, w_right) = self.get_window_bounds();
        Range {
            start: w_left - self.current_byte,
            end: w_right - self.current_byte
        }
    }

    pub fn context_size(&self) -> usize { self.context.len() }

    pub fn file_pos(&self) -> usize { self.current_byte }

    /// Gets context for this finder
    pub fn get_context(&self, codepoint_diff: i32, bytes_per_character: u32) -> Text {
        Text::from_slice(self.context.as_slice(), codepoint_diff, bytes_per_character)
    }

    // Finds phrases in current window
    fn find_phrases(&mut self, phrase_instances: &mut Vec<PhraseInstance>) {

        // For all phrases..
        for i in 0..self.phrases.len() {

            // If phrase is to be skipped, skip it
            let skip = &mut self.phrase_skip_counters[i];
            if *skip > 0 {
                *skip -= 1;
                continue;
            }

            // Search for phrase
            let (w_left, w_right) = self.get_window_bounds();
            self.find_phrase(i, w_left, w_right, phrase_instances);
        }
    }

    /// Searches for a single phrase
    fn find_phrase(
        &mut self,
        phrase_index: usize,
        w_left: usize,
        w_right: usize,
        instances: &mut Vec<PhraseInstance>
    ) {

        // Computes the window of the current context
        let phrase = &self.phrases[phrase_index];
        let context = self.context.as_slice();
        let window = &context[w_left..w_right];

        // Searches for the phrase in the window calculated
        let mut earliest_token_idx = usize::MAX;    // Index of earliest token index found
        let mut last_diff: Option<i32> = None;      // Last character diff
        let mut last_bpc = 0;                       // Last bytes-per-character
        for token in &phrase.0 {

            // If token was found in the buffer...
            if let Some(token_instance) = search_multibyte(&token.0, window, last_diff) {

                // If another token in the phrase was found previously, but it had a different
                // codepoint diff or bytes-per-character value, it's a failed match
                if let Some(last_diff) = last_diff {
                    let diff = token_instance.codepoint_diff;
                    let bpc = token_instance.bytes_per_character;
                    if diff != last_diff || bpc != last_bpc {
                        return;
                    }
                }

                // Keep track of the earliest token index in the phrase so we know how much to skip when we're done
                let token_idx = token_instance.index;
                if token_idx < earliest_token_idx {
                    earliest_token_idx = token_idx;
                    last_diff = Some(token_instance.codepoint_diff);
                    last_bpc = token_instance.bytes_per_character;
                }
            }

            // Otherwise, it's a failed match
            else {
                return;
            }
        }

        // Add the buffer's contents to results and skip past the phrase
        let bytes_read = self.current_byte + 1;
        let w_left_pos = bytes_read - self.context.len() + w_left;
        instances.push(PhraseInstance {
            phrase_index,
            codepoint_diff: last_diff.unwrap(),
            file_pos: w_left_pos + earliest_token_idx,
            bytes_per_character: last_bpc
        });
        self.phrase_skip_counters[phrase_index] = earliest_token_idx;
    }

    fn get_window_bounds(&self) -> (usize, usize) {
        let w_right = if self.window_right > self.context.len() { self.context.len() } else { self.window_right };
        let w_left = if w_right > self.window_size { w_right - self.window_size } else { 0 };
        (w_left, w_right)
    }

    fn next_char(&mut self) -> Option<u8> {
        let mut b: [u8; 1] = [0];
        match self.reader.read(&mut b) {
            Ok(bytes_read) => {
                match bytes_read {
                    0 => None,
                    _ => {
                        Some(b[0])
                    }
                }
            },
            Err(_) => None
        }
    }
}

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

impl fmt::Display for Text {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.write_chars(f)?;
        Ok(())
    }
}
impl AsRef<[u32]> for Text {
    fn as_ref(&self) -> &[u32] { &self.0 }
}


/// Searches for a within b
fn search_multibyte(a: &[u32], b: &[u8], codepoint_diff: Option<i32>) -> Option<TokenInstance> {
    if let Some(codepoint_diff) = codepoint_diff {
        let result = search_with_diff(a, b, codepoint_diff);
        if result.is_some() {
            return result;
        }
        let result = search_2bytes_with_diff(a, b, codepoint_diff);
        if result.is_some() {
            return result;
        }
    }
    else {
        let result = search(a, b);
        if result.is_some() {
            return result;
        }
        let result = search_2bytes(a, b);
        if result.is_some() {
            return result;
        }
    }
    None
}

/// Searches for a within b
fn search(a: &[u32], b: &[u8]) -> Option<TokenInstance> {
    let a = a.as_ref();
    let b = b.as_ref();
    let b_len = b.len();
    if a.is_empty() { return None; }
    if a.len() > b_len { return None; }
    'outer: for b_idx in 0..(b_len - a.len() + 1) {
        let b_at_idx = b[b_idx];
        let codepoint_diff = b_at_idx as i32 - a[0] as i32;
        for a_idx in 0..a.len() {
            let char_a = a[a_idx] as i32;
            let char_b = b[b_idx + a_idx] as u32;
            let char_b = char_b as i32 - codepoint_diff;
            if char_a != char_b { continue 'outer; }
        }
        return Some(TokenInstance {
            index: b_idx,
            codepoint_diff,
            bytes_per_character: 1
        });
    }
    return None;
}

/// Searches for a within b. Assumes b is 2 bytes per character.
fn search_2bytes(a: &[u32], b: &[u8]) -> Option<TokenInstance> {
    let a = a.as_ref();
    let b = b.as_ref();
    let b_len = b.len() / 2;
    if a.is_empty() { return None; }
    if a.len() > b_len { return None; }
    'outer: for b_idx in 0..(b_len - a.len() + 1) {
        let b_at_idx = get_2bytes(b, b_idx);
        let codepoint_diff = b_at_idx as i32 - a[0] as i32;
        for a_idx in 0..a.len() {
            let char_a = a[a_idx] as i32;
            let char_b = get_2bytes(b, b_idx + a_idx);
            let char_b = char_b as i32 - codepoint_diff;
            if char_a != char_b { continue 'outer; }
        }
        return Some(TokenInstance {
            index: b_idx*2,
            codepoint_diff,
            bytes_per_character: 2
        });
    }
    return None;
}

/// Searches for a within b, assuming the specified codepoind diff
fn search_with_diff(a: &[u32], b: &[u8], codepoint_diff: i32) -> Option<TokenInstance> {
    let a = a.as_ref();
    let b = b.as_ref();
    let b_len = b.len();
    if a.is_empty() { return None; }
    if a.len() > b_len { return None; }
    'outer: for b_idx in 0..(b_len - a.len() + 1) {
        let b_at_idx = b[b_idx];
        for a_idx in 0..a.len() {
            let char_a = a[a_idx] as i32;
            let char_b = b[b_idx + a_idx] as u32;
            let char_b = char_b as i32 - codepoint_diff;
            if char_a != char_b { continue 'outer; }
        }
        return Some(TokenInstance {
            index: b_idx,
            codepoint_diff,
            bytes_per_character: 1
        });
    }
    return None;
}

/// Searches for a within b. Assumes b is 2 bytes per character.
fn search_2bytes_with_diff(a: &[u32], b: &[u8], codepoint_diff: i32) -> Option<TokenInstance> {
    let a = a.as_ref();
    let b = b.as_ref();
    let b_len = b.len() / 2;
    if a.is_empty() { return None; }
    if a.len() > b_len { return None; }
    'outer: for b_idx in 0..(b_len - a.len() + 1) {
        let b_at_idx = get_2bytes(b, b_idx);
        for a_idx in 0..a.len() {
            let char_a = a[a_idx] as i32;
            let char_b = get_2bytes(b, b_idx + a_idx);
            let char_b = char_b as i32 - codepoint_diff;
            if char_a != char_b { continue 'outer; }
        }
        return Some(TokenInstance {
            index: b_idx*2,
            codepoint_diff,
            bytes_per_character: 2
        });
    }
    return None;
}

pub fn get_2bytes(slice: &[u8], idx: usize) -> u32 {
    let a = slice[idx*2] as u32;
    let b = slice[idx*2+1] as u32;
    a + (b << 8)
}


/// Result of a text search
#[derive(Debug, Copy, Clone)]
pub struct TokenInstance {
    pub index: usize,
    pub codepoint_diff: i32,
    pub bytes_per_character: u32
}

/// A sequence of texts
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Phrase(pub Vec<Text>);
impl Phrase {
    pub fn from_strs(strs: &[&str]) -> Self {
        let texts = strs
            .iter()
            .map(|str| Text::from_str(str))
            .collect();
        Self(texts)
    }
}

impl Display for Phrase {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for (index, text) in self.0.iter().enumerate() {
            text.fmt(f)?;
            if index != self.0.len() - 1 {
                f.write_char(' ')?;
            }
        }
        Ok(())
    }
}

/// Instance of a phrase found
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct PhraseInstance {
    pub phrase_index: usize,
    pub file_pos: usize,
    pub codepoint_diff: i32,
    pub bytes_per_character: u32
}

/// A group of phrase instances
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub struct PhraseInstanceGroup(pub Vec<PhraseInstance>);

#[test]
fn test_finder_1() {
    use std::io::BufReader;
    // "Reads" input
    let input: &[u8] = include_bytes!("test_text_1.txt");
    let mut reader = BufReader::new(input);

    // Sets up finder
    let phrase = Phrase::from_strs(&["famine", "where"]);
    let phrases = &[phrase];
    let finder = Finder::new(phrases, 40, 20, &mut reader);

    // Runs finder and checks
    let expected = vec![PhraseInstance {
        phrase_index: 0,
        codepoint_diff: 0,
        file_pos: 288,
        bytes_per_character: 1
    }];
    let groups: Vec<PhraseInstanceGroup> = finder.collect();
    let actual: Vec<PhraseInstance> = groups
        .iter()
        .flat_map(|group| group.0.iter())
        .map(|instance| instance.clone())
        .collect();
    assert_eq!(expected, actual);
}

#[test]
fn test_finder_2() {
    use std::io::BufReader;
    // "Reads" input
    let input: &[u8] = include_bytes!("test_text_2.txt");
    let mut reader = BufReader::new(input);

    // Sets up finder
    let phrase = Phrase::from_strs(&["within", "sunken", "deep"]);
    let phrases = &[phrase];
    let finder = Finder::new(phrases, 64, 32, &mut reader);
    
    // Runs finder and checks
    let expected = vec![PhraseInstance {
        phrase_index: 0,
        codepoint_diff: 0,
        file_pos: 285,
        bytes_per_character: 1
    }];
    let groups: Vec<PhraseInstanceGroup> = finder.collect();
    let actual: Vec<PhraseInstance> = groups
        .iter()
        .flat_map(|group| (*group.0).iter())
        .map(|instance| instance.clone())
        .collect();
    assert_eq!(expected, actual);
}

#[test]
fn test_finder_edgecase() {
    use std::io::BufReader;
    // "Reads" input
    let input: &[u8] = "Four letter word".as_bytes();
    let mut reader = BufReader::new(input);

    // Sets up finder
    let phrase = Phrase::from_strs(&["word"]);
    let phrases = &[phrase];
    let finder = Finder::new(phrases, 8, 4, &mut reader);
    
    // Runs finder and checks
    let expected = vec![PhraseInstance {
        phrase_index: 0,
        codepoint_diff: 0,
        file_pos: 12,
        bytes_per_character: 1
    }];
    let groups: Vec<PhraseInstanceGroup> = finder.collect();
    let actual: Vec<PhraseInstance> = groups
        .iter()
        .flat_map(|group| (*group.0).iter())
        .map(|instance| instance.clone())
        .collect();
    assert_eq!(expected, actual);
}

#[test]
fn test_finder_multiphrase() {
    use std::io::BufReader;
    // "Reads" input
    let input: &[u8] = include_bytes!("test_text_2.txt");
    let mut reader = BufReader::new(input);

    // Sets up finder
    let phrase1 = Phrase::from_strs(&["within", "sunken", "deep"]);
    let phrase2 = Phrase::from_strs(&["sum", "my", "count"]);
    let phrases = &[phrase1, phrase2];
    let finder = Finder::new(phrases, 64, 32, &mut reader);
    
    // Runs finder and checks
    let expected = vec![
        PhraseInstance {
            phrase_index: 0,
            codepoint_diff: 0,
            file_pos: 285,
            bytes_per_character: 1
        },
        PhraseInstance {
            phrase_index: 1,
            file_pos: 479,
            codepoint_diff: 0,
            bytes_per_character: 1
        }
    ];
    let groups: Vec<PhraseInstanceGroup> = finder.collect();
    let actual: Vec<PhraseInstance> = groups
        .iter()
        .flat_map(|group| (*group.0).iter())
        .map(|instance| instance.clone())
        .collect();
    assert_eq!(expected, actual);
}


#[test]
fn test_finder_context() {
    use std::io::BufReader;
    // "Reads" input
    let input: &[u8] = include_bytes!("test_text_1.txt");
    let mut reader = BufReader::new(input);

    // Sets up finder
    let phrase = Phrase::from_strs(&["famine", "where"]);
    let phrases = &[phrase];
    let mut finder = Finder::new(phrases, 40, 20, &mut reader);

    // Runs finder and checks
    finder.next().unwrap();
    let context = finder.get_context(0, 1);
    assert_eq!(Text::from_str(" fuel,\n  Making a famine where abundance"), context);
}

#[test]
fn test_finder_u16_le() {
    use std::io::BufReader;
    // "Reads" input as little-endian
    let input: &[u8] = include_bytes!("test_text_2.txt");
    let input_le: Vec<u8> = input
        .iter()
        .flat_map(|b| [*b, 0])
        .collect();
    let mut reader = BufReader::new(input_le.as_slice());

    // Sets up finder
    let phrase = Phrase::from_strs(&["within", "sunken", "deep"]);
    let phrases = &[phrase];
    let finder = Finder::new(phrases, 128, 64, &mut reader);

    // Runs finder and checks
    let expected = vec![PhraseInstance {
        phrase_index: 0,
        codepoint_diff: 0,
        file_pos: 570,
        bytes_per_character: 2
    }];
    let groups: Vec<PhraseInstanceGroup> = finder.collect();
    let actual: Vec<PhraseInstance> = groups
        .iter()
        .flat_map(|group| group.0.iter())
        .map(|instance| instance.clone())
        .collect();
    assert_eq!(expected, actual);
}

#[test]
fn test_finder_u16_be() {
    use std::io::BufReader;
    // "Reads" input as big-endian
    let input: &[u8] = include_bytes!("test_text_2.txt");
    let input_be: Vec<u8> = input
        .iter()
        .flat_map(|b| [0, *b])
        .collect();
    let mut reader = BufReader::new(input_be.as_slice());

    // Sets up finder
    let phrase = Phrase::from_strs(&["within", "sunken", "deep"]);
    let phrases = &[phrase];
    let finder = Finder::new(phrases, 128, 64, &mut reader);

    // Runs finder and checks
    let expected = vec![PhraseInstance {
        phrase_index: 0,
        codepoint_diff: 0,
        file_pos: 571,
        bytes_per_character: 2
    }];
    let groups: Vec<PhraseInstanceGroup> = finder.collect();
    let actual: Vec<PhraseInstance> = groups
        .iter()
        .flat_map(|group| group.0.iter())
        .map(|instance| instance.clone())
        .collect();
    assert_eq!(expected, actual);
}


#[test]
fn test_finder_offset13() {
    use std::io::BufReader;
    // "Reads" input
    let input = include_bytes!("test_text_2.txt");
    let rotated_input: Vec<u8> = input.iter().map(|b| b + 13).collect();
    let mut reader = BufReader::new(rotated_input.as_slice());

    // Sets up finder
    let phrase = Phrase::from_strs(&["within", "sunken", "deep"]);
    let phrases = &[phrase];
    let finder = Finder::new(phrases, 64, 32, &mut reader);

    // Runs finder and checks
    let expected = vec![PhraseInstance {
        phrase_index: 0,
        codepoint_diff: 13,
        file_pos: 285,
        bytes_per_character: 1
    }];
    let groups: Vec<PhraseInstanceGroup> = finder.collect();
    let actual: Vec<PhraseInstance> = groups
        .iter()
        .flat_map(|group| group.0.iter())
        .map(|instance| instance.clone())
        .collect();
    assert_eq!(expected, actual);
}


#[test]
fn test_search() {
    let b: Vec<u8> = "This is the text we're testing".bytes().collect();

    let a = Text::from_str("text");
    assert!(search(&a.0, &b).is_some());
    let a = Text::from_str("text!");
    assert!(search(&a.0, &b).is_none());
    let a = Text::from_str("e're");
    assert!(search(&a.0, &b).is_some());
    let a = Text::from_str("this");
    assert!(search(&a.0, &b).is_none());
}


#[test]
fn test_edgecase() {
    let mut input = "fuel,   Making a famine w".as_bytes();
    // Sets up finder
    let phrase = Phrase::from_strs(&["Making", "a"]);
    let phrases = &[phrase];
    let mut finder = Finder::new(phrases, 64, 32, &mut input);

    let found = finder.next();
    let expected = Some(PhraseInstanceGroup(vec![PhraseInstance {
        phrase_index: 0,
        file_pos: 8,
        codepoint_diff: 0,
        bytes_per_character: 1
    }]));
    assert_eq!(expected, found);
}