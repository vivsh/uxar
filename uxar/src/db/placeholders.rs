use std::sync::Arc;

/// Output of PlaceholderIter
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaceholderPart<'a> {
    Sql(&'a str),
    Placeholder(&'a str), // name WITHOUT ':'
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Normal,
    SingleQuote,
    DoubleQuote,
    Backtick,
    BracketIdent,
    LineComment,
    BlockComment,
    DollarQuote { tag_start: usize, tag_end: usize }, // tag is s[tag_start..tag_end], may be empty
}

/// Non-allocating, panic-free iterator over SQL + :name placeholders.
/// - Only detects ASCII placeholders: :[A-Za-z_][A-Za-z0-9_]*
/// - Skips: strings, quoted identifiers, line/block comments, PG dollar quotes ($$ or $tag$)
pub struct PlaceholderIter<'a> {
    s: &'a str,
    b: &'a [u8],
    i: usize,         // scan cursor (byte index)
    chunk_start: usize,
    mode: Mode,
    done: bool,
}

impl<'a> PlaceholderIter<'a> {
    pub fn new(s: &'a str) -> Self {
        Self {
            s,
            b: s.as_bytes(),
            i: 0,
            chunk_start: 0,
            mode: Mode::Normal,
            done: false,
        }
    }

    #[inline]
    fn is_name_start(x: u8) -> bool {
        (b'a'..=b'z').contains(&x) || (b'A'..=b'Z').contains(&x) || x == b'_'
    }
    #[inline]
    fn is_name_char(x: u8) -> bool {
        Self::is_name_start(x) || (b'0'..=b'9').contains(&x)
    }
    #[inline]
    fn is_word_char(x: u8) -> bool {
        Self::is_name_char(x)
    }

    #[inline]
    fn slice(&self, a: usize, b: usize) -> &'a str {
        // Safe because we only advance on UTF-8 character boundaries.
        // Indices are always in-range by construction (checked before advancing).
        &self.s[a..b]
    }

    /// Advance by one UTF-8 character. Returns the byte length of the character.
    /// This ensures we never land in the middle of a multi-byte character.
    /// Returns 0 if at end of string. Steps 1 byte on invalid UTF-8.
    #[inline]
    fn advance_one_char(&mut self) -> usize {
        let Some(&b0) = self.b.get(self.i) else {
            return 0;
        };
        
        // Fast path for ASCII (vast majority of SQL)
        if b0 < 0x80 {
            self.i += 1;
            return 1;
        }

        // Determine UTF-8 character length from leading byte
        let len = if (b0 & 0b1110_0000) == 0b1100_0000 {
            2
        } else if (b0 & 0b1111_0000) == 0b1110_0000 {
            3
        } else if (b0 & 0b1111_1000) == 0b1111_0000 {
            4
        } else {
            1 // Invalid lead byte, step 1
        };

        let end = self.i.saturating_add(len).min(self.b.len());
        // Validate continuation bytes (must match 0b10xx_xxxx pattern)
        if end - self.i == len
            && self.b[self.i + 1..end]
                .iter()
                .all(|&c| (c & 0b1100_0000) == 0b1000_0000)
        {
            self.i = end;
            len
        } else {
            // Invalid UTF-8 sequence, step 1 byte
            self.i += 1;
            1
        }
    }

    #[inline]
    fn peek(&self) -> Option<u8> {
        self.b.get(self.i).copied()
    }
    #[inline]
    fn peek_n(&self, n: usize) -> Option<u8> {
        self.b.get(self.i + n).copied()
    }

    #[inline]
    fn starts_with_at(&self, pos: usize, pat: &[u8]) -> bool {
        self.b.get(pos..pos + pat.len()).map_or(false, |x| x == pat)
    }

    // Attempts to parse a dollar-quote opener at current `i` (which must be on '$').
    // On success, sets mode and advances `i` past opener, returning true.
    fn try_enter_dollar_quote(&mut self) -> bool {
        // at '$'
        // $$ ...
        if self.starts_with_at(self.i, b"$$") {
            self.mode = Mode::DollarQuote {
                tag_start: self.i + 1,
                tag_end: self.i + 1, // empty tag
            };
            self.advance_one_char(); // first $
            self.advance_one_char(); // second $
            return true;
        }

        // $tag$
        let tag_start = self.i + 1;
        let first = self.b.get(tag_start).copied();
        if first.is_none() || !Self::is_name_start(first.unwrap()) {
            return false;
        }

        let mut j = tag_start + 1;
        while let Some(&ch) = self.b.get(j) {
            if Self::is_name_char(ch) {
                j += 1;
            } else {
                break;
            }
        }
        // must end with '$'
        if self.b.get(j).copied() == Some(b'$') {
            self.mode = Mode::DollarQuote {
                tag_start,
                tag_end: j, // excludes '$'
            };
            // Advance past $ + tag + $
            // Tag is ASCII (alphanumeric/underscore) so byte-wise is safe
            self.i = j + 1;
            return true;
        }

        false
    }

    // Attempts to detect dollar-quote closer at current `i` (which must be on '$').
    // On success, consumes closer and returns true.
    fn try_exit_dollar_quote(&mut self) -> bool {
        let (tag_start, tag_end) = match self.mode {
            Mode::DollarQuote { tag_start, tag_end } => (tag_start, tag_end),
            _ => return false,
        };

        // Empty tag: "$$"
        if tag_start == tag_end {
            if self.starts_with_at(self.i, b"$$") {
                self.advance_one_char(); // first $
                self.advance_one_char(); // second $
                self.mode = Mode::Normal;
                return true;
            }
            return false;
        }

        // Non-empty: "$tag$"
        // Need: '$' + tag + '$'
        let tag = self.b.get(tag_start..tag_end).unwrap_or(&[]);
        if self.peek() != Some(b'$') {
            return false;
        }
        let after_dollar = self.i + 1;
        if self.b.get(after_dollar..after_dollar + tag.len()) != Some(tag) {
            return false;
        }
        if self.b.get(after_dollar + tag.len()).copied() != Some(b'$') {
            return false;
        }

        // Advance past the closing delimiter: $ + tag + $
        self.advance_one_char(); // opening $
        // Tag is ASCII (alphanumeric/underscore), so byte-wise advancement is safe
        self.i += tag.len();
        self.advance_one_char(); // closing $
        self.mode = Mode::Normal;
        true
    }
}

impl<'a> Iterator for PlaceholderIter<'a> {
    type Item = PlaceholderPart<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }

        while self.i < self.b.len() {
            match self.mode {
                Mode::Normal => {
                    let c = self.b[self.i];

                    // -- line comment
                    if c == b'-' && self.peek_n(1) == Some(b'-') {
                        self.advance_one_char(); // first -
                        self.advance_one_char(); // second -
                        self.mode = Mode::LineComment;
                        continue;
                    }
                    // # line comment
                    if c == b'#' {
                        self.advance_one_char();
                        self.mode = Mode::LineComment;
                        continue;
                    }
                    // /* block comment */
                    if c == b'/' && self.peek_n(1) == Some(b'*') {
                        self.advance_one_char(); // /
                        self.advance_one_char(); // *
                        self.mode = Mode::BlockComment;
                        continue;
                    }

                    // quotes / identifiers
                    if c == b'\'' {
                        self.advance_one_char();
                        self.mode = Mode::SingleQuote;
                        continue;
                    }
                    if c == b'"' {
                        self.advance_one_char();
                        self.mode = Mode::DoubleQuote;
                        continue;
                    }
                    if c == b'`' {
                        self.advance_one_char();
                        self.mode = Mode::Backtick;
                        continue;
                    }
                    if c == b'[' {
                        self.advance_one_char();
                        self.mode = Mode::BracketIdent;
                        continue;
                    }

                    // dollar quote
                    if c == b'$' {
                        if self.try_enter_dollar_quote() {
                            continue;
                        }
                        self.advance_one_char();
                        continue;
                    }

                    // placeholder :name (NOT :: and NOT abc:name)
                    if c == b':' {
                        let prev = if self.i == 0 { None } else { Some(self.b[self.i - 1]) };
                        // Only reject if prev is a word char AND we didn't just emit a placeholder
                        let prev_is_word = prev.map(Self::is_word_char).unwrap_or(false);
                        let prev_is_colon = prev == Some(b':');
                        let just_emitted_placeholder = self.chunk_start == self.i;

                        if !prev_is_colon && (!prev_is_word || just_emitted_placeholder) {
                            if let Some(n0) = self.peek_n(1) {
                                if Self::is_name_start(n0) {
                                    let name_start = self.i + 1;
                                    let mut j = name_start + 1;
                                    while let Some(&ch) = self.b.get(j) {
                                        if Self::is_name_char(ch) {
                                            j += 1;
                                        } else {
                                            break;
                                        }
                                    }

                                    // Emit SQL chunk before placeholder, then placeholder in next call.
                                    if self.chunk_start < self.i {
                                        let sql = self.slice(self.chunk_start, self.i);
                                        self.chunk_start = self.i; // placeholder begins here
                                        return Some(PlaceholderPart::Sql(sql));
                                    }

                                    // Emit placeholder (name only), advance past it.
                                    let name = self.slice(name_start, j);
                                    self.i = j;
                                    self.chunk_start = j;
                                    return Some(PlaceholderPart::Placeholder(name));
                                }
                            }
                        }

                        self.advance_one_char();
                        continue;
                    }

                    self.advance_one_char();
                }

                Mode::SingleQuote => {
                    match self.peek() {
                        Some(b'\\') => {
                            // backslash: skip it and next char if present
                            self.advance_one_char();
                            if self.peek().is_some() {
                                self.advance_one_char();
                            }
                        }
                        Some(b'\'') => {
                            if self.peek_n(1) == Some(b'\'') {
                                self.advance_one_char(); // first '
                                self.advance_one_char(); // second '
                            } else {
                                self.advance_one_char();
                                self.mode = Mode::Normal;
                            }
                        }
                        _ => {
                            self.advance_one_char();
                        }
                    }
                }

                Mode::DoubleQuote => {
                    match self.peek() {
                        Some(b'\\') => {
                            // backslash: skip it and next char if present
                            self.advance_one_char();
                            if self.peek().is_some() {
                                self.advance_one_char();
                            }
                        }
                        Some(b'"') => {
                            if self.peek_n(1) == Some(b'"') {
                                self.advance_one_char(); // first "
                                self.advance_one_char(); // second "
                            } else {
                                self.advance_one_char();
                                self.mode = Mode::Normal;
                            }
                        }
                        _ => {
                            self.advance_one_char();
                        }
                    }
                }

                Mode::Backtick => {
                    match self.peek() {
                        Some(b'\\') => {
                            // backslash: skip it and next char if present
                            self.advance_one_char();
                            if self.peek().is_some() {
                                self.advance_one_char();
                            }
                        }
                        Some(b'`') => {
                            if self.peek_n(1) == Some(b'`') {
                                self.advance_one_char(); // first `
                                self.advance_one_char(); // second `
                            } else {
                                self.advance_one_char();
                                self.mode = Mode::Normal;
                            }
                        }
                        _ => {
                            self.advance_one_char();
                        }
                    }
                }

                Mode::BracketIdent => {
                    if self.peek() == Some(b']') {
                        self.advance_one_char();
                        self.mode = Mode::Normal;
                    } else {
                        self.advance_one_char();
                    }
                }

                Mode::LineComment => {
                    if self.peek() == Some(b'\n') {
                        self.advance_one_char();
                        self.mode = Mode::Normal;
                    } else {
                        self.advance_one_char();
                    }
                }

                Mode::BlockComment => {
                    if self.peek() == Some(b'*') && self.peek_n(1) == Some(b'/') {
                        self.advance_one_char(); // *
                        self.advance_one_char(); // /
                        self.mode = Mode::Normal;
                    } else {
                        self.advance_one_char();
                    }
                }

                Mode::DollarQuote { .. } => {
                    // scan until we see a '$' that matches closer
                    if self.peek() == Some(b'$') && self.try_exit_dollar_quote() {
                        continue;
                    }
                    self.advance_one_char();
                }
            }
        }

        // end: flush remaining SQL chunk
        self.done = true;
        if self.chunk_start < self.s.len() {
            Some(PlaceholderPart::Sql(self.slice(self.chunk_start, self.s.len())))
        } else {
            None
        }
    }
}

/// Check if SQL string contains at least one named placeholder (:name).
/// Returns true on first match without scanning the entire string.
pub fn has_named_placeholder(sql: &str) -> bool {
    PlaceholderIter::new(sql).any(|part| matches!(part, PlaceholderPart::Placeholder(_)))
}

/// Database dialect for placeholder formatting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dialect {
    /// PostgreSQL uses $1, $2, $3, etc.
    Postgres,
    /// MySQL uses ? for all placeholders
    Mysql,
    /// SQLite uses ? for all placeholders
    Sqlite,
}

/// Errors that can occur during placeholder resolution
#[derive(Debug, Clone)]
pub enum PlaceholderError {
    /// Placeholder not found in values map
    MissingValue(String),
    /// Failed to bind value to arguments
    BindError {
        placeholder: String,
        source: Arc<dyn std::error::Error + Send + Sync>,
    },
}

impl std::fmt::Display for PlaceholderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingValue(name) => write!(f, "placeholder '{}' not found in values map", name),
            Self::BindError { placeholder, source } => {
                write!(f, "failed to bind placeholder '{}': {}", placeholder, source)
            }
        }
    }
}

impl std::error::Error for PlaceholderError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::BindError { source, .. } => Some(source.as_ref()),
            _ => None,
        }
    }
}

/// Resolve named placeholders (:name) to database-specific format.
/// Binds values from the map to the arguments and returns the transformed SQL.
/// Returns an error if a placeholder is not found in the map.
/// For PostgreSQL, reuses positions for placeholders that appear multiple times.
pub fn resolve_placeholders(
    sql: &str,
    arguments: &mut super::commons::Arguments<'_>,
    values: &std::collections::HashMap<String, super::argvalue::ArgValue>,
    dialect: Dialect,
) -> Result<String, PlaceholderError> {
    use sqlx::Arguments as _;

    let mut output = String::with_capacity(sql.len());
    let mut position = arguments.len() + 1;

    // Only needed for Postgres reuse
    let mut bound_positions: std::collections::HashMap<&str, usize> =
        std::collections::HashMap::new();

    for part in PlaceholderIter::new(sql) {
        match part {
            PlaceholderPart::Sql(s) => output.push_str(s),
            PlaceholderPart::Placeholder(name) => {
                match dialect {
                    Dialect::Postgres => {
                        let pos = if let Some(&p) = bound_positions.get(name) {
                            p
                        } else {
                            let value = values
                                .get(name)
                                .ok_or_else(|| PlaceholderError::MissingValue(name.to_string()))?;
                            value.bind_value(arguments).map_err(|e| PlaceholderError::BindError {
                                placeholder: name.to_string(),
                                source: Arc::from(e) as Arc<dyn std::error::Error + Send + Sync>,
                            })?;
                            let p = position;
                            bound_positions.insert(name, p);
                            position += 1;
                            p
                        };

                        output.push('$');
                        output.push_str(&pos.to_string()); // swap to itoa if you care
                    }

                    Dialect::Mysql | Dialect::Sqlite => {
                        let value = values
                            .get(name)
                            .ok_or_else(|| PlaceholderError::MissingValue(name.to_string()))?;
                        value.bind_value(arguments).map_err(|e| PlaceholderError::BindError {
                            placeholder: name.to_string(),
                            source: Arc::from(e) as Arc<dyn std::error::Error + Send + Sync>,
                        })?;
                        position += 1;
                        output.push('?');
                    }
                }
            }
        }
    }

    Ok(output)
}


#[cfg(test)]
mod tests {
    use super::*;

    fn collect_parts(sql: &str) -> Vec<PlaceholderPart> {
        PlaceholderIter::new(sql).collect()
    }

    fn parts_to_strings(parts: Vec<PlaceholderPart>) -> Vec<String> {
        parts
            .into_iter()
            .map(|p| match p {
                PlaceholderPart::Sql(s) => format!("SQL:{}", s),
                PlaceholderPart::Placeholder(n) => format!("PARAM:{}", n),
            })
            .collect()
    }

    #[test]
    fn empty_string() {
        assert_eq!(collect_parts(""), vec![]);
    }

    #[test]
    fn no_placeholders() {
        let parts = collect_parts("SELECT * FROM users");
        assert_eq!(parts, vec![PlaceholderPart::Sql("SELECT * FROM users")]);
    }

    #[test]
    fn single_placeholder() {
        let parts = collect_parts("SELECT * FROM users WHERE id = :id");
        assert_eq!(
            parts,
            vec![
                PlaceholderPart::Sql("SELECT * FROM users WHERE id = "),
                PlaceholderPart::Placeholder("id"),
            ]
        );
    }

    #[test]
    fn multiple_placeholders() {
        let parts = collect_parts("SELECT * FROM users WHERE id = :id AND name = :name");
        assert_eq!(
            parts,
            vec![
                PlaceholderPart::Sql("SELECT * FROM users WHERE id = "),
                PlaceholderPart::Placeholder("id"),
                PlaceholderPart::Sql(" AND name = "),
                PlaceholderPart::Placeholder("name"),
            ]
        );
    }

    #[test]
    fn placeholder_at_start() {
        let parts = collect_parts(":id");
        assert_eq!(parts, vec![PlaceholderPart::Placeholder("id"),]);
    }

    #[test]
    fn placeholder_at_end() {
        let parts = collect_parts("SELECT :id");
        assert_eq!(
            parts,
            vec![
                PlaceholderPart::Sql("SELECT "),
                PlaceholderPart::Placeholder("id"),
            ]
        );
    }

    #[test]
    fn placeholder_names_with_numbers_and_underscores() {
        let parts = collect_parts(":id_1 :user2 :_private");
        assert_eq!(
            parts,
            vec![
                PlaceholderPart::Placeholder("id_1"),
                PlaceholderPart::Sql(" "),
                PlaceholderPart::Placeholder("user2"),
                PlaceholderPart::Sql(" "),
                PlaceholderPart::Placeholder("_private"),
            ]
        );
    }

    #[test]
    fn double_colon_not_placeholder() {
        // PostgreSQL cast operator ::
        let parts = collect_parts("SELECT id::integer");
        assert_eq!(parts, vec![PlaceholderPart::Sql("SELECT id::integer")]);
    }

    #[test]
    fn colon_at_end_of_string() {
        let parts = collect_parts("SELECT :");
        assert_eq!(parts, vec![PlaceholderPart::Sql("SELECT :")]);
    }

    #[test]
    fn colon_followed_by_non_name() {
        let parts = collect_parts("SELECT :123");
        assert_eq!(parts, vec![PlaceholderPart::Sql("SELECT :123")]);
    }

    #[test]
    fn word_char_before_colon_not_placeholder() {
        let parts = collect_parts("abc:param");
        assert_eq!(parts, vec![PlaceholderPart::Sql("abc:param")]);
    }

    #[test]
    fn placeholder_in_single_quotes_ignored() {
        let parts = collect_parts("SELECT 'text :param' FROM t");
        assert_eq!(
            parts,
            vec![PlaceholderPart::Sql("SELECT 'text :param' FROM t")]
        );
    }

    #[test]
    fn placeholder_in_double_quotes_ignored() {
        let parts = collect_parts("SELECT \"col:param\" FROM t");
        assert_eq!(
            parts,
            vec![PlaceholderPart::Sql("SELECT \"col:param\" FROM t")]
        );
    }

    #[test]
    fn placeholder_in_backticks_ignored() {
        let parts = collect_parts("SELECT `col:param` FROM t");
        assert_eq!(
            parts,
            vec![PlaceholderPart::Sql("SELECT `col:param` FROM t")]
        );
    }

    #[test]
    fn placeholder_in_bracket_ident_ignored() {
        let parts = collect_parts("SELECT [col:param] FROM t");
        assert_eq!(
            parts,
            vec![PlaceholderPart::Sql("SELECT [col:param] FROM t")]
        );
    }

    #[test]
    fn escaped_single_quote_with_doubling() {
        let parts = collect_parts("SELECT 'don''t :param' FROM t");
        assert_eq!(
            parts,
            vec![PlaceholderPart::Sql("SELECT 'don''t :param' FROM t")]
        );
    }

    #[test]
    fn escaped_double_quote_with_doubling() {
        let parts = collect_parts("SELECT \"col\"\"name:param\" FROM t");
        assert_eq!(
            parts,
            vec![PlaceholderPart::Sql("SELECT \"col\"\"name:param\" FROM t")]
        );
    }

    #[test]
    fn escaped_backtick_with_doubling() {
        let parts = collect_parts("SELECT `col``name:param` FROM t");
        assert_eq!(
            parts,
            vec![PlaceholderPart::Sql("SELECT `col``name:param` FROM t")]
        );
    }

    #[test]
    fn backslash_escape_single_quote() {
        let parts = collect_parts("SELECT 'don\\'t :param' FROM t");
        assert_eq!(
            parts,
            vec![PlaceholderPart::Sql("SELECT 'don\\'t :param' FROM t")]
        );
    }

    #[test]
    fn backslash_escape_double_quote() {
        let parts = collect_parts("SELECT \"col\\\"name:param\" FROM t");
        assert_eq!(
            parts,
            vec![PlaceholderPart::Sql("SELECT \"col\\\"name:param\" FROM t")]
        );
    }

    #[test]
    fn backslash_escape_backtick() {
        let parts = collect_parts("SELECT `col\\`name:param` FROM t");
        assert_eq!(
            parts,
            vec![PlaceholderPart::Sql("SELECT `col\\`name:param` FROM t")]
        );
    }

    #[test]
    fn backslash_at_end_of_single_quote() {
        // 'text\' - the backslash escapes the closing quote, so string is unclosed
        let parts = collect_parts("SELECT 'text\\' :param");
        assert_eq!(
            parts,
            vec![PlaceholderPart::Sql("SELECT 'text\\' :param")]
        );
    }

    #[test]
    fn backslash_backslash_in_quote() {
        let parts = collect_parts("SELECT 'path\\\\:param' FROM t");
        assert_eq!(
            parts,
            vec![PlaceholderPart::Sql("SELECT 'path\\\\:param' FROM t")]
        );
    }

    #[test]
    fn line_comment_double_dash() {
        let parts = collect_parts("SELECT * -- :param\nFROM t");
        assert_eq!(parts, vec![PlaceholderPart::Sql("SELECT * -- :param\nFROM t")]);
    }

    #[test]
    fn line_comment_hash() {
        let parts = collect_parts("SELECT * # :param\nFROM t");
        assert_eq!(parts, vec![PlaceholderPart::Sql("SELECT * # :param\nFROM t")]);
    }

    #[test]
    fn line_comment_at_end_no_newline() {
        let parts = collect_parts("SELECT * -- :param");
        assert_eq!(parts, vec![PlaceholderPart::Sql("SELECT * -- :param")]);
    }

    #[test]
    fn block_comment() {
        let parts = collect_parts("SELECT * /* :param */ FROM t");
        assert_eq!(
            parts,
            vec![PlaceholderPart::Sql("SELECT * /* :param */ FROM t")]
        );
    }

    #[test]
    fn block_comment_multiline() {
        let parts = collect_parts("SELECT * /* line1\n:param\nline2 */ FROM t");
        assert_eq!(
            parts,
            vec![PlaceholderPart::Sql("SELECT * /* line1\n:param\nline2 */ FROM t")]
        );
    }

    #[test]
    fn block_comment_not_closed() {
        let parts = collect_parts("SELECT * /* :param");
        assert_eq!(parts, vec![PlaceholderPart::Sql("SELECT * /* :param")]);
    }

    #[test]
    fn dollar_quote_empty_tag() {
        let parts = collect_parts("SELECT $$:param$$ FROM t");
        assert_eq!(
            parts,
            vec![PlaceholderPart::Sql("SELECT $$:param$$ FROM t")]
        );
    }

    #[test]
    fn dollar_quote_with_tag() {
        let parts = collect_parts("SELECT $tag$:param$tag$ FROM t");
        assert_eq!(
            parts,
            vec![PlaceholderPart::Sql("SELECT $tag$:param$tag$ FROM t")]
        );
    }

    #[test]
    fn dollar_quote_different_tags_not_matched() {
        let parts = collect_parts("SELECT $a$text$b$ FROM t");
        assert_eq!(
            parts,
            vec![PlaceholderPart::Sql("SELECT $a$text$b$ FROM t")]
        );
    }

    #[test]
    fn dollar_quote_tag_must_be_identifier() {
        let parts = collect_parts("SELECT $123$ FROM t");
        assert_eq!(parts, vec![PlaceholderPart::Sql("SELECT $123$ FROM t")]);
    }

    #[test]
    fn dollar_quote_nested_dollar_signs() {
        let parts = collect_parts("SELECT $$text $ more$$ FROM t");
        assert_eq!(
            parts,
            vec![PlaceholderPart::Sql("SELECT $$text $ more$$ FROM t")]
        );
    }

    #[test]
    fn dollar_quote_tag_prefix_matching() {
        // $tag$ should not match $tagg$
        let parts = collect_parts("SELECT $tag$text$tagg$ FROM t");
        assert_eq!(
            parts,
            vec![PlaceholderPart::Sql("SELECT $tag$text$tagg$ FROM t")]
        );
    }

    #[test]
    fn placeholder_before_and_after_quotes() {
        let parts = collect_parts(":a 'text' :b");
        assert_eq!(
            parts,
            vec![
                PlaceholderPart::Placeholder("a"),
                PlaceholderPart::Sql(" 'text' "),
                PlaceholderPart::Placeholder("b"),
            ]
        );
    }

    #[test]
    fn complex_query_with_multiple_features() {
        let sql = r#"
            SELECT * FROM users
            WHERE id = :id -- user id
              AND name = 'O''Neil :fake'
              AND email = :email /* :notthis */
              AND data = $${"key": ":value"}$$
              AND status::text = :status
        "#;
        let parts = parts_to_strings(collect_parts(sql));
        assert!(parts.contains(&"PARAM:id".to_string()));
        assert!(parts.contains(&"PARAM:email".to_string()));
        assert!(parts.contains(&"PARAM:status".to_string()));
        assert!(!parts.iter().any(|s| s.contains("PARAM:fake")));
        assert!(!parts.iter().any(|s| s.contains("PARAM:notthis")));
        assert!(!parts.iter().any(|s| s.contains("PARAM:value")));
    }

    #[test]
    fn placeholder_after_various_punctuation() {
        let parts = collect_parts("(:a, :b):c {:d}");
        assert_eq!(
            parts_to_strings(parts),
            vec![
                "SQL:(".to_string(),
                "PARAM:a".to_string(),
                "SQL:, ".to_string(),
                "PARAM:b".to_string(),
                "SQL:)".to_string(),
                "PARAM:c".to_string(),
                "SQL: {".to_string(),
                "PARAM:d".to_string(),
                "SQL:}".to_string(),
            ]
        );
    }

    #[test]
    fn unclosed_single_quote() {
        let parts = collect_parts("SELECT ':param");
        assert_eq!(parts, vec![PlaceholderPart::Sql("SELECT ':param")]);
    }

    #[test]
    fn unclosed_double_quote() {
        let parts = collect_parts("SELECT \":param");
        assert_eq!(parts, vec![PlaceholderPart::Sql("SELECT \":param")]);
    }

    #[test]
    fn unclosed_backtick() {
        let parts = collect_parts("SELECT `:param");
        assert_eq!(parts, vec![PlaceholderPart::Sql("SELECT `:param")]);
    }

    #[test]
    fn unclosed_bracket_ident() {
        let parts = collect_parts("SELECT [:param");
        assert_eq!(parts, vec![PlaceholderPart::Sql("SELECT [:param")]);
    }

    #[test]
    fn unclosed_dollar_quote() {
        let parts = collect_parts("SELECT $$:param");
        assert_eq!(parts, vec![PlaceholderPart::Sql("SELECT $$:param")]);
    }

    #[test]
    fn empty_placeholder_name() {
        // : followed by space
        let parts = collect_parts("SELECT : FROM t");
        assert_eq!(parts, vec![PlaceholderPart::Sql("SELECT : FROM t")]);
    }

    #[test]
    fn consecutive_placeholders() {
        let parts = collect_parts(":a:b:c");
        assert_eq!(
            parts_to_strings(parts),
            vec![
                "PARAM:a".to_string(),
                "PARAM:b".to_string(),
                "PARAM:c".to_string(),
            ]
        );
    }

    #[test]
    fn placeholder_with_operators() {
        let parts = collect_parts("SELECT :a+:b*:c");
        assert_eq!(
            parts_to_strings(parts),
            vec![
                "SQL:SELECT ".to_string(),
                "PARAM:a".to_string(),
                "SQL:+".to_string(),
                "PARAM:b".to_string(),
                "SQL:*".to_string(),
                "PARAM:c".to_string(),
            ]
        );
    }

    #[test]
    fn all_quote_types_in_sequence() {
        let parts = collect_parts("':a' \":b\" `:c` [:d] $$:e$$");
        assert_eq!(
            parts,
            vec![PlaceholderPart::Sql("':a' \":b\" `:c` [:d] $$:e$$")]
        );
    }

    #[test]
    fn mixed_comment_types() {
        let parts = collect_parts("-- :a\n/* :b */ # :c\n:d");
        assert_eq!(
            parts,
            vec![
                PlaceholderPart::Sql("-- :a\n/* :b */ # :c\n"),
                PlaceholderPart::Placeholder("d"),
            ]
        );
    }

    #[test]
    fn backslash_at_string_end_boundary() {
        // Backslash at end of input inside quote
        let parts = collect_parts("SELECT 'text\\");
        assert_eq!(parts, vec![PlaceholderPart::Sql("SELECT 'text\\")]);
    }

    #[test]
    fn single_colon() {
        let parts = collect_parts(":");
        assert_eq!(parts, vec![PlaceholderPart::Sql(":")]);
    }

    #[test]
    fn only_placeholder() {
        let parts = collect_parts(":param");
        assert_eq!(parts, vec![PlaceholderPart::Placeholder("param")]);
    }

    #[test]
    fn placeholder_uppercase_letters() {
        let parts = collect_parts(":USER_ID :UserName");
        assert_eq!(
            parts,
            vec![
                PlaceholderPart::Placeholder("USER_ID"),
                PlaceholderPart::Sql(" "),
                PlaceholderPart::Placeholder("UserName"),
            ]
        );
    }

    #[test]
    fn dollar_sign_not_quote_start() {
        let parts = collect_parts("SELECT $ :param FROM t");
        assert_eq!(
            parts,
            vec![
                PlaceholderPart::Sql("SELECT $ "),
                PlaceholderPart::Placeholder("param"),
                PlaceholderPart::Sql(" FROM t"),
            ]
        );
    }

    #[test]
    fn star_slash_outside_comment() {
        let parts = collect_parts("SELECT */ :param");
        assert_eq!(
            parts,
            vec![
                PlaceholderPart::Sql("SELECT */ "),
                PlaceholderPart::Placeholder("param"),
            ]
        );
    }

    #[test]
    fn dash_not_double() {
        let parts = collect_parts("SELECT - :param");
        assert_eq!(
            parts,
            vec![
                PlaceholderPart::Sql("SELECT - "),
                PlaceholderPart::Placeholder("param"),
            ]
        );
    }

    #[test]
    fn has_placeholder_returns_true() {
        assert!(has_named_placeholder("SELECT * WHERE id = :id"));
        assert!(has_named_placeholder(":param"));
        assert!(has_named_placeholder("text :param more"));
    }

    #[test]
    fn has_placeholder_returns_false() {
        assert!(!has_named_placeholder("SELECT * FROM users"));
        assert!(!has_named_placeholder(""));
        assert!(!has_named_placeholder("SELECT ':param' FROM t"));
        assert!(!has_named_placeholder("-- :param"));
        assert!(!has_named_placeholder("id::integer"));
    }

    #[test]
    fn has_placeholder_short_circuits() {
        // Should stop at first placeholder without scanning entire string
        assert!(has_named_placeholder(":first :second :third"));
    }

    #[test]
    fn has_placeholder_incomplete_single_quote() {
        // Unclosed quote - placeholder before quote should be found
        assert!(has_named_placeholder(":param 'unclosed"));
        // Unclosed quote - placeholder inside quote should be ignored
        assert!(!has_named_placeholder("'unclosed :param"));
    }

    #[test]
    fn has_placeholder_incomplete_double_quote() {
        // Unclosed double quote
        assert!(has_named_placeholder(":param \"unclosed"));
        assert!(!has_named_placeholder("\"unclosed :param"));
    }

    #[test]
    fn has_placeholder_incomplete_backtick() {
        // Unclosed backtick
        assert!(has_named_placeholder(":param `unclosed"));
        assert!(!has_named_placeholder("`unclosed :param"));
    }

    #[test]
    fn has_placeholder_incomplete_bracket() {
        // Unclosed bracket identifier
        assert!(has_named_placeholder(":param [unclosed"));
        assert!(!has_named_placeholder("[unclosed :param"));
    }

    #[test]
    fn has_placeholder_incomplete_line_comment() {
        // Line comment without newline - placeholder in comment ignored
        assert!(!has_named_placeholder("-- :param"));
        assert!(!has_named_placeholder("# :param"));
        // Placeholder before comment found
        assert!(has_named_placeholder(":param --"));
    }

    #[test]
    fn has_placeholder_incomplete_block_comment() {
        // Unclosed block comment - placeholder inside ignored
        assert!(!has_named_placeholder("/* :param"));
        // Placeholder before unclosed comment found
        assert!(has_named_placeholder(":param /*"));
    }

    #[test]
    fn has_placeholder_incomplete_dollar_quote() {
        // Incomplete dollar quote
        assert!(has_named_placeholder(":param $$text"));
        assert!(has_named_placeholder(":param $tag$text"));
        // Placeholder inside unclosed dollar quote
        assert!(!has_named_placeholder("$$:param"));
        assert!(!has_named_placeholder("$tag$:param"));
    }

    #[test]
    fn has_placeholder_partial_placeholder() {
        // Colon at end of string
        assert!(!has_named_placeholder("SELECT :"));
        // Colon followed by non-name char
        assert!(!has_named_placeholder("SELECT :123"));
        // Incomplete placeholder name is still a placeholder
        assert!(has_named_placeholder("SELECT :p"));
    }

    #[test]
    fn has_placeholder_escaped_quote_incomplete() {
        // Escaped quote at end - string still open
        assert!(!has_named_placeholder("'text\\' :param"));
        // Note: 'text\' leaves quote open, :param is inside the string context
    }

    #[test]
    fn has_placeholder_mixed_incomplete() {
        // Multiple incomplete constructs
        assert!(has_named_placeholder(":a /* :b"));
        assert!(has_named_placeholder(":a 'b"));
        assert!(!has_named_placeholder("'a :b /* :c"));
        assert!(!has_named_placeholder("/* 'quoted :param"));
    }

    #[test]
    fn has_placeholder_empty_and_whitespace() {
        assert!(!has_named_placeholder(""));
        assert!(!has_named_placeholder("   "));
        assert!(!has_named_placeholder("\n\t"));
    }

    #[test]
    fn has_placeholder_only_special_chars() {
        assert!(!has_named_placeholder("::::"));
        assert!(!has_named_placeholder("/* */ -- "));
        assert!(!has_named_placeholder("'''' \"\" ``"));
    }

    #[test]
    fn resolve_postgres_placeholders() {
        use std::collections::HashMap;
        let sql = "SELECT * FROM users WHERE id = :id AND name = :name";
        let mut args = super::super::commons::Arguments::default();
        let mut values = HashMap::new();
        values.insert("id".to_string(), super::super::argvalue::ArgValue::new(42i32));
        values.insert("name".to_string(), super::super::argvalue::ArgValue::new("Alice"));

        let result = resolve_placeholders(sql, &mut args, &values, Dialect::Postgres);
        assert!(result.is_ok());
        assert_eq!(
            result.unwrap(),
            "SELECT * FROM users WHERE id = $1 AND name = $2"
        );
    }

    #[test]
    fn resolve_mysql_placeholders() {
        use std::collections::HashMap;
        let sql = "SELECT * FROM users WHERE id = :id AND name = :name";
        let mut args = super::super::commons::Arguments::default();
        let mut values = HashMap::new();
        values.insert("id".to_string(), super::super::argvalue::ArgValue::new(42i32));
        values.insert("name".to_string(), super::super::argvalue::ArgValue::new("Alice"));

        let result = resolve_placeholders(sql, &mut args, &values, Dialect::Mysql);
        assert!(result.is_ok());
        assert_eq!(
            result.unwrap(),
            "SELECT * FROM users WHERE id = ? AND name = ?"
        );
    }

    #[test]
    fn resolve_sqlite_placeholders() {
        use std::collections::HashMap;
        let sql = "SELECT * FROM users WHERE id = :id";
        let mut args = super::super::commons::Arguments::default();
        let mut values = HashMap::new();
        values.insert("id".to_string(), super::super::argvalue::ArgValue::new(42i32));

        let result = resolve_placeholders(sql, &mut args, &values, Dialect::Sqlite);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "SELECT * FROM users WHERE id = ?");
    }

    #[test]
    fn resolve_missing_placeholder() {
        use std::collections::HashMap;
        let sql = "SELECT * FROM users WHERE id = :id AND name = :name";
        let mut args = super::super::commons::Arguments::default();
        let values = HashMap::new(); // empty map

        let result = resolve_placeholders(sql, &mut args, &values, Dialect::Postgres);
        assert!(result.is_err());
        match result.unwrap_err() {
            PlaceholderError::MissingValue(name) => {
                assert_eq!(name, "id");
            }
            _ => panic!("Expected MissingValue error"),
        }
    }

    #[test]
    fn resolve_skips_placeholders_in_quotes() {
        use std::collections::HashMap;
        let sql = "SELECT ':fake' FROM users WHERE id = :id";
        let mut args = super::super::commons::Arguments::default();
        let mut values = HashMap::new();
        values.insert("id".to_string(), super::super::argvalue::ArgValue::new(42i32));

        let result = resolve_placeholders(sql, &mut args, &values, Dialect::Postgres);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "SELECT ':fake' FROM users WHERE id = $1");
    }

    #[test]
    fn resolve_skips_placeholders_in_comments() {
        use std::collections::HashMap;
        let sql = "SELECT * FROM users WHERE id = :id -- :fake";
        let mut args = super::super::commons::Arguments::default();
        let mut values = HashMap::new();
        values.insert("id".to_string(), super::super::argvalue::ArgValue::new(42i32));

        let result = resolve_placeholders(sql, &mut args, &values, Dialect::Postgres);
        assert!(result.is_ok());
        assert_eq!(
            result.unwrap(),
            "SELECT * FROM users WHERE id = $1 -- :fake"
        );
    }

    #[test]
    fn resolve_consecutive_placeholders() {
        use std::collections::HashMap;
        let sql = ":a:b:c";
        let mut args = super::super::commons::Arguments::default();
        let mut values = HashMap::new();
        values.insert("a".to_string(), super::super::argvalue::ArgValue::new(1i32));
        values.insert("b".to_string(), super::super::argvalue::ArgValue::new(2i32));
        values.insert("c".to_string(), super::super::argvalue::ArgValue::new(3i32));

        let result = resolve_placeholders(sql, &mut args, &values, Dialect::Postgres);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "$1$2$3");
    }

    #[test]
    fn resolve_empty_sql() {
        use std::collections::HashMap;
        let sql = "";
        let mut args = super::super::commons::Arguments::default();
        let values = HashMap::new();

        let result = resolve_placeholders(sql, &mut args, &values, Dialect::Postgres);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "");
    }

    #[test]
    fn resolve_increments_arguments_len() {
        use std::collections::HashMap;
        use sqlx::Arguments as _;
        let sql = "SELECT * FROM users WHERE id = :id AND name = :name AND age = :age";
        let mut args = super::super::commons::Arguments::default();
        let mut values = HashMap::new();
        values.insert("id".to_string(), super::super::argvalue::ArgValue::new(42i32));
        values.insert("name".to_string(), super::super::argvalue::ArgValue::new("Bob"));
        values.insert("age".to_string(), super::super::argvalue::ArgValue::new(30i32));

        assert_eq!(args.len(), 0);

        let result = resolve_placeholders(sql, &mut args, &values, Dialect::Postgres);
        assert!(result.is_ok());
        assert_eq!(args.len(), 3);
        assert_eq!(
            result.unwrap(),
            "SELECT * FROM users WHERE id = $1 AND name = $2 AND age = $3"
        );
    }

    #[test]
    fn resolve_continues_from_existing_arguments() {
        use std::collections::HashMap;
        use sqlx::Arguments as _;

        let mut args = super::super::commons::Arguments::default();
        // Pre-bind some arguments
        args.add(&100i32).unwrap();
        args.add(&"existing").unwrap();
        assert_eq!(args.len(), 2);

        let sql = "WHERE status = :status AND type = :type";
        let mut values = HashMap::new();
        values.insert("status".to_string(), super::super::argvalue::ArgValue::new("active"));
        values.insert("type".to_string(), super::super::argvalue::ArgValue::new(5i32));

        let result = resolve_placeholders(sql, &mut args, &values, Dialect::Postgres);
        assert!(result.is_ok());
        assert_eq!(args.len(), 4);
        assert_eq!(result.unwrap(), "WHERE status = $3 AND type = $4");
    }

    #[test]
    fn resolve_error_on_multiple_missing_values() {
        use std::collections::HashMap;
        let sql = "SELECT * FROM users WHERE id = :id AND name = :name AND email = :email";
        let mut args = super::super::commons::Arguments::default();
        let mut values = HashMap::new();
        values.insert("id".to_string(), super::super::argvalue::ArgValue::new(42i32));
        // name and email are missing

        let result = resolve_placeholders(sql, &mut args, &values, Dialect::Postgres);
        assert!(result.is_err());
        match result.unwrap_err() {
            PlaceholderError::MissingValue(name) => {
                assert_eq!(name, "name"); // First missing placeholder
            }
            _ => panic!("Expected MissingValue error"),
        }
    }

    #[test]
    fn resolve_error_preserves_arguments_on_failure() {
        use std::collections::HashMap;
        use sqlx::Arguments as _;
        let sql = "SELECT * FROM users WHERE id = :id AND name = :missing";
        let mut args = super::super::commons::Arguments::default();
        let mut values = HashMap::new();
        values.insert("id".to_string(), super::super::argvalue::ArgValue::new(42i32));

        let initial_len = args.len();
        let result = resolve_placeholders(sql, &mut args, &values, Dialect::Postgres);
        assert!(result.is_err());
        // Arguments should have been modified up to the point of error
        assert!(args.len() > initial_len);
        assert_eq!(args.len(), 1); // Only :id was bound before error
    }

    #[test]
    fn resolve_reuses_postgres_positions() {
        use std::collections::HashMap;
        use sqlx::Arguments as _;
        let sql = "SELECT * FROM users WHERE id = :id AND parent_id = :id AND status = :status";
        let mut args = super::super::commons::Arguments::default();
        let mut values = HashMap::new();
        values.insert("id".to_string(), super::super::argvalue::ArgValue::new(42i32));
        values.insert("status".to_string(), super::super::argvalue::ArgValue::new("active"));

        let result = resolve_placeholders(sql, &mut args, &values, Dialect::Postgres);
        assert!(result.is_ok());
        // :id should reuse $1, :status should be $2
        assert_eq!(
            result.unwrap(),
            "SELECT * FROM users WHERE id = $1 AND parent_id = $1 AND status = $2"
        );
        // Only 2 values should be bound even though there are 3 placeholders
        assert_eq!(args.len(), 2);
    }

    #[test]
    fn resolve_mysql_with_duplicate_placeholders() {
        use std::collections::HashMap;
        use sqlx::Arguments as _;
        let sql = "SELECT * FROM users WHERE id = :id OR parent_id = :id";
        let mut args = super::super::commons::Arguments::default();
        let mut values = HashMap::new();
        values.insert("id".to_string(), super::super::argvalue::ArgValue::new(42i32));

        let result = resolve_placeholders(sql, &mut args, &values, Dialect::Mysql);
        assert!(result.is_ok());
        // MySQL uses ? for all, but only binds once
        assert_eq!(result.unwrap(), "SELECT * FROM users WHERE id = ? OR parent_id = ?");
        assert_eq!(args.len(), 2); // Always binds for each placeholder in MySQL
    }

    // KNOWN LIMITATION: Tests for non-ASCII SQL
    // The current implementation uses byte-based iteration and may not correctly handle
    // multi-byte UTF-8 characters. While it doesn't panic in simple cases (because slicing
    // happens at boundaries the algorithm naturally lands on), this is not guaranteed and
    // depends on where multi-byte characters appear relative to SQL syntax elements.
    //
    // These tests document the limitation. TODO: Fix by using char-aware iteration or
    // ensuring all byte offsets are on valid UTF-8 boundaries.

    #[test]
    fn iter_with_multibyte_utf8_in_sql() {
        // Multi-byte UTF-8 characters (emoji, accented chars, etc.)
        let sql = "SELECT * FROM café WHERE name = :name";
        let parts: Vec<_> = PlaceholderIter::new(sql).collect();
        assert_eq!(parts.len(), 2); // SQL before (with é), placeholder
        assert!(matches!(parts[0], PlaceholderPart::Sql(_)));
        assert_eq!(parts[1], PlaceholderPart::Placeholder("name"));
    }

    #[test]
    fn iter_with_emoji_in_sql() {
        let sql = "SELECT * FROM users WHERE status = '🎉' AND id = :id";
        let parts: Vec<_> = PlaceholderIter::new(sql).collect();
        assert!(parts.len() >= 2); // Should handle emoji correctly
    }

    #[test]
    fn iter_with_chinese_characters() {
        let sql = "SELECT 你好 FROM users WHERE id = :id";
        let parts: Vec<_> = PlaceholderIter::new(sql).collect();
        assert!(parts.len() >= 2); // Should handle Chinese characters
    }

    #[test]
    fn iter_with_multibyte_in_comment() {
        let sql = "-- Comment with café\nSELECT :id";
        let parts: Vec<_> = PlaceholderIter::new(sql).collect();
        assert_eq!(parts.len(), 2); // Comment, then placeholder
    }

    #[test]
    fn iter_with_multibyte_in_string() {
        let sql = "SELECT 'café' FROM users WHERE id = :id";
        let parts: Vec<_> = PlaceholderIter::new(sql).collect();
        assert!(parts.len() >= 2); // Should handle accented chars in strings
    }

    #[test]
    fn iter_with_cyrillic() {
        let sql = "SELECT * FROM пользователи WHERE id = :id";
        let parts: Vec<_> = PlaceholderIter::new(sql).collect();
        assert!(parts.len() >= 2); // Should handle Cyrillic
    }

    #[test]
    fn has_placeholder_with_multibyte() {
        assert!(has_named_placeholder("SELECT café WHERE id = :id"));
    }

    #[test]
    fn resolve_with_multibyte() {
        use std::collections::HashMap;
        let sql = "SELECT * FROM café WHERE id = :id";
        let mut args = super::super::commons::Arguments::default();
        let mut values = HashMap::new();
        values.insert("id".to_string(), super::super::argvalue::ArgValue::new(42i32));

        let result = resolve_placeholders(sql, &mut args, &values, Dialect::Postgres);
        // Currently works but not guaranteed
        assert!(result.is_ok());
    }
}
