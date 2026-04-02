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

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum NfaState {
    Split { out1: StateId, out2: StateId },
    Match { symbol: char, next: StateId },
    Finish,
    None,
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

#[derive(Debug, Clone, Copy)]
enum FragmentState {
    Match { symbol: char },
    Finish,
    Split { out1: FragmentId, out2: FragmentId },
    OptRepeat { out1: FragmentId },
    Optional { out1: FragmentId },
}

#[derive(Debug)]
struct Fragment {
    id: FragmentId,
    state: FragmentState,
    first: FragmentId,
    last: FragmentId,
    next: Option<FragmentId>,
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
    match_start: bool,
    match_end: bool,
    items: Vec<Fragment>,
    stack: Vec<FragmentId>,
}

impl NfaBuilder {
    fn next_frag_id(&self) -> FragmentId {
        FragmentId(self.items.len())
    }

    fn push_fragment(&mut self, frag: Fragment) {
        assert!(frag.id == self.next_frag_id());
        self.stack.push(frag.id);
        self.items.push(frag);
    }

    fn push(&mut self, id: FragmentId) {
        self.stack.push(id);
    }

    fn pop(&mut self) -> ParseResult<FragmentId> {
        self.stack.pop().ok_or(ParseError::MalformedRegex)
    }

    fn match_char(&mut self, ch: char) -> ParseResult<()> {
        let frag_id = self.next_frag_id();
        let fragment = Fragment {
            id: frag_id,
            state: FragmentState::Match { symbol: ch },
            first: frag_id,
            last: frag_id,
            next: None,
        };

        self.push_fragment(fragment);

        Ok(())
    }

    fn concat(&mut self) -> ParseResult<()> {
        let right = self.pop()?;
        let left = self.pop()?;

        // We continue chain from the last.
        let left_last = self.items[left].last;
        self.items[left_last].next = Some(right);

        // Update the first items (left) last.
        self.items[left].last = self.items[right].last;

        self.push(left);

        Ok(())
    }

    fn pipe(&mut self) -> ParseResult<()> {
        let right = self.pop()?;
        let left = self.pop()?;

        let frag_id = self.next_frag_id();
        let fragment = Fragment {
            id: frag_id,
            state: FragmentState::Split {
                out1: self.items[left].first,
                out2: self.items[right].first,
            },
            first: frag_id,
            last: frag_id,
            next: None,
        };

        self.push_fragment(fragment);

        Ok(())
    }

    fn star(&mut self) -> ParseResult<()> {
        let last = self.pop()?;

        let frag_id = self.next_frag_id();
        let fragment = Fragment {
            id: frag_id,
            state: FragmentState::OptRepeat { out1: last },
            first: frag_id,
            last: frag_id,
            next: None,
        };

        self.push_fragment(fragment);

        Ok(())
    }

    fn plus(&mut self) -> ParseResult<()> {
        let last = self.pop()?;

        let frag_id = self.next_frag_id();
        let fragment = Fragment {
            id: frag_id,
            state: FragmentState::OptRepeat { out1: last },
            first: frag_id,
            last: frag_id,
            next: None,
        };

        self.push(last);
        self.push_fragment(fragment);
        self.concat()?;

        Ok(())
    }

    fn question(&mut self) -> ParseResult<()> {
        let last = self.pop()?;

        let frag_id = self.next_frag_id();
        let fragment = Fragment {
            id: frag_id,
            state: FragmentState::Optional { out1: last },
            first: frag_id,
            last: frag_id,
            next: None,
        };

        self.push_fragment(fragment);

        Ok(())
    }

    fn finish(&mut self) -> ParseResult<()> {
        if self.stack.is_empty() {
            return Err(ParseError::EmptyRegex);
        }

        let frag_id = self.next_frag_id();
        self.push_fragment(Fragment {
            id: frag_id,
            state: FragmentState::Finish,
            first: frag_id,
            last: frag_id,
            next: None,
        });

        self.concat()?;

        Ok(())
    }

    fn build_frags(&mut self, tokens: &PostfixTokens) -> ParseResult<()> {
        for &el in tokens.iter() {
            match el {
                Token::Char(ch) => self.match_char(ch)?,
                Token::Concat => self.concat()?,
                Token::Pipe => self.pipe()?,
                Token::Star => self.star()?,
                Token::Plus => self.plus()?,
                Token::Question => self.question()?,
                _ => return Err(ParseError::UnsupportedToken(el)),
            }
        }

        self.finish()?;

        Ok(())
    }

    fn build_nfa(&mut self) -> ParseResult<Nfa> {
        let entry_id = self.pop()?;

        let mut nfa = Nfa {
            entry: self.items[entry_id].first.into(),
            match_start: self.match_start,
            match_end: self.match_end,
            states: vec![NfaState::None; self.items.len()],
        };

        let mut fragments = vec![entry_id];
        let mut visited = HashSet::<FragmentId>::new();

        while let Some(frag_id) = fragments.pop() {
            let Fragment { state, next, .. } = self.items[frag_id];

            let state_id: StateId = frag_id.into();

            if visited.contains(&frag_id) {
                continue;
            }

            visited.insert(frag_id);

            match state {
                FragmentState::Match { symbol } => {
                    let next = next.unwrap_or_else(|| {
                        unreachable!(
                            "Match fragment always has a next after finish"
                        )
                    });

                    fragments.push(next);

                    nfa.states[state_id] = NfaState::Match {
                        symbol,
                        next: next.into(),
                    }
                }
                FragmentState::Split { out1, out2 } => {
                    fragments.push(out1);
                    fragments.push(out2);

                    let last_out1 = self.items[out1].last;
                    self.items[last_out1].next = next;
                    let last_out2 = self.items[out2].last;
                    self.items[last_out2].next = next;

                    nfa.states[state_id] = NfaState::Split {
                        out1: out1.into(),
                        out2: out2.into(),
                    }
                }
                FragmentState::OptRepeat { out1 } => {
                    let next = next.unwrap_or_else(|| {
                        unreachable!("OptRepeat must always have a next.")
                    });

                    fragments.push(out1);
                    fragments.push(next);

                    let last_out1 = self.items[out1].last;
                    // Loop on itself.
                    self.items[last_out1].next = Some(frag_id);

                    nfa.states[state_id] = NfaState::Split {
                        out1: out1.into(),
                        out2: next.into(),
                    }
                }
                FragmentState::Optional { out1 } => {
                    let next = next.unwrap_or_else(|| {
                        unreachable!("Optional must always have a next.")
                    });

                    fragments.push(out1);
                    fragments.push(next);

                    let last_out = self.items[out1].last;
                    // Go to the next.
                    self.items[last_out].next = Some(next);

                    nfa.states[state_id] = NfaState::Split {
                        out1: out1.into(),
                        out2: next.into(),
                    }
                }
                FragmentState::Finish => {
                    nfa.states[state_id] = NfaState::Finish;
                }
            }
        }

        Ok(nfa)
    }

    pub(super) fn build(tokens: &PostfixTokens) -> ParseResult<Nfa> {
        let mut builder = NfaBuilder {
            match_start: tokens.match_start,
            match_end: tokens.match_end,
            items: vec![],
            stack: vec![],
        };

        builder.build_frags(tokens)?;
        builder.build_nfa()
    }
}

#[cfg(test)]
mod tests {
    use super::super::lexer::{tokenize, topostfix};
    use super::*;

    #[test]
    fn test_single_char_a() {
        let tokens = tokenize("a".chars()).unwrap();
        let postfix = topostfix(tokens).unwrap();
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
        let postfix = topostfix(tokens).unwrap();
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
        let postfix = topostfix(tokens).unwrap();
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
        let postfix = topostfix(tokens).unwrap();
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
        let postfix = topostfix(tokens).unwrap();
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
        let postfix = topostfix(tokens).unwrap();
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
        let postfix = topostfix(tokens).unwrap();
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
        let postfix = topostfix(tokens).unwrap();
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
        let postfix = topostfix(tokens).unwrap();
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
        let postfix = topostfix(tokens).unwrap();
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
        let postfix = topostfix(tokens).unwrap();
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
        let postfix = topostfix(tokens).unwrap();
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
        let postfix = topostfix(tokens).unwrap();
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
    fn test_empty_regex_is_rejected() {
        let tokens = tokenize("".chars()).unwrap();
        let postfix = topostfix(tokens).unwrap();

        assert!(matches!(
            NfaBuilder::build(&postfix),
            Err(ParseError::EmptyRegex)
        ));
    }

    #[test]
    fn test_dangling_pipe_is_rejected() {
        let tokens = tokenize("a|".chars()).unwrap();
        let postfix = topostfix(tokens).unwrap();

        assert!(matches!(
            NfaBuilder::build(&postfix),
            Err(ParseError::MalformedRegex)
        ));
    }

    #[test]
    fn test_leading_quantifier_is_rejected() {
        let tokens = tokenize("*a".chars()).unwrap();
        let postfix = topostfix(tokens).unwrap();

        assert!(matches!(
            NfaBuilder::build(&postfix),
            Err(ParseError::MalformedRegex)
        ));
    }
}
