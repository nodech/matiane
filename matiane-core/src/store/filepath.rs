use chrono::{DateTime, NaiveDate, Utc};
use std::error::Error;
use std::fmt;
use std::path::PathBuf;

const DATE_FORMAT: &str = "%Y%m%d";
const EXTENSION: &str = "log";

#[derive(Debug, PartialEq)]
pub enum TryIntoFilenameError {
    Utf8Error,
    BadFileName,
    ExtensionMissing,
    IncorrectExtension,
}

impl fmt::Display for TryIntoFilenameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TryIntoFilenameError::BadFileName => {
                write!(f, "Path is not in date format")
            }
            TryIntoFilenameError::Utf8Error => {
                write!(f, "Filename is not correct utf8")
            }
            TryIntoFilenameError::ExtensionMissing => {
                write!(f, "Extension {} not found", EXTENSION)
            }
            TryIntoFilenameError::IncorrectExtension => {
                write!(f, "Incorrect extension")
            }
        }
    }
}

impl Error for TryIntoFilenameError {}

#[derive(Debug, PartialEq, Clone)]
pub(crate) struct Filepath {
    path: PathBuf,
    date: NaiveDate,
}

impl Filepath {
    pub fn set_path(&mut self, path: PathBuf) -> &mut Self {
        self.path = path;
        self
    }
}

impl From<Filepath> for PathBuf {
    fn from(filename: Filepath) -> Self {
        let formatted = filename.date.format(DATE_FORMAT);
        filename
            .path
            .join(formatted.to_string())
            .with_extension(EXTENSION)
    }
}

impl TryFrom<PathBuf> for Filepath {
    type Error = TryIntoFilenameError;

    fn try_from(path: PathBuf) -> Result<Filepath, TryIntoFilenameError> {
        match path.extension() {
            Some(ext) if ext == EXTENSION => {}
            Some(_) => return Err(TryIntoFilenameError::IncorrectExtension),
            None => return Err(TryIntoFilenameError::ExtensionMissing),
        }

        let filename = path
            .file_stem()
            .expect("already validated filename, so stem must exist")
            .to_str()
            .ok_or(TryIntoFilenameError::Utf8Error)?;

        let date = NaiveDate::parse_from_str(filename, DATE_FORMAT)
            .map_err(|_| TryIntoFilenameError::BadFileName)?;

        Ok(Self {
            path: path.with_file_name(""),
            date,
        })
    }
}

impl From<NaiveDate> for Filepath {
    fn from(date: NaiveDate) -> Self {
        Self {
            path: PathBuf::default(),
            date,
        }
    }
}

impl From<DateTime<Utc>> for Filepath {
    fn from(datetime: DateTime<Utc>) -> Self {
        datetime.date_naive().into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;
    use chrono::{DateTime, TimeZone, Utc};

    #[test]
    fn get_filename_by_name_test() -> Result<()> {
        #[derive(Debug)]
        struct TestCase {
            date: DateTime<Utc>,
            expected: PathBuf,
        }

        let dates = [
            TestCase {
                date: Utc.with_ymd_and_hms(1970, 1, 1, 0, 0, 0).unwrap(),
                expected: "19700101.log".into(),
            },
            TestCase {
                date: Utc.with_ymd_and_hms(1970, 1, 1, 23, 59, 59).unwrap(),
                expected: "19700101.log".into(),
            },
            TestCase {
                date: Utc.with_ymd_and_hms(2000, 1, 1, 0, 0, 0).unwrap(),
                expected: "20000101.log".into(),
            },
            TestCase {
                date: Utc.with_ymd_and_hms(2000, 1, 1, 5, 6, 7).unwrap(),
                expected: "20000101.log".into(),
            },
            TestCase {
                date: Utc.with_ymd_and_hms(2025, 12, 31, 23, 59, 59).unwrap(),
                expected: "20251231.log".into(),
            },
        ];

        for test in dates {
            let name: Filepath = test.date.into();
            let path: PathBuf = name.into();
            assert_eq!(path, test.expected, "{:?}", test);

            let name: Filepath = test.date.date_naive().into();
            let path: PathBuf = name.into();
            assert_eq!(path, test.expected, "{:?}", test);

            // None of the tests have prefix/path.
            let name: Filepath = test.expected.clone().try_into()?;
            let path: PathBuf = name.into();
            assert_eq!(path, test.expected, "{:?}", test);
        }

        Ok(())
    }

    #[test]
    fn filename_from_pathbuf() -> Result<()> {
        #[derive(Debug)]
        struct TestCase {
            source: PathBuf,
            expected: Result<Filepath, TryIntoFilenameError>,
        }

        let tests = [
            TestCase {
                source: "path/to/not.correct".into(),
                expected: Err(TryIntoFilenameError::IncorrectExtension),
            },
            TestCase {
                source: "path/is/fine-but-not-date.log".into(),
                expected: Err(TryIntoFilenameError::BadFileName),
            },
            TestCase {
                source: "path/with/noext".into(),
                expected: Err(TryIntoFilenameError::ExtensionMissing),
            },
            TestCase {
                source: "path/is/20260123.log".into(),
                expected: Ok(Filepath {
                    path: "path/is/".into(),
                    date: NaiveDate::from_ymd_opt(2026, 01, 23).unwrap(),
                }),
            },
        ];

        for test in tests {
            let name: Result<Filepath, TryIntoFilenameError> =
                test.source.clone().try_into();
            assert_eq!(name, test.expected, "{:?}", test);
        }

        Ok(())
    }

    #[test]
    #[cfg(unix)]
    fn filename_utf8_error() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;

        let path = PathBuf::from(OsStr::from_bytes(b"\xF0hello.log"));
        let result = Filepath::try_from(path);

        assert!(matches!(result, Err(TryIntoFilenameError::Utf8Error)));
    }

    #[test]
    fn filename_pathbuf_symmetry() -> Result<()> {
        let tests: [PathBuf; _] = [
            "20251231.log".into(),
            "/20251231.log".into(),
            "relative/path/20251231.log".into(),
            "/absolute/path/20251231.log".into(),
            "./20251231.log".into(),
            "../20251231.log".into(),
            "path/to/../other/20251231.log".into(),
            "path//double/20251231.log".into(),
            "path/./dot/20251231.log".into(),
        ];

        for test in tests {
            let file_path: Filepath = test.clone().try_into()?;
            let back_path: PathBuf = file_path.into();

            assert_eq!(back_path, test, "{:?} != {:?}", back_path, test);
        }

        Ok(())
    }
}
