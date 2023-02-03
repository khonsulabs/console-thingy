use std::ops::{Deref, DerefMut, Range};

#[derive(Debug, Default, Clone)]
pub struct Wrapped {
    string: String,
    wrapped_width: usize,
    offsets: Vec<Range<usize>>,
    dirty: bool,
}

impl Wrapped {
    pub fn lines(&mut self, width: usize) -> Lines<'_> {
        if self.dirty || self.wrapped_width != width {
            self.wrap(width);
        }

        Lines {
            source: &self.string,
            wrapped: &self.offsets,
        }
    }

    fn wrap(&mut self, chars_wide: usize) {
        self.offsets.clear();
        self.dirty = false;
        self.wrapped_width = chars_wide;

        let mut line_start = 0;
        let mut is_after_breakable = true;
        let mut last_word_start = 0;
        let mut word_char_length = 0;
        let mut line_length = 0;
        let mut chars = self.string.char_indices().peekable();
        while let Some((index, ch)) = chars.next() {
            if ch == '\n' || ch == '\r' {
                // TODO handle CRLF
                self.offsets.push(line_start..index);
                line_start = index + 1;
                last_word_start = 0;
                word_char_length = 0;
                line_length = 0;
                is_after_breakable = true;
            } else {
                line_length += 1;
                if is_break(ch) {
                    is_after_breakable = true;
                    word_char_length = 0;
                } else if is_after_breakable {
                    is_after_breakable = false;
                    last_word_start = index;
                    word_char_length = 1;
                } else {
                    word_char_length += 1;
                }
            }

            if line_length == chars_wide && chars.peek().is_some() {
                self.offsets.push(line_start..last_word_start);
                line_start = last_word_start;
                line_length = word_char_length;
            }
        }

        if line_length > 0 {
            self.offsets.push(line_start..self.string.len());
        } else if self.offsets.is_empty() {
            self.offsets.push(0..0)
        }
    }
}

impl From<String> for Wrapped {
    fn from(string: String) -> Self {
        Self {
            string,
            wrapped_width: 0,
            offsets: Vec::new(),
            dirty: true,
        }
    }
}

impl From<Wrapped> for String {
    fn from(wrapped: Wrapped) -> Self {
        wrapped.string
    }
}

impl<'a> From<&'a str> for Wrapped {
    fn from(value: &'a str) -> Self {
        Self::from(value.to_string())
    }
}

impl Deref for Wrapped {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        &self.string
    }
}

impl DerefMut for Wrapped {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.dirty = true;
        &mut self.string
    }
}

fn is_break(ch: char) -> bool {
    ch.is_ascii_punctuation() || ch == ' ' || ch == '\t' || ch.is_ascii_control()
}

#[derive(Debug)]
pub struct Lines<'a> {
    source: &'a str,
    wrapped: &'a [Range<usize>],
}

impl<'a> Iterator for Lines<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        let range = self.wrapped.first()?.clone();
        self.wrapped = &self.wrapped[1..];
        Some(&self.source[range])
    }
}

impl<'a> ExactSizeIterator for Lines<'a> {
    fn len(&self) -> usize {
        self.wrapped.len()
    }
}

impl<'a> DoubleEndedIterator for Lines<'a> {
    fn next_back(&mut self) -> Option<Self::Item> {
        let range = self.wrapped.last()?.clone();
        self.wrapped = &self.wrapped[0..self.wrapped.len() - 1];
        Some(&self.source[range])
    }
}

#[test]
fn wrap_tests() {
    let mut wrapped = Wrapped::from("hello world");
    assert_eq!(wrapped.lines(10).collect::<Vec<_>>(), ["hello ", "world"]);
    assert_eq!(wrapped.lines(11).collect::<Vec<_>>(), ["hello world"]);
    assert_eq!(
        wrapped.lines(10).rev().collect::<Vec<_>>(),
        ["world", "hello "]
    );
}
