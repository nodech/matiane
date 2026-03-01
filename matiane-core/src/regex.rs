use thiserror::Error;

mod exec;
mod lexer;
mod parser;

pub struct Regex<'a> {
    src: &'a str,
    nfa: parser::Nfa,
}

#[derive(Debug, Error)]
pub enum RegexCompileError {
    #[error("Lexer failed: {0}")]
    LexError(#[from] lexer::LexError),
    #[error("Parse failed: {0}")]
    ParseError(#[from] parser::ParseError),
}

impl<'a> Regex<'a> {
    pub fn compile(raw_regex: &'a str) -> Result<Self, RegexCompileError> {
        let tokens = lexer::tokenize(raw_regex.chars())?;
        let postfix_tokens = lexer::topostfix(tokens)?;
        let nfa = parser::NfaBuilder::build(&postfix_tokens)?;

        Ok(Self {
            src: raw_regex,
            nfa,
        })
    }

    pub fn is_match(&self, hay: &str) -> bool {
        let exec = exec::ExecRegex::new(&self.nfa);

        exec.is_match(hay)
    }
}
