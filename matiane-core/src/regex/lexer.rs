use std::fmt::Debug;
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
    Concat, // pseudo element?
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

    #[test]
    fn test_topostfix_with_parens() {
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
    fn test_topostfix_bad_parens() {
        let parens = tokenize("((".chars()).unwrap();
        let postfix = topostfix(parens);

        assert_eq!(postfix, Err(LexError::UnbalancedParens));
    }

    #[test]
    fn test_topostfix_question() {
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
