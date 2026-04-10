use super::lexer::PostfixTokens;
use super::lexer::Token;
use std::collections::HashSet;
use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum ParseError {
    #[error("Lexer failed: {0}")]
    LexerError(#[from] super::lexer::LexError),

    #[error("Unsupported token {0:?}")]
    UnsupportedToken(Token),

    #[error("Empty regex")]
    EmptyRegex,

    #[error("Invalid syntax")]
    MalformedRegex,
}

pub type ParseResult<T> = Result<T, ParseError>;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct StateId(usize);

#[derive(Debug, Clone, PartialEq)]
enum FragmentState {
    Split {
        out1: Option<StateId>,
        out2: Option<StateId>,
    },
    Match {
        symbol: char,
        next: Option<StateId>,
    },
    Finish,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) enum NfaState {
    Split { out1: StateId, out2: StateId },
    Match { symbol: char, next: StateId },
    Finish,
}

#[derive(Debug)]
pub(super) struct Nfa {
    pub(super) entry: StateId,
    pub(super) match_start: bool,
    pub(super) match_end: bool,
    pub(super) states: Vec<NfaState>,
}

impl std::ops::Index<StateId> for Vec<NfaState> {
    type Output = NfaState;

    fn index(&self, index: StateId) -> &Self::Output {
        &self[index.0]
    }
}

impl std::ops::IndexMut<StateId> for Vec<NfaState> {
    fn index_mut(&mut self, index: StateId) -> &mut Self::Output {
        &mut self[index.0]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct FragmentId(usize);

impl From<FragmentId> for StateId {
    fn from(value: FragmentId) -> Self {
        Self(value.0)
    }
}

#[derive(Debug)]
struct Fragment {
    start: StateId,
    outs: Vec<StateId>,
}

impl std::ops::Index<FragmentId> for Vec<Fragment> {
    type Output = Fragment;

    fn index(&self, index: FragmentId) -> &Self::Output {
        &self[index.0]
    }
}

impl std::ops::IndexMut<FragmentId> for Vec<Fragment> {
    fn index_mut(&mut self, index: FragmentId) -> &mut Self::Output {
        &mut self[index.0]
    }
}

#[derive(Debug)]
pub struct NfaBuilder {
    states: Vec<FragmentState>,
    stack: Vec<Fragment>,
    match_start: bool,
    match_end: bool,
}

impl NfaBuilder {
    fn state(&mut self, s: FragmentState) -> StateId {
        self.states.push(s);
        StateId(self.states.len() - 1)
    }

    fn push(&mut self, frag: Fragment) {
        self.stack.push(frag);
    }

    fn pop(&mut self) -> ParseResult<Fragment> {
        self.stack.pop().ok_or(ParseError::MalformedRegex)
    }

    fn patch(&mut self, outs: Vec<StateId>, s: StateId) {
        for out in outs {
            let state = &mut self.states[out.0];

            match state {
                FragmentState::Match { next, .. } => {
                    *next = Some(s);
                }
                FragmentState::Split { out1, out2 } => {
                    if out1.is_none() {
                        *out1 = Some(s);
                    } else {
                        *out2 = Some(s);
                    }
                }
                _ => {}
            }
        }
    }

    fn match_char(&mut self, ch: char) -> ParseResult<()> {
        let s = self.state(FragmentState::Match {
            symbol: ch,
            next: None,
        });

        self.push(Fragment {
            start: s,
            outs: vec![s],
        });

        Ok(())
    }

    fn concat(&mut self) -> ParseResult<()> {
        let e2 = self.pop()?;
        let e1 = self.pop()?;

        self.patch(e1.outs, e2.start);

        self.push(Fragment {
            start: e1.start,
            outs: e2.outs,
        });

        Ok(())
    }

    fn pipe(&mut self) -> ParseResult<()> {
        let right = self.pop()?;
        let left = self.pop()?;

        let s = self.state(FragmentState::Split {
            out1: Some(left.start),
            out2: Some(right.start),
        });

        let mut outs = left.outs;
        outs.extend(right.outs);

        self.push(Fragment { start: s, outs });

        Ok(())
    }

    fn star(&mut self) -> ParseResult<()> {
        let last = self.pop()?;

        let s = self.state(FragmentState::Split {
            out1: Some(last.start),
            out2: None,
        });

        self.patch(last.outs, s);

        self.push(Fragment {
            start: s,
            outs: vec![s],
        });

        Ok(())
    }

    fn plus(&mut self) -> ParseResult<()> {
        let last = self.pop()?;

        let s = self.state(FragmentState::Split {
            out1: Some(last.start),
            out2: None,
        });

        self.patch(last.outs, s);

        self.push(Fragment {
            start: last.start,
            outs: vec![s],
        });

        Ok(())
    }

    fn question(&mut self) -> ParseResult<()> {
        let last = self.pop()?;

        let s = self.state(FragmentState::Split {
            out1: Some(last.start),
            out2: None,
        });

        let mut outs = last.outs;
        outs.push(s);

        self.push(Fragment { start: s, outs });

        Ok(())
    }

    fn finish(mut self) -> ParseResult<Nfa> {
        let last = self.pop()?;

        let s = self.state(FragmentState::Finish);
        self.patch(last.outs, s);

        self.push(Fragment {
            start: last.start,
            outs: vec![],
        });

        let final_states: Result<Vec<NfaState>, ParseError> = self
            .states
            .into_iter()
            .map(|fs| {
                Ok(match fs {
                    FragmentState::Match { symbol, next } => NfaState::Match {
                        symbol,
                        next: next.ok_or(ParseError::MalformedRegex)?,
                    },
                    FragmentState::Split { out1, out2 } => NfaState::Split {
                        out1: out1.ok_or(ParseError::MalformedRegex)?,
                        out2: out2.ok_or(ParseError::MalformedRegex)?,
                    },
                    FragmentState::Finish => NfaState::Finish,
                })
            })
            .collect();

        Ok(Nfa {
            match_start: self.match_start,
            match_end: self.match_end,
            states: final_states?,
            entry: last.start,
        })
    }

    fn build_frags(&mut self, tokens: &PostfixTokens) -> ParseResult<()> {
        for el in tokens.iter() {
            match el {
                Token::Char(ch) => self.match_char(*ch)?,
                Token::Concat => self.concat()?,
                Token::Pipe => self.pipe()?,
                Token::Star => self.star()?,
                Token::Plus => self.plus()?,
                Token::Question => self.question()?,
                _ => return Err(ParseError::UnsupportedToken(el.clone())),
            }
        }

        Ok(())
    }

    fn build_nfa(self) -> ParseResult<Nfa> {
        if self.stack.is_empty() {
            return Err(ParseError::EmptyRegex);
        }

        self.finish()
    }

    pub(super) fn build(tokens: &PostfixTokens) -> ParseResult<Nfa> {
        let mut builder = NfaBuilder {
            states: vec![],
            stack: vec![],
            match_start: tokens.match_start,
            match_end: tokens.match_end,
        };

        builder.build_frags(tokens)?;
        builder.build_nfa()
    }
}

#[cfg(test)]
mod tests {
    use super::super::lexer::{to_postfix, tokenize};
    use super::*;

    #[test]
    fn test_single_char_a() {
        let tokens = tokenize("a".chars()).unwrap();
        let postfix = to_postfix(tokens).unwrap();
        let nfa = NfaBuilder::build(&postfix).unwrap();

        let expected = vec![
            NfaState::Match {
                symbol: 'a',
                next: StateId(1),
            },
            NfaState::Finish,
        ];

        assert_eq!(nfa.states, expected);
        assert_eq!(nfa.entry, StateId(0));
    }

    #[test]
    fn test_simple_abc() {
        let tokens = tokenize("abc".chars()).unwrap();
        let postfix = to_postfix(tokens).unwrap();
        let nfa = NfaBuilder::build(&postfix).unwrap();

        let expected = vec![
            NfaState::Match {
                symbol: 'a',
                next: StateId(1),
            },
            NfaState::Match {
                symbol: 'b',
                next: StateId(2),
            },
            NfaState::Match {
                symbol: 'c',
                next: StateId(3),
            },
            NfaState::Finish,
        ];

        assert_eq!(nfa.states, expected);
        assert_eq!(nfa.entry, StateId(0));
    }

    #[test]
    fn test_four_chars_abcd() {
        let tokens = tokenize("abcd".chars()).unwrap();
        let postfix = to_postfix(tokens).unwrap();
        let nfa = NfaBuilder::build(&postfix).unwrap();

        let expected = vec![
            NfaState::Match {
                symbol: 'a',
                next: StateId(1),
            },
            NfaState::Match {
                symbol: 'b',
                next: StateId(2),
            },
            NfaState::Match {
                symbol: 'c',
                next: StateId(3),
            },
            NfaState::Match {
                symbol: 'd',
                next: StateId(4),
            },
            NfaState::Finish,
        ];
        assert_eq!(nfa.states, expected);
        assert_eq!(nfa.entry, StateId(0));
    }

    #[test]
    fn test_simple_ab_or_d() {
        let tokens = tokenize("ab|d".chars()).unwrap();
        let postfix = to_postfix(tokens).unwrap();
        let nfa = NfaBuilder::build(&postfix).unwrap();

        let expected = vec![
            NfaState::Match {
                symbol: 'a',
                next: StateId(1),
            },
            NfaState::Match {
                symbol: 'b',
                next: StateId(4),
            },
            NfaState::Match {
                symbol: 'd',
                next: StateId(4),
            },
            NfaState::Split {
                out1: StateId(0),
                out2: StateId(2),
            },
            NfaState::Finish,
        ];

        assert_eq!(nfa.states, expected);
        assert_eq!(nfa.entry, StateId(3));
    }

    #[test]
    fn test_ab_or_cd() {
        let tokens = tokenize("ab|cd".chars()).unwrap();
        let postfix = to_postfix(tokens).unwrap();
        let nfa = NfaBuilder::build(&postfix).unwrap();

        let expected = vec![
            NfaState::Match {
                // 0
                symbol: 'a',
                next: StateId(1),
            },
            NfaState::Match {
                // 1
                symbol: 'b',
                next: StateId(5),
            },
            NfaState::Match {
                // 2
                symbol: 'c',
                next: StateId(3),
            },
            NfaState::Match {
                // 3
                symbol: 'd',
                next: StateId(5),
            },
            NfaState::Split {
                // 4
                out1: StateId(0),
                out2: StateId(2),
            },
            NfaState::Finish, // 5
        ];
        assert_eq!(nfa.states, expected);
        assert_eq!(nfa.entry, StateId(4));
    }

    #[test]
    fn test_aorc_withd_or_io() {
        let tokens = tokenize("(a|c)d|io".chars()).unwrap();
        let postfix = to_postfix(tokens).unwrap();
        let nfa = NfaBuilder::build(&postfix).unwrap();

        let expected = vec![
            NfaState::Match {
                // 0
                symbol: 'a',
                next: StateId(3),
            },
            NfaState::Match {
                // 1
                symbol: 'c',
                next: StateId(3),
            },
            NfaState::Split {
                // 2
                out1: StateId(0),
                out2: StateId(1),
            },
            NfaState::Match {
                // 3
                symbol: 'd',
                next: StateId(7),
            },
            NfaState::Match {
                // 4
                symbol: 'i',
                next: StateId(5),
            },
            NfaState::Match {
                // 5
                symbol: 'o',
                next: StateId(7),
            },
            NfaState::Split {
                // 6
                out1: StateId(2),
                out2: StateId(4),
            },
            NfaState::Finish, // 7
        ];

        assert_eq!(nfa.states, expected);
        assert_eq!(nfa.entry, StateId(6));
    }

    #[test]
    fn test_aorb_concat_cord() {
        let tokens = tokenize("(a|b)(c|d)".chars()).unwrap();
        let postfix = to_postfix(tokens).unwrap();
        let nfa = NfaBuilder::build(&postfix).unwrap();

        let expected = vec![
            NfaState::Match {
                symbol: 'a',
                next: StateId(5),
            },
            NfaState::Match {
                symbol: 'b',
                next: StateId(5),
            },
            NfaState::Split {
                out1: StateId(0),
                out2: StateId(1),
            },
            NfaState::Match {
                symbol: 'c',
                next: StateId(6),
            },
            NfaState::Match {
                symbol: 'd',
                next: StateId(6),
            },
            NfaState::Split {
                out1: StateId(3),
                out2: StateId(4),
            },
            NfaState::Finish,
        ];

        assert_eq!(nfa.states, expected);
        assert_eq!(nfa.entry, StateId(2));
    }

    #[test]
    fn test_astar_bstar() {
        let tokens = tokenize("a*|b*".chars()).unwrap();
        let postfix = to_postfix(tokens).unwrap();
        let nfa = NfaBuilder::build(&postfix).unwrap();

        let expected = [
            NfaState::Match {
                // 0
                symbol: 'a',
                next: StateId(1),
            },
            NfaState::Split {
                // 1
                out1: StateId(0),
                out2: StateId(5),
            },
            NfaState::Match {
                // 2
                symbol: 'b',
                next: StateId(3),
            },
            NfaState::Split {
                // 3
                out1: StateId(2),
                out2: StateId(5),
            },
            NfaState::Split {
                // 4
                out1: StateId(1),
                out2: StateId(3),
            },
            NfaState::Finish, // 5
        ];

        assert_eq!(nfa.states, expected);
        assert_eq!(nfa.entry, StateId(4));
    }

    #[test]
    fn test_aorbstar_bstar() {
        let tokens = tokenize("(a|b)*|b*".chars()).unwrap();
        let postfix = to_postfix(tokens).unwrap();
        let nfa = NfaBuilder::build(&postfix).unwrap();

        let expected = [
            NfaState::Match {
                // 0
                symbol: 'a',
                next: StateId(3),
            },
            NfaState::Match {
                // 1
                symbol: 'b',
                next: StateId(3),
            },
            NfaState::Split {
                // 2
                out1: StateId(0),
                out2: StateId(1),
            },
            NfaState::Split {
                // 3
                out1: StateId(2),
                out2: StateId(7),
            },
            NfaState::Match {
                // 4
                symbol: 'b',
                next: StateId(5),
            },
            NfaState::Split {
                // 5
                out1: StateId(4),
                out2: StateId(7),
            },
            NfaState::Split {
                // 6
                out1: StateId(3),
                out2: StateId(5),
            },
            NfaState::Finish, // 7
        ];
        assert_eq!(nfa.states, expected);
        assert_eq!(nfa.entry, StateId(6));
    }

    #[test]
    fn test_aplus_bplus() {
        let tokens = tokenize("a+|b+".chars()).unwrap();
        let postfix = to_postfix(tokens).unwrap();
        let nfa = NfaBuilder::build(&postfix).unwrap();

        let expected = [
            NfaState::Match {
                // 0
                symbol: 'a',
                next: StateId(1),
            },
            NfaState::Split {
                // 1
                out1: StateId(0),
                out2: StateId(5),
            },
            NfaState::Match {
                // 2
                symbol: 'b',
                next: StateId(3),
            },
            NfaState::Split {
                // 3
                out1: StateId(2),
                out2: StateId(5),
            },
            NfaState::Split {
                // 4
                out1: StateId(0),
                out2: StateId(2),
            },
            NfaState::Finish, // 5
        ];

        assert_eq!(nfa.states, expected);
        assert_eq!(nfa.entry, StateId(4));
    }

    #[test]
    fn test_bastar() {
        let tokens = tokenize("ba*".chars()).unwrap();
        let postfix = to_postfix(tokens).unwrap();
        let nfa = NfaBuilder::build(&postfix).unwrap();

        let expected = [
            NfaState::Match {
                // 0
                symbol: 'b',
                next: StateId(2),
            },
            NfaState::Match {
                // 1
                symbol: 'a',
                next: StateId(2),
            },
            NfaState::Split {
                // 2
                out1: StateId(1),
                out2: StateId(3),
            },
            NfaState::Finish, // 3
        ];

        assert_eq!(nfa.states, expected);
        assert_eq!(nfa.entry, StateId(0));
    }

    #[test]
    fn test_baplus() {
        let tokens = tokenize("ba+".chars()).unwrap();
        let postfix = to_postfix(tokens).unwrap();
        let nfa = NfaBuilder::build(&postfix).unwrap();

        let expected = [
            NfaState::Match {
                // 0
                symbol: 'b',
                next: StateId(1),
            },
            NfaState::Match {
                // 1
                symbol: 'a',
                next: StateId(2),
            },
            NfaState::Split {
                // 2
                out1: StateId(1),
                out2: StateId(3),
            },
            NfaState::Finish, // 3
        ];

        assert_eq!(nfa.states, expected);
        assert_eq!(nfa.entry, StateId(0));
    }

    #[test]
    fn test_optional_lit() {
        let tokens = tokenize("ba?".chars()).unwrap();
        let postfix = to_postfix(tokens).unwrap();
        let nfa = NfaBuilder::build(&postfix).unwrap();

        let expected = [
            NfaState::Match {
                // 0
                symbol: 'b',
                next: StateId(2),
            },
            NfaState::Match {
                // 1
                symbol: 'a',
                next: StateId(3),
            },
            NfaState::Split {
                // 2
                out1: StateId(1),
                out2: StateId(3),
            },
            NfaState::Finish, // 3
        ];

        assert_eq!(nfa.states, expected);
        assert_eq!(nfa.entry, StateId(0));
    }

    #[test]
    fn test_optional_abcd() {
        let tokens = tokenize("abc?d".chars()).unwrap();
        let postfix = to_postfix(tokens).unwrap();
        let nfa = NfaBuilder::build(&postfix).unwrap();

        let expected = [
            NfaState::Match {
                // 0
                symbol: 'a',
                next: StateId(1),
            },
            NfaState::Match {
                // 1
                symbol: 'b',
                next: StateId(3),
            },
            NfaState::Match {
                // 2
                symbol: 'c',
                next: StateId(4),
            },
            NfaState::Split {
                // 3
                out1: StateId(2),
                out2: StateId(4),
            },
            NfaState::Match {
                // 4
                symbol: 'd',
                next: StateId(5),
            },
            NfaState::Finish, // 5
        ];

        assert_eq!(nfa.states, expected);
        assert_eq!(nfa.entry, StateId(0));
    }

    #[test]
    fn test_empty_regex_is_rejected() {
        let tokens = tokenize("".chars()).unwrap();
        let postfix = to_postfix(tokens).unwrap();

        let out = NfaBuilder::build(&postfix);
        assert!(matches!(out, Err(ParseError::EmptyRegex)));
    }

    #[test]
    fn test_dangling_pipe_is_rejected() {
        let tokens = tokenize("a|".chars()).unwrap();
        let postfix = to_postfix(tokens).unwrap();

        assert!(matches!(
            NfaBuilder::build(&postfix),
            Err(ParseError::MalformedRegex)
        ));
    }

    #[test]
    fn test_leading_quantifier_is_rejected() {
        let tokens = tokenize("*a".chars()).unwrap();
        let postfix = to_postfix(tokens).unwrap();

        assert!(matches!(
            NfaBuilder::build(&postfix),
            Err(ParseError::MalformedRegex)
        ));
    }
}
