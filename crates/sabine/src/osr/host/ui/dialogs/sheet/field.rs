use std::ops::Range;

/// The single line of text a prompt asks for, with its caret and selection
/// as byte offsets, and what the input method composes at the caret.
#[derive(Debug, Default)]
pub(super) struct TextField {
    pub(super) text: String,
    pub(super) caret: usize,
    pub(super) anchor: usize,
    pub(super) preedit: String,
}

impl TextField {
    /// A field holding `text`, all of it selected, as browsers open prompts.
    pub(super) fn new(text: String) -> Self {
        let text = single_line(&text);
        Self {
            caret: text.len(),
            anchor: 0,
            text,
            preedit: String::new(),
        }
    }

    pub(super) fn selection(&self) -> Range<usize> {
        self.caret.min(self.anchor)..self.caret.max(self.anchor)
    }

    pub(super) fn selected_text(&self) -> &str {
        &self.text[self.selection()]
    }

    /// Replaces the selection with `text`, as typing or pasting does.
    pub(super) fn insert(&mut self, text: &str) {
        let text = single_line(text);
        let selection = self.selection();
        self.text.replace_range(selection.clone(), &text);
        self.caret = selection.start + text.len();
        self.anchor = self.caret;
    }

    pub(super) fn delete_backward(&mut self, word: bool) {
        if self.anchor == self.caret {
            self.anchor = if word {
                self.word_start(self.caret)
            } else {
                self.previous(self.caret)
            };
        }
        self.insert("");
    }

    pub(super) fn delete_forward(&mut self, word: bool) {
        if self.anchor == self.caret {
            self.anchor = if word {
                self.word_end(self.caret)
            } else {
                self.next(self.caret)
            };
        }
        self.insert("");
    }

    /// Moves the caret a character or word, or to an end of a selection
    /// that `extend` doesn't grow.
    pub(super) fn step(&mut self, forward: bool, word: bool, extend: bool) {
        let selection = self.selection();
        self.caret = match (forward, word) {
            _ if !extend && !selection.is_empty() => {
                if forward {
                    selection.end
                } else {
                    selection.start
                }
            }
            (true, true) => self.word_end(self.caret),
            (true, false) => self.next(self.caret),
            (false, true) => self.word_start(self.caret),
            (false, false) => self.previous(self.caret),
        };
        if !extend {
            self.anchor = self.caret;
        }
    }

    pub(super) fn move_to(&mut self, offset: usize, extend: bool) {
        self.caret = offset.min(self.text.len());
        if !extend {
            self.anchor = self.caret;
        }
    }

    pub(super) fn select_all(&mut self) {
        self.anchor = 0;
        self.caret = self.text.len();
    }

    /// The text as drawn: what the input method composes sits at the caret.
    pub(super) fn shown(&self) -> String {
        let mut shown = self.text.clone();
        shown.insert_str(self.caret, &self.preedit);
        shown
    }

    fn previous(&self, offset: usize) -> usize {
        self.text[..offset]
            .char_indices()
            .next_back()
            .map_or(0, |(index, _)| index)
    }

    fn next(&self, offset: usize) -> usize {
        self.text[offset..]
            .chars()
            .next()
            .map_or(offset, |character| offset + character.len_utf8())
    }

    fn word_start(&self, offset: usize) -> usize {
        let before = self.text[..offset].trim_end_matches(char::is_whitespace);
        before.rfind(char::is_whitespace).map_or(0, |space| {
            space + before[space..].chars().next().map_or(1, char::len_utf8)
        })
    }

    fn word_end(&self, offset: usize) -> usize {
        let after = &self.text[offset..];
        let word = after.len() - after.trim_start_matches(char::is_whitespace).len();
        after[word..]
            .find(char::is_whitespace)
            .map_or(self.text.len(), |space| offset + word + space)
    }
}

/// Text from a paste or the page, with its line breaks turned into spaces.
fn single_line(text: &str) -> String {
    text.chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::TextField;

    #[test]
    fn typing_replaces_the_selection() {
        let mut field = TextField::new("Untitled".into());
        field.insert("Notes");
        assert_eq!(field.text, "Notes");
        field.step(false, false, true);
        field.step(false, false, true);
        field.insert("ar");
        assert_eq!(field.text, "Notar");
    }

    #[test]
    fn words_end_at_spaces() {
        let mut field = TextField::new("plan the trip".into());
        field.move_to(field.text.len(), false);
        field.delete_backward(true);
        assert_eq!(field.text, "plan the ");
        field.move_to(0, false);
        field.step(true, true, false);
        assert_eq!(field.caret, 4);
    }

    #[test]
    fn carets_stay_on_character_boundaries() {
        let mut field = TextField::new("añ🦆".into());
        field.move_to(field.text.len(), false);
        field.delete_backward(false);
        field.delete_backward(false);
        assert_eq!(field.text, "a");
    }

    #[test]
    fn pasted_lines_join_into_one() {
        let mut field = TextField::default();
        field.insert("one\ntwo");
        assert_eq!(field.text, "one two");
    }
}
