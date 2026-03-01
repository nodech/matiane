use super::lexer::PostfixTokens;
use super::lexer::Token;
use std::collections::HashSet;
use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum ParseError {
    #[error("Lexer failed: {0}")]
    LexerError(#[from] super::lexer::LexError),

    #[error("Token not implemented {0:?}")]
    UnexpectedToken(Token),

    #[error("Invalid syntax")]
    MalformedRegex,
}

pub type ParseResult<T> = Result<T, ParseError>;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct StateId(usize);

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum State {
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
    pub(super) states: Vec<State>,
}

impl std::ops::Index<StateId> for Vec<State> {
    type Output = State;

    fn index(&self, index: StateId) -> &Self::Output {
        &self[index.0]
    }
}

impl std::ops::IndexMut<StateId> for Vec<State> {
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
    Concat,
    Accept,
    Split { out1: FragmentId, out2: FragmentId },
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
    entry: FragmentId,
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

        let frag_id = self.next_frag_id();
        let fragment = Fragment {
            id: frag_id,
            state: FragmentState::Concat,
            first: self.items[left].first,
            last: self.items[right].last,
            next: None,
        };

        // first last would be the same
        let left_last = self.items[left].last;
        self.items[left_last].next = Some(right);
        self.items[left_last].last = right;

        // let left_last = self.items[left].last;
        // let right_first = self.items[right].first;
        // let right_last = self.items[right].last;
        //
        // self.items[left_last].next = Some(right_first);
        // self.items[left_last].last = right_last;

        self.push_fragment(fragment);

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
            state: FragmentState::Optional { out1: last },
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
            state: FragmentState::Optional { out1: last },
            first: frag_id,
            // first: self.items[last].first,
            last: frag_id,
            next: None,
        };

        self.push(last);
        self.push_fragment(fragment);
        self.concat()?;

        Ok(())
    }

    fn finish(&mut self) -> ParseResult<()> {
        let frag_id = self.next_frag_id();
        self.push_fragment(Fragment {
            id: frag_id,
            state: FragmentState::Accept,
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
                _ => return Err(ParseError::UnexpectedToken(el)),
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
            states: vec![State::None; self.items.len()],
        };

        let mut fragments = vec![entry_id];
        let mut visited = HashSet::<FragmentId>::new();

        while let Some(frag_id) = fragments.pop() {
            let Fragment {
                state,
                first,
                // last,
                next,
                ..
            } = self.items[frag_id];

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

                    nfa.states[state_id] = State::Match {
                        symbol,
                        next: next.into(),
                    }
                }
                FragmentState::Concat => {
                    nfa.states[state_id] = State::None;
                    fragments.push(first);
                }
                FragmentState::Split { out1, out2 } => {
                    fragments.push(out1);
                    fragments.push(out2);

                    let last_out1 = self.items[out1].last;
                    self.items[last_out1].next = next;
                    let last_out2 = self.items[out2].last;
                    self.items[last_out2].next = next;

                    nfa.states[state_id] = State::Split {
                        out1: out1.into(),
                        out2: out2.into(),
                    }
                }
                FragmentState::Optional { out1 } => {
                    let next = next.unwrap_or_else(|| {
                        unreachable!("Optional must always have a next.")
                    });

                    fragments.push(out1);
                    fragments.push(next);

                    let last_out1 = self.items[out1].last;
                    self.items[last_out1].next = Some(frag_id);

                    nfa.states[state_id] = State::Split {
                        out1: out1.into(),
                        out2: next.into(),
                    }
                }
                FragmentState::Accept => {
                    nfa.states[state_id] = State::Finish;
                }
            }
        }

        Ok(nfa)
    }

    pub(super) fn build(tokens: &PostfixTokens) -> ParseResult<Nfa> {
        let mut builder = NfaBuilder {
            match_start: tokens.match_start,
            match_end: tokens.match_end,
            entry: FragmentId(0),
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
            State::Match {
                symbol: 'a',
                next: StateId(1),
            },
            State::Finish,
            State::None,
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
            State::Match {
                symbol: 'a',
                next: StateId(1),
            },
            State::Match {
                symbol: 'b',
                next: StateId(3),
            },
            State::None,
            State::Match {
                symbol: 'c',
                next: StateId(5),
            },
            State::None,
            State::Finish,
            State::None,
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
            State::Match {
                symbol: 'a',
                next: StateId(1),
            },
            State::Match {
                symbol: 'b',
                next: StateId(3),
            },
            State::None,
            State::Match {
                symbol: 'c',
                next: StateId(5),
            },
            State::None,
            State::Match {
                symbol: 'd',
                next: StateId(7),
            },
            State::None,
            State::Finish,
            State::None,
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
            State::Match {
                // 0
                symbol: 'a',
                next: StateId(1),
            },
            State::Match {
                // 1
                symbol: 'b',
                next: StateId(5),
            },
            State::None, // 2
            State::Match {
                // 3
                symbol: 'd',
                next: StateId(5),
            },
            State::Split {
                // 4
                out1: StateId(0),
                out2: StateId(3),
            },
            State::Finish, // 5
            State::None,   // 6
        ];

        assert_eq!(nfa.states, expected);
        assert_eq!(nfa.entry, StateId(4));
    }

    #[test]
    fn test_ab_or_cd() {
        let tokens = tokenize("ab|cd".chars()).unwrap();
        let postfix = topostfix(tokens).unwrap();
        let nfa = NfaBuilder::build(&postfix).unwrap();

        let expected = vec![
            State::Match {
                // 0
                symbol: 'a',
                next: StateId(1),
            },
            State::Match {
                // 1
                symbol: 'b',
                next: StateId(7),
            },
            State::None, // 2
            State::Match {
                // 3
                symbol: 'c',
                next: StateId(4),
            },
            State::Match {
                // 4
                symbol: 'd',
                next: StateId(7),
            },
            State::None, // 5
            State::Split {
                // 6
                out1: StateId(0),
                out2: StateId(3),
            },
            State::Finish, // 7
            State::None,   // 8
        ];
        assert_eq!(nfa.states, expected);
        assert_eq!(nfa.entry, StateId(6));
    }

    #[test]
    fn test_aorc_withd_or_io() {
        let tokens = tokenize("(a|c)d|io".chars()).unwrap();
        let postfix = topostfix(tokens).unwrap();
        let nfa = NfaBuilder::build(&postfix).unwrap();

        let expected = vec![
            State::Match {
                // 0
                symbol: 'a',
                next: StateId(3),
            },
            State::Match {
                // 1
                symbol: 'c',
                next: StateId(3),
            },
            State::Split {
                // 2
                out1: StateId(0),
                out2: StateId(1),
            },
            State::Match {
                // 3
                symbol: 'd',
                next: StateId(9),
            },
            State::None, // 4
            State::Match {
                // 5
                symbol: 'i',
                next: StateId(6),
            },
            State::Match {
                // 6
                symbol: 'o',
                next: StateId(9),
            },
            State::None, // 7
            State::Split {
                // 8
                out1: StateId(2),
                out2: StateId(5),
            },
            State::Finish, // 9
            State::None,
        ];

        assert_eq!(nfa.states, expected);
        assert_eq!(nfa.entry, StateId(8));
    }

    #[test]
    fn test_aorb_concat_cord() {
        let tokens = tokenize("(a|b)(c|d)".chars()).unwrap();
        let postfix = topostfix(tokens).unwrap();
        let nfa = NfaBuilder::build(&postfix).unwrap();

        let expected = vec![
            State::Match {
                symbol: 'a',
                next: StateId(5),
            },
            State::Match {
                symbol: 'b',
                next: StateId(5),
            },
            State::Split {
                out1: StateId(0),
                out2: StateId(1),
            },
            State::Match {
                symbol: 'c',
                next: StateId(7),
            },
            State::Match {
                symbol: 'd',
                next: StateId(7),
            },
            State::Split {
                out1: StateId(3),
                out2: StateId(4),
            },
            State::None,
            State::Finish,
            State::None,
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
            State::Match {
                // 0
                symbol: 'a',
                next: StateId(1),
            },
            State::Split {
                // 1
                out1: StateId(0),
                out2: StateId(5),
            },
            State::Match {
                // 2
                symbol: 'b',
                next: StateId(3),
            },
            State::Split {
                // 3
                out1: StateId(2),
                out2: StateId(5),
            },
            State::Split {
                // 4
                out1: StateId(1),
                out2: StateId(3),
            },
            State::Finish, // 5
            State::None,
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
            State::Match {
                // 0
                symbol: 'a',
                next: StateId(3),
            },
            State::Match {
                // 1
                symbol: 'b',
                next: StateId(3),
            },
            State::Split {
                // 2
                out1: StateId(0),
                out2: StateId(1),
            },
            State::Split {
                // 3
                out1: StateId(2),
                out2: StateId(7),
            },
            State::Match {
                // 4
                symbol: 'b',
                next: StateId(5),
            },
            State::Split {
                // 5
                out1: StateId(4),
                out2: StateId(7),
            },
            State::Split {
                // 6
                out1: StateId(3),
                out2: StateId(5),
            },
            State::Finish, // 7
            State::None,
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
            State::Match {
                // 0
                symbol: 'a',
                next: StateId(1),
            },
            State::Split {
                // 1
                out1: StateId(0),
                out2: StateId(7),
            },
            State::None, // 2
            State::Match {
                // 3
                symbol: 'b',
                next: StateId(4),
            },
            State::Split {
                // 4
                out1: StateId(3),
                out2: StateId(7),
            },
            State::None, // 5
            State::Split {
                // 6
                out1: StateId(0),
                out2: StateId(3),
            },
            State::Finish, // 7
            State::None,
        ];

        assert_eq!(nfa.states, expected);
        assert_eq!(nfa.entry, StateId(6));
    }

    #[test]
    fn test_bastar() {
        let tokens = tokenize("ba*".chars()).unwrap();
        let postfix = topostfix(tokens).unwrap();
        let nfa = NfaBuilder::build(&postfix).unwrap();

        let expected = [
            State::Match {
                // 0
                symbol: 'b',
                next: StateId(2),
            },
            State::Match {
                // 1
                symbol: 'a',
                next: StateId(2),
            },
            State::Split {
                // 2
                out1: StateId(1),
                out2: StateId(4),
            },
            State::None,   // 3
            State::Finish, // 4
            State::None,   // 5
        ];

        assert_eq!(nfa.states, expected);
    }

    #[test]
    fn test_baplus() {
        let tokens = tokenize("ba+".chars()).unwrap();
        let postfix = topostfix(tokens).unwrap();
        let nfa = NfaBuilder::build(&postfix).unwrap();

        let expected = [
            State::Match {
                // 0
                symbol: 'b',
                next: StateId(1),
            },
            State::Match {
                // 1
                symbol: 'a',
                next: StateId(2),
            },
            State::Split {
                // 2
                out1: StateId(1),
                out2: StateId(4),
            },
            State::None,   // 3
            State::Finish, // 4
            State::None,   // 5
        ];

        assert_eq!(nfa.states, expected);
    }
}
