use std::fmt::Debug;
use std::iter::Peekable;
use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum LexError {
    #[error("Unexpected EOF at {0}")]
    UnexpectedEof(usize),
    #[error("Unexpected Caret at {0}")]
    UnexpectedCaret(usize),
    #[error("Unexpected dollar at {0}")]
    UnexpectedDollar(usize),
    #[error("Unsupported token {token:?} at {pos}")]
    UnsupportedToken { token: Token, pos: usize },
    #[error("Unbalance parens")]
    UnbalancedParens,

    #[error("Range out of order in character class at {0}.")]
    IncorrectCharClassRangeOrder(usize),
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, PartialOrd, Ord)]
pub struct CharRange {
    pub start: char,
    pub end: char,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
struct ClassAtom {
    ch: char,
    escaped: bool,
}

#[derive(Debug, Default, PartialEq, Clone)]
pub struct CharClass {
    pub ranges: Vec<CharRange>,
    pub negated: bool,
}

impl CharClass {
    pub fn matches(&self, ch: char) -> bool {
        // O(N) for now, maybe O(log N) later.
        for range in &self.ranges {
            if range.start > ch {
                continue;
            }

            if range.start <= ch && range.end >= ch {
                return !self.negated;
            }

            if range.start > ch {
                break;
            }
        }

        self.negated
    }

    fn negated(mut self) -> Self {
        self.negated = !self.negated;
        self
    }

    fn parse(chars: &[ClassAtom], offset: usize) -> Result<Self, LexError> {
        let mut char_class = Self::default();
        let mut i = 0;

        if chars.first()
            == Some(&ClassAtom {
                ch: '^',
                escaped: false,
            })
        {
            char_class.negated = true;
            i = 1;
        }

        while i < chars.len() {
            let chr = chars[i];
            let next = chars.get(i + 1);
            let next2 = chars.get(i + 2);

            match (chr, next, next2) {
                (
                    start,
                    Some(ClassAtom {
                        ch: '-',
                        escaped: false,
                    }),
                    Some(end),
                ) => {
                    if start.ch > end.ch {
                        return Err(LexError::IncorrectCharClassRangeOrder(
                            offset + i,
                        ));
                    }

                    char_class.ranges.push(CharRange {
                        start: start.ch,
                        end: end.ch,
                    });
                    i += 3;
                }
                _ => {
                    char_class.ranges.push(CharRange {
                        start: chr.ch,
                        end: chr.ch,
                    });

                    i += 1;
                }
            }
        }

        char_class.normalize();

        Ok(char_class)
    }

    fn normalize(&mut self) {
        if self.ranges.is_empty() {
            return;
        }

        self.ranges.sort_unstable();

        let mut merged: Vec<CharRange> = Vec::with_capacity(self.ranges.len());

        for item in &self.ranges {
            if merged
                .last()
                .is_none_or(|last| next_char(last.end) < item.start)
            {
                merged.push(*item);
            } else {
                let last = merged.last_mut().unwrap();
                last.end = last.end.max(item.end)
            }
        }

        self.ranges = merged;
    }

    fn digits() -> Self {
        CharClass {
            ranges: vec![CharRange {
                start: '0',
                end: '9',
            }],
            negated: false,
        }
    }

    fn word() -> Self {
        CharClass {
            ranges: vec![
                CharRange {
                    start: '0',
                    end: '9',
                },
                CharRange {
                    start: 'A',
                    end: 'Z',
                },
                CharRange {
                    start: '_',
                    end: '_',
                },
                CharRange {
                    start: 'a',
                    end: 'z',
                },
            ],
            negated: false,
        }
    }

    fn space() -> Self {
        CharClass {
            ranges: vec![
                CharRange {
                    start: '\t',
                    end: '\t',
                },
                CharRange {
                    start: '\n',
                    end: '\n',
                },
                CharRange {
                    start: '\u{000B}',
                    end: '\u{000B}',
                },
                CharRange {
                    start: '\u{000C}',
                    end: '\u{000C}',
                },
                CharRange {
                    start: '\r',
                    end: '\r',
                },
                CharRange {
                    start: ' ',
                    end: ' ',
                },
                CharRange {
                    start: '\u{00A0}',
                    end: '\u{00A0}',
                },
                CharRange {
                    start: '\u{2028}',
                    end: '\u{2028}',
                },
                CharRange {
                    start: '\u{2029}',
                    end: '\u{2029}',
                },
            ],
            negated: false,
        }
    }

    fn line_terminators() -> Self {
        CharClass {
            ranges: vec![
                CharRange {
                    start: '\n',
                    end: '\n',
                },
                CharRange {
                    start: '\r',
                    end: '\r',
                },
                CharRange {
                    start: '\u{2028}',
                    end: '\u{2028}',
                },
                CharRange {
                    start: '\u{2029}',
                    end: '\u{2029}',
                },
            ],
            negated: false,
        }
    }

    fn dot() -> Self {
        Self::line_terminators().negated()
    }

    pub(super) fn char(ch: char) -> Self {
        CharClass {
            ranges: vec![CharRange { start: ch, end: ch }],
            negated: false,
        }
    }
}

#[derive(Debug, PartialEq, Clone)]
pub enum Token {
    Concat,   // Pseudo element?
    Question, // ?
    Star,     // *
    Plus,     // +
    Caret,    // ^
    Dollar,   // $
    Pipe,     // |
    LParen,   // (
    RParen,   // )
    LBrace,   // {
    RBrace,   // }
    Class(CharClass),
}

type TokenStream = Vec<Token>;

#[derive(Debug, PartialEq)]
pub(super) struct PostfixTokens {
    pub(super) match_start: bool,
    pub(super) match_end: bool,
    tokens: Vec<Token>,
}

impl PostfixTokens {
    pub fn iter(&self) -> std::slice::Iter<'_, Token> {
        self.tokens.iter()
    }

    pub fn into_iter(self) -> std::vec::IntoIter<Token> {
        self.tokens.into_iter()
    }
}

fn parse_char_class(
    iter: &mut Peekable<impl Iterator<Item = char>>,
    offset: usize,
) -> Result<(CharClass, usize), LexError> {
    let mut idx = 0;
    let mut contents: Vec<ClassAtom> = vec![];

    while let Some(ch) = iter.next() {
        match ch {
            '\\' => {
                idx += 1;

                let escaped = *iter
                    .peek()
                    .ok_or(LexError::UnexpectedEof(offset + idx))?;

                iter.next();
                idx += 1;

                contents.push(ClassAtom {
                    ch: escaped,
                    escaped: true,
                });
            }
            ']' => {
                let chclass = CharClass::parse(&contents, offset)?;
                return Ok((chclass, offset + idx + 1));
            }
            _ => {
                idx += 1;
                contents.push(ClassAtom { ch, escaped: false });
            }
        }
    }

    Err(LexError::UnexpectedEof(offset + idx))
}

pub(super) fn tokenize(
    raw: impl IntoIterator<Item = char>,
) -> Result<TokenStream, LexError> {
    let mut result: TokenStream = vec![];
    let mut idx = 0;
    let mut iter = raw.into_iter().peekable();

    while let Some(ch) = iter.next() {
        idx += 1;
        let token = match ch {
            '^' => {
                if idx != 1 {
                    return Err(LexError::UnexpectedCaret(idx));
                }

                Token::Caret
            }
            '$' => {
                if iter.peek().is_some() {
                    return Err(LexError::UnexpectedDollar(idx));
                }

                Token::Dollar
            }
            '\\' => {
                let escaped =
                    *iter.peek().ok_or(LexError::UnexpectedEof(idx))?;

                iter.next();
                idx += 1;

                match escaped {
                    'w' => Token::Class(CharClass::word()),
                    'W' => Token::Class(CharClass::word().negated()),
                    'd' => Token::Class(CharClass::digits()),
                    'D' => Token::Class(CharClass::digits().negated()),
                    's' => Token::Class(CharClass::space()),
                    'S' => Token::Class(CharClass::space().negated()),
                    _ => Token::Class(CharClass::char(escaped)),
                }
            }
            '\n' | '\r' => {
                continue;
            }
            '(' => Token::LParen,
            ')' => Token::RParen,
            '|' => Token::Pipe,
            '.' => Token::Class(CharClass::dot()),
            '*' => Token::Star,
            '+' => Token::Plus,
            '?' => Token::Question,
            '[' => {
                let (chclass, updated_idx) = parse_char_class(&mut iter, idx)?;
                idx = updated_idx;

                Token::Class(chclass)
            }
            _ => Token::Class(CharClass::char(ch)),
        };

        insert_maybe_concat(&mut result, token);
    }

    Ok(result)
}

fn insert_maybe_concat(tokens: &mut TokenStream, token: Token) {
    if let Some(last) = tokens.last()
        && insert_concat_after(last)
        && insert_concat_before(&token)
    {
        tokens.push(Token::Concat);
    }

    tokens.push(token);
}

fn insert_concat_after(token: &Token) -> bool {
    matches!(
        token,
        Token::RParen
            | Token::Star
            | Token::Plus
            | Token::Question
            | Token::Class(_)
    )
}

fn insert_concat_before(token: &Token) -> bool {
    matches!(token, Token::LParen | Token::Class(_))
}

fn precedence(token: &Token) -> usize {
    match token {
        Token::Star | Token::Plus | Token::Question => 3,
        Token::Concat => 2,
        Token::Pipe => 1,
        _ => 0,
    }
}

// Shunting
pub(super) fn to_postfix(
    tokens: TokenStream,
) -> Result<PostfixTokens, LexError> {
    let mut ops: Vec<Token> = vec![];
    let mut out = vec![];
    let mut match_start = false;
    let mut match_last = false;

    for tok in tokens {
        if tok == Token::Caret {
            match_start = true;
            continue;
        }

        if tok == Token::Dollar {
            match_last = true;
            continue;
        }

        if tok == Token::LParen {
            ops.push(tok);
            continue;
        }

        if tok == Token::RParen {
            loop {
                let last = ops.pop().ok_or(LexError::UnbalancedParens)?;

                if last == Token::LParen {
                    break;
                }

                out.push(last);
            }
            continue;
        }

        let tok_prec = precedence(&tok);

        if tok_prec == 0 {
            out.push(tok);
            continue;
        }

        loop {
            let last_op = ops.last();

            let last_op = match last_op {
                Some(token) => token,
                None => {
                    ops.push(tok);
                    break;
                }
            };

            if *last_op == Token::LParen {
                ops.push(tok);
                break;
            }

            let last_prec = precedence(last_op);

            if last_prec < tok_prec {
                ops.push(tok);
                break;
            }

            out.push(ops.pop().unwrap());
        }
    }

    for op in ops.into_iter().rev() {
        if op == Token::LParen {
            return Err(LexError::UnbalancedParens);
        }

        out.push(op);
    }

    Ok(PostfixTokens {
        match_start,
        match_end: match_last,
        tokens: out,
    })
}

fn next_char(c: char) -> char {
    char::from_u32(c as u32 + 1).unwrap_or(c)
}

#[cfg(test)]
mod tests {
    use super::Token::*;
    use super::*;

    mod tokenize {
        use super::*;

        #[test]
        fn test_simple_literals() {
            assert_eq!(
                tokenize("abcd".chars()).unwrap(),
                vec![
                    Class(CharClass::char('a')),
                    Concat,
                    Class(CharClass::char('b')),
                    Concat,
                    Class(CharClass::char('c')),
                    Concat,
                    Class(CharClass::char('d'))
                ]
            );
        }

        #[test]
        fn test_escape() {
            assert_eq!(
                tokenize("\\\\\\(".chars()).unwrap(),
                vec![
                    Class(CharClass::char('\\')),
                    Concat,
                    Class(CharClass::char('(')),
                ]
            );

            assert_eq!(
                tokenize("ab \\".chars()),
                Err(LexError::UnexpectedEof(4))
            )
        }

        #[test]
        fn test_several() {
            assert_eq!(
                tokenize("^ab*|d".chars()).unwrap(),
                vec![
                    Caret,
                    Class(CharClass::char('a')),
                    Concat,
                    Class(CharClass::char('b')),
                    Star,
                    Pipe,
                    Class(CharClass::char('d'))
                ]
            );
        }

        #[test]
        fn test_question_token() {
            assert_eq!(
                tokenize("a?".chars()),
                Ok(vec![Class(CharClass::char('a')), Question])
            );
        }
    }

    mod to_postfix {
        use super::*;

        #[test]
        fn test_without_parens() {
            // a (+) b* | d*
            // a b * (+) d |
            let tokens = tokenize("ab*|d".chars()).unwrap();
            let postfix = to_postfix(tokens).unwrap();
            assert_eq!(
                postfix.tokens,
                vec![
                    Class(CharClass::char('a')),
                    Class(CharClass::char('b')),
                    Star,
                    Concat,
                    Class(CharClass::char('d')),
                    Pipe,
                ]
            );

            // a (+) b * | d * (+) x +
            // a b * (+) d * x + (+) |
            let tokens = tokenize("ab*|d*x+".chars()).unwrap();
            let postfix = to_postfix(tokens).unwrap();

            assert_eq!(
                postfix.tokens,
                vec![
                    Class(CharClass::char('a')),
                    Class(CharClass::char('b')),
                    Star,
                    Concat,
                    Class(CharClass::char('d')),
                    Star,
                    Class(CharClass::char('x')),
                    Plus,
                    Concat,
                    Pipe
                ]
            );
        }

        #[test]
        fn test_with_parens() {
            // a (+) (b* | c)
            // a b * c | (+)
            let tokens = tokenize("a(b*|c)".chars()).unwrap();
            let postfix = to_postfix(tokens).unwrap();

            assert_eq!(
                postfix.tokens,
                vec![
                    Class(CharClass::char('a')),
                    Class(CharClass::char('b')),
                    Star,
                    Class(CharClass::char('c')),
                    Pipe,
                    Concat,
                ]
            );
        }

        #[test]
        fn test_bad_parens() {
            let parens = tokenize("((".chars()).unwrap();
            let postfix = to_postfix(parens);

            assert_eq!(postfix, Err(LexError::UnbalancedParens));
        }

        #[test]
        fn test_question() {
            let tokens = tokenize("abc?d".chars()).unwrap();
            let postfix = to_postfix(tokens).unwrap();

            assert_eq!(
                postfix.tokens,
                vec![
                    Class(CharClass::char('a')),
                    Class(CharClass::char('b')),
                    Concat,
                    Class(CharClass::char('c')),
                    Question,
                    Concat,
                    Class(CharClass::char('d')),
                    Concat,
                ]
            )
        }

        #[test]
        fn test_char_class() {
            let tokens = tokenize("a[b-fg-y]?z".chars()).unwrap();
            let postfix = to_postfix(tokens).unwrap();

            assert_eq!(
                postfix.tokens,
                vec![
                    Class(CharClass::char('a')),
                    Class(CharClass {
                        ranges: vec![CharRange {
                            start: 'b',
                            end: 'y',
                        },],
                        negated: false,
                    }),
                    Question,
                    Concat,
                    Class(CharClass::char('z')),
                    Concat,
                ]
            );
        }

        #[test]
        fn test_char_class_dot() {
            let tokens = tokenize("a.?z".chars()).unwrap();
            let postfix = to_postfix(tokens).unwrap();

            assert_eq!(
                postfix.tokens,
                vec![
                    Class(CharClass::char('a')),
                    Class(CharClass::dot()),
                    Question,
                    Concat,
                    Class(CharClass::char('z')),
                    Concat,
                ]
            );
        }

        #[test]
        fn test_char_class_escapes() {
            let tokens = tokenize("\\s\\S\\w\\W\\d\\D".chars()).unwrap();
            let postfix = to_postfix(tokens).unwrap();

            assert_eq!(
                postfix.tokens,
                vec![
                    Class(CharClass::space()),
                    Class(CharClass::space().negated()),
                    Concat,
                    Class(CharClass::word()),
                    Concat,
                    Class(CharClass::word().negated()),
                    Concat,
                    Class(CharClass::digits()),
                    Concat,
                    Class(CharClass::digits().negated()),
                    Concat,
                ]
            );
        }

        #[test]
        fn test_unterminated_escape() {
            let err = tokenize("[\\".chars()).unwrap_err();
            assert_eq!(err, LexError::UnexpectedEof(2));

            let err = tokenize("[a\\".chars()).unwrap_err();
            assert_eq!(err, LexError::UnexpectedEof(3));

            let err = tokenize("a[b\\".chars()).unwrap_err();
            assert_eq!(err, LexError::UnexpectedEof(4));
        }
    }

    mod character_class {
        use super::*;

        fn atom_vec(s: &str) -> Vec<ClassAtom> {
            let mut content: Vec<ClassAtom> = vec![];
            let mut iter = s.chars().peekable();

            while let Some(ch) = iter.next() {
                match ch {
                    '\\' => {
                        let escaped = *iter.peek().unwrap();

                        iter.next();

                        content.push(ClassAtom {
                            ch: escaped,
                            escaped: true,
                        });
                    }
                    _ => {
                        content.push(ClassAtom { ch, escaped: false });
                    }
                }
            }

            content
        }

        fn char_vec(s: &str) -> Vec<char> {
            s.chars().collect()
        }

        #[track_caller]
        fn assert_all_match(chclass: &CharClass, arr: &[char]) {
            for ch in arr {
                assert!(chclass.matches(*ch), r#"Regex must contain "{}""#, ch);
            }
        }

        #[track_caller]
        fn assert_none_match(chclass: &CharClass, arr: &[char]) {
            for ch in arr {
                assert!(
                    !chclass.matches(*ch),
                    r#"Regex must not contain "{}""#,
                    ch
                );
            }
        }

        #[test]
        fn test_basic() {
            let input = atom_vec("0-9ka-zA-Z%");
            let parsed = CharClass::parse(&input, 0).unwrap();

            assert_eq!(
                parsed.ranges,
                vec![
                    CharRange {
                        start: '%',
                        end: '%',
                    },
                    CharRange {
                        start: '0',
                        end: '9',
                    },
                    CharRange {
                        start: 'A',
                        end: 'Z',
                    },
                    CharRange {
                        start: 'a',
                        end: 'z',
                    },
                ]
            );

            assert_all_match(
                &parsed,
                &char_vec(concat!(
                    "0123456789",
                    "abcdefghijklmnopqrstuvwxyz",
                    "ABCDEFGHIJKLMNOPQRSTUVWXYZ",
                    "%",
                )),
            );

            assert_none_match(&parsed, &char_vec("!@#$^&*)_ \n\r"));
        }

        #[test]
        fn test_single_chars() {
            let input = atom_vec("kza%");
            let parsed = CharClass::parse(&input, 0).unwrap();

            assert_eq!(
                parsed.ranges,
                vec![
                    CharRange {
                        start: '%',
                        end: '%'
                    },
                    CharRange {
                        start: 'a',
                        end: 'a'
                    },
                    CharRange {
                        start: 'k',
                        end: 'k'
                    },
                    CharRange {
                        start: 'z',
                        end: 'z'
                    },
                ]
            );
        }

        #[test]
        fn test_dedupes() {
            let input = atom_vec("aaac");
            let parsed = CharClass::parse(&input, 0).unwrap();

            assert_eq!(
                parsed.ranges,
                vec![
                    CharRange {
                        start: 'a',
                        end: 'a'
                    },
                    CharRange {
                        start: 'c',
                        end: 'c'
                    },
                ]
            );
        }

        #[test]
        fn test_merges_overlapping_ranges() {
            let input = atom_vec("a-fd-z");
            let parsed = CharClass::parse(&input, 0).unwrap();

            assert_eq!(
                parsed.ranges,
                vec![CharRange {
                    start: 'a',
                    end: 'z'
                },]
            );
        }

        #[test]
        fn test_range_order_error() {
            let input = atom_vec("z-a");
            let err = CharClass::parse(&input, 7).unwrap_err();

            assert_eq!(err, LexError::IncorrectCharClassRangeOrder(7));
        }

        #[test]
        fn test_negated() {
            let input = atom_vec("^a-z");
            let parsed = CharClass::parse(&input, 0).unwrap();

            assert!(parsed.negated);
            assert_eq!(
                parsed.ranges,
                vec![CharRange {
                    start: 'a',
                    end: 'z'
                },]
            );
        }

        #[test]
        fn test_dash_literal_at_start() {
            let input = atom_vec("-a^");
            let parsed = CharClass::parse(&input, 0).unwrap();

            assert_eq!(
                parsed.ranges,
                vec![
                    CharRange {
                        start: '-',
                        end: '-'
                    },
                    CharRange {
                        start: '^',
                        end: '^',
                    },
                    CharRange {
                        start: 'a',
                        end: 'a'
                    },
                ]
            );
        }

        #[test]
        fn test_dash_literal_at_end() {
            let input = atom_vec("a-");
            let parsed = CharClass::parse(&input, 0).unwrap();

            assert_eq!(
                parsed.ranges,
                vec![
                    CharRange {
                        start: '-',
                        end: '-'
                    },
                    CharRange {
                        start: 'a',
                        end: 'a'
                    },
                ]
            );
        }

        #[test]
        fn test_merge_adjacent() {
            let input = atom_vec("a-fgh-z");
            let parsed = CharClass::parse(&input, 0).unwrap();

            assert_eq!(
                parsed.ranges,
                vec![CharRange {
                    start: 'a',
                    end: 'z',
                }]
            );
        }

        #[test]
        fn test_merge_numbers() {
            let input = atom_vec("0-9");
            let parsed = CharClass::parse(&input, 0).unwrap();

            assert_eq!(
                parsed.ranges,
                vec![CharRange {
                    start: '0',
                    end: '9',
                }]
            );
        }

        #[test]
        fn test_merge_word() {
            let input = atom_vec("0-9a-zA-Z_");
            let parsed = CharClass::parse(&input, 0).unwrap();

            assert_eq!(
                parsed.ranges,
                vec![
                    CharRange {
                        start: '0',
                        end: '9',
                    },
                    CharRange {
                        start: 'A',
                        end: 'Z',
                    },
                    CharRange {
                        start: '_',
                        end: '_',
                    },
                    CharRange {
                        start: 'a',
                        end: 'z',
                    },
                ]
            );
        }

        #[test]
        fn test_escaped_dash() {
            let input = atom_vec("0\\-9");
            let parsed = CharClass::parse(&input, 0).unwrap();

            assert_eq!(
                parsed.ranges,
                vec![
                    CharRange {
                        start: '-',
                        end: '-',
                    },
                    CharRange {
                        start: '0',
                        end: '0',
                    },
                    CharRange {
                        start: '9',
                        end: '9',
                    },
                ]
            );
        }

        #[test]
        fn test_escaped_negate() {
            let input = atom_vec("\\^");
            let parsed = CharClass::parse(&input, 0).unwrap();

            assert_eq!(
                parsed.ranges,
                vec![CharRange {
                    start: '^',
                    end: '^'
                }]
            );
        }

        #[test]
        fn test_escaped_endbracket() {
            let input = atom_vec("A-\\]");
            let parsed = CharClass::parse(&input, 0).unwrap();

            assert_eq!(
                parsed.ranges,
                vec![CharRange {
                    start: 'A',
                    end: ']'
                }]
            );
        }
    }
}
