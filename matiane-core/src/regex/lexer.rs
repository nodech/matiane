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

    #[error("Incorrect char class.")]
    IncorrectCharClass,
    #[error("Range out of order in character class at {0}.")]
    IncorrectCharClassRangeOrder(usize),
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, PartialOrd, Ord)]
pub struct CharRange {
    pub start: char,
    pub end: char,
}

#[derive(Debug, Default, PartialEq, Clone)]
pub struct CharacterClass {
    pub ranges: Vec<CharRange>,
    pub negated: bool,
}

impl CharacterClass {
    fn parse(chars: &[char], offset: usize) -> Result<Self, LexError> {
        let mut char_class = Self::default();
        let mut i = 0;

        if chars.first() == Some(&'^') {
            char_class.negated = true;
            i = 1;
        }

        while i < chars.len() {
            let chr = chars[i];
            let next = chars.get(i + 1);
            let next2 = chars.get(i + 2);

            match (chr, next, next2) {
                (start, Some('-'), Some(end)) => {
                    if start > *end {
                        return Err(LexError::IncorrectCharClassRangeOrder(
                            offset + i,
                        ));
                    }

                    char_class.ranges.push(CharRange { start, end: *end });
                    i += 3;
                }
                _ => {
                    char_class.ranges.push(CharRange {
                        start: chr,
                        end: chr,
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
}

#[derive(Debug, PartialEq, Clone)]
pub enum Token {
    Char(char),
    Dot,      // .
    Question, // ?
    Star,     // *
    Plus,     // +
    Caret,    // ^
    Dollar,   // $
    Pipe,     // |
    LParen,   // (
    RParen,   // )
    // LBracket, // [
    // RBracket, // ]
    // LBrace,   // {
    // RBrace,   // }
    // Dash,     // -
    // Digits,  // \d
    // Word,    // \w
    // Space,   // \s
    CharClass(CharacterClass),
    Concat, // Pseudo element?
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
}

fn parse_char_class(
    iter: &mut Peekable<impl Iterator<Item = char>>,
    offset: usize,
) -> Result<(CharacterClass, usize), LexError> {
    let mut idx = 0;
    let mut contents: Vec<char> = vec![];

    while let Some(ch) = iter.next() {
        idx += 1;

        match ch {
            '\\' => {
                let escaped = *iter
                    .peek()
                    .ok_or(LexError::UnexpectedEof(offset + idx))?;

                iter.next();
                idx += 1;

                contents.push(escaped);
            }
            ']' => {
                let chclass = CharacterClass::parse(&contents, offset)?;
                return Ok((chclass, offset + idx));
            }
            _ => {
                contents.push(ch);
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
                Token::Char(escaped)
            }
            '\n' | '\r' => {
                continue;
            }
            '(' => Token::LParen,
            ')' => Token::RParen,
            '|' => Token::Pipe,
            '.' => Token::Dot,
            '*' => Token::Star,
            '+' => Token::Plus,
            '?' => Token::Question,
            '[' => {
                let (chclass, updated_idx) = parse_char_class(&mut iter, idx)?;
                idx = updated_idx;

                Token::CharClass(chclass)
            }
            _ => Token::Char(ch),
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
        Token::Char(_)
            | Token::RParen
            | Token::Dot
            | Token::Star
            | Token::Plus
            | Token::Question
    )
}

fn insert_concat_before(token: &Token) -> bool {
    matches!(token, Token::Char(_) | Token::Dot | Token::LParen)
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
pub(super) fn topostfix(
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
                    Char('a'),
                    Concat,
                    Char('b'),
                    Concat,
                    Char('c'),
                    Concat,
                    Char('d')
                ]
            );
        }

        #[test]
        fn test_escape() {
            assert_eq!(
                tokenize("\\\\\\(".chars()).unwrap(),
                vec![Char('\\'), Concat, Char('('),]
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
                    Char('a'),
                    Concat,
                    Char('b'),
                    Star,
                    Pipe,
                    Char('d')
                ]
            );
        }

        #[test]
        fn test_question_token() {
            assert_eq!(tokenize("a?".chars()), Ok(vec![Char('a'), Question]));
        }
    }

    mod toposort {
        use super::*;

        #[test]
        fn test_without_parens() {
            // a (+) b* | d*
            // a b * (+) d |
            let tokens = tokenize("ab*|d".chars()).unwrap();
            let postfix = topostfix(tokens).unwrap();
            assert_eq!(
                postfix.tokens,
                vec![Char('a'), Char('b'), Star, Concat, Char('d'), Pipe,]
            );

            // a (+) b * | d * (+) x +
            // a b * (+) d * x + (+) |
            let tokens = tokenize("ab*|d*x+".chars()).unwrap();
            let postfix = topostfix(tokens).unwrap();

            assert_eq!(
                postfix.tokens,
                vec![
                    Char('a'),
                    Char('b'),
                    Star,
                    Concat,
                    Char('d'),
                    Star,
                    Char('x'),
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
            let postfix = topostfix(tokens).unwrap();

            assert_eq!(
                postfix.tokens,
                vec![Char('a'), Char('b'), Star, Char('c'), Pipe, Concat,]
            );
        }

        #[test]
        fn test_bad_parens() {
            let parens = tokenize("((".chars()).unwrap();
            let postfix = topostfix(parens);

            assert_eq!(postfix, Err(LexError::UnbalancedParens));
        }

        #[test]
        fn test_question() {
            let tokens = tokenize("abc?d".chars()).unwrap();
            let postfix = topostfix(tokens).unwrap();

            assert_eq!(
                postfix.tokens,
                vec![
                    Char('a'),
                    Char('b'),
                    Concat,
                    Char('c'),
                    Question,
                    Concat,
                    Char('d'),
                    Concat,
                ]
            )
        }
    }

    mod character_class {
        use super::*;

        fn char_vec(s: &str) -> Vec<char> {
            s.chars().collect()
        }

        #[test]
        fn test_basic() {
            let input = char_vec("0-9ka-zA-Z%");
            let parsed = CharacterClass::parse(&input, 0).unwrap();

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
            )
        }

        #[test]
        fn test_single_chars() {
            let input = char_vec("kza%");
            let parsed = CharacterClass::parse(&input, 0).unwrap();

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
            let input = char_vec("aaac");
            let parsed = CharacterClass::parse(&input, 0).unwrap();

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
            let input = char_vec("a-fd-z");
            let parsed = CharacterClass::parse(&input, 0).unwrap();

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
            let input = char_vec("z-a");
            let err = CharacterClass::parse(&input, 7).unwrap_err();

            assert_eq!(err, LexError::IncorrectCharClassRangeOrder(7));
        }

        #[test]
        fn test_negated() {
            let input = char_vec("^a-z");
            let parsed = CharacterClass::parse(&input, 0).unwrap();

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
            let input = char_vec("-a^");
            let parsed = CharacterClass::parse(&input, 0).unwrap();

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
            let input = char_vec("a-");
            let parsed = CharacterClass::parse(&input, 0).unwrap();

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
            let input = char_vec("a-fgh-z");
            let parsed = CharacterClass::parse(&input, 0).unwrap();

            assert_eq!(
                parsed.ranges,
                vec![CharRange {
                    start: 'a',
                    end: 'z',
                }]
            );
        }
    }
}
