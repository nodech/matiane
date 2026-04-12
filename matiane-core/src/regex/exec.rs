use crate::regex::parser::StateId;

use super::parser::Nfa;
use super::parser::NfaState;

pub(super) struct ExecRegex<'a> {
    nfa: &'a Nfa,
}

impl<'a> ExecRegex<'a> {
    pub(super) fn new(nfa: &'a Nfa) -> Self {
        ExecRegex { nfa }
    }

    fn add_state(&self, state_id: StateId, list: &mut Vec<StateId>) -> bool {
        let state = &self.nfa.states[state_id];

        match state {
            NfaState::MatchClass { .. } => {
                list.push(state_id);
                false
            }
            NfaState::Split { out1, out2 } => {
                let left = self.add_state(*out1, list);
                let right = self.add_state(*out2, list);

                left || right
            }
            NfaState::Finish => true,
        }
    }

    pub(super) fn is_match(&self, hay: &str) -> bool {
        let mut states = Vec::with_capacity(self.nfa.states.len());
        let mut next_states = Vec::with_capacity(self.nfa.states.len());

        let mut matched: bool = self.add_state(self.nfa.entry, &mut states);

        if matched && !self.nfa.match_end {
            return true;
        }

        for ch in hay.chars() {
            next_states.clear();
            matched = false;

            if !self.nfa.match_start {
                matched |= self.add_state(self.nfa.entry, &mut next_states);
            }

            for state_id in &states {
                let state = &self.nfa.states[*state_id];

                match state {
                    NfaState::MatchClass { class, next } => {
                        if class.matches(ch) {
                            matched |= self.add_state(*next, &mut next_states);
                        }
                    }
                    &NfaState::Split { .. } | &NfaState::Finish => {
                        unreachable!("Unreachable")
                    }
                }
            }

            if matched && !self.nfa.match_end {
                return true;
            }

            std::mem::swap(&mut states, &mut next_states);
        }

        matched
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
    fn test_is_match_one_repeat() {
        let regex = Regex::compile("fa+").unwrap();

        assert_all_matches!(
            regex,
            ["far", "faar", "#far#", "faarbb", "faaaaaar",]
        );
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

    #[test]
    fn test_is_match_last_simple() {
        let regex = Regex::compile("aaa$").unwrap();

        assert_all_matches!(regex, ["aaa", "mmmmaaa", "aaaaaaaa",]);
        assert_none_matches!(regex, ["aaaab", "aaabbaaab", "bbbaaaaabb",]);
    }

    #[test]
    fn test_is_match_start_to_end() {
        let regex = Regex::compile("^abbc+d$").unwrap();

        assert_all_matches!(regex, ["abbcd", "abbcccd", "abbcccccd",]);
        assert_none_matches!(
            regex,
            ["aabbcd", "aabbcccd", "aabbd", "abbcda", "abbcccda", "abbda",]
        )
    }

    #[test]
    fn test_is_match_optional() {
        let regex = Regex::compile("abc?d").unwrap();

        assert_all_matches!(regex, ["abcd", "abd", "aaabd", "----abcd------"]);
        assert_none_matches!(regex, ["acd", "aaa", "bbbbbbbbb"]);
    }

    #[test]
    fn test_is_match_range() {
        let regex = Regex::compile("[a-z]").unwrap();

        assert_all_matches!(regex, ["a", "b", "g", "z"]);
        assert_none_matches!(regex, ["0", "@@", "1"]);
    }

    #[test]
    fn test_is_match_last_opt_range() {
        let regex = Regex::compile("^abc[0-9]?$").unwrap();

        assert_all_matches!(
            regex,
            [
                "abc0", "abc1", "abc2", "abc3", "abc4", "abc5", "abc6", "abc7",
                "abc8", "abc9", "abc",
            ]
        );

        assert_none_matches!(regex, ["abcd", "mabc0", "abc0m"]);
    }

    #[test]
    fn test_is_match_neg_range() {
        let regex = Regex::compile("[^a-z]\\d\\D").unwrap();

        assert_all_matches!(regex, ["99d", "@1@"]);
        assert_none_matches!(regex, ["ab1", "bd2", "z91"])
    }

    #[test]
    fn test_is_match_any() {
        let regex = Regex::compile("...").unwrap();

        assert_all_matches!(regex, ["012", "abc", "#@!"]);
        assert_none_matches!(regex, ["0\n2", "a\rc"]);
    }

    #[test]
    fn test_is_match_negated_class() {
        let regex = Regex::compile(r"[^a]").unwrap();

        assert_all_matches!(regex, ["b", "z", "0"]);
        assert_none_matches!(regex, ["a", "aaa"]);
    }

    #[test]
    fn test_is_match_negated_escape_class() {
        let regex = Regex::compile(r"^\D$").unwrap();

        assert_all_matches!(regex, ["x", "_"]);
        assert_none_matches!(regex, ["7", "0"]);
    }

    #[test]
    fn test_is_match_dot() {
        let regex = Regex::compile(r"^.?$").unwrap();

        assert_all_matches!(regex, ["", "a"]);
        assert_none_matches!(regex, ["\n", "\r"]);
    }
}
