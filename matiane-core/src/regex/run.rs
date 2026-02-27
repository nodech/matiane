use super::parser::Nfa;
use super::parser::State;

pub(super) struct ExecRegex<'a> {
    nfa: &'a Nfa,
}

impl<'a> ExecRegex<'a> {
    pub(super) fn new(nfa: &'a Nfa) -> Self {
        ExecRegex { nfa }
    }

    pub(super) fn is_match(&self, hay: &str) -> bool {
        let mut states = vec![];
        let mut next_states = vec![];

        states.push(self.nfa.entry);

        for ch in hay.chars() {
            if !self.nfa.match_start {
                next_states.push(self.nfa.entry);
            }

            while let Some(state_id) = states.pop() {
                let state = self.nfa.states[state_id];

                match state {
                    State::Match { symbol, next } => {
                        if ch == symbol {
                            next_states.push(next);
                        }
                    }
                    State::Split { out1, out2 } => {
                        states.push(out1);
                        states.push(out2);
                    }
                    State::Finish => return true,
                    State::None => panic!("Unexpected state."),
                }
            }

            states.clear();
            std::mem::swap(&mut states, &mut next_states);
        }

        // final check
        while let Some(state_id) = states.pop() {
            let state = self.nfa.states[state_id];

            // println!("Final checking... {:?}", state);
            match state {
                State::Match { .. } => continue,
                State::Split { out1, out2 } => {
                    states.push(out1);
                    states.push(out2);
                }
                State::Finish => return true,
                _ => panic!("Unexpected state."),
            }
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use super::super::Regex;

    macro_rules! assert_all_matches {
        ($regex:expr, $array:expr) => {
            for hay in $array {
                assert!(
                    $regex.is_match(hay),
                    r#""{}" Must match "{}""#,
                    $regex.src,
                    hay
                );
            }
        };
    }

    macro_rules! assert_none_matches {
        ($regex:expr, $array:expr) => {
            for hay in $array {
                assert!(
                    !$regex.is_match(hay),
                    r#""{}" Must not match "{}""#,
                    $regex.src,
                    hay
                );
            }
        };
    }

    #[test]
    fn test_is_match_simple_ab() {
        let regex = Regex::compile("ab").unwrap();

        assert_all_matches!(regex, ["ab", "someab", "notabout", "aaaab"]);
        assert_none_matches!(regex, ["anotb", "", "aa"]);
    }

    #[test]
    fn test_is_match_simple_all() {
        let regex = Regex::compile("a*|b*").unwrap();

        assert_all_matches!(
            regex,
            [
                "aaaaakkk",
                "bbbaaakk",
                "kkkka",
                "xxx",
                "anythingcanmatch",
                "....",
                "",
            ]
        );
    }

    #[test]
    fn test_is_match_or() {
        let regex = Regex::compile("aa|cd|ee").unwrap();

        assert_all_matches!(
            regex,
            [
                "aa",
                "cd",
                "ee",
                "aa_inthebeginning",
                "inthe_aa_middle",
                "attheend_aa",
                "cd_inthebeginning",
                "inthe_cd_middle",
                "attheend_cd",
                "ee_inthebeginning",
                "inthe_ee_middle",
                "attheend_ee",
            ]
        );
        assert_none_matches!(regex, ["", "ab", "de", "efefef", "whatever"]);
    }

    #[test]
    fn test_is_this_exploding() {
        let regex = Regex::compile("a*a*a*a*a*a*aaa").unwrap();

        assert_all_matches!(regex, ["aaa", "aaaa",]);

        assert_none_matches!(regex, ["bbb", "bba",])
    }

    #[test]
    fn test_is_match_start() {
        let regex = Regex::compile("^aaa").unwrap();

        assert_all_matches!(regex, ["aaa", "aaammmm", "aaaaaaaa",]);

        assert_none_matches!(regex, ["baaaaa", "aabbaaa", "bbbaaaaabb",]);
    }
}
