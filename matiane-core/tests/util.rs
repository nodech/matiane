use tempfile::{Builder, TempDir};

pub fn tmpdir(name: &str) -> TempDir {
    Builder::new()
        .prefix(&format!("matiane-core-{}", name))
        .rand_bytes(10)
        .tempdir()
        .unwrap()
}

#[macro_export]
macro_rules! json_lines {
    ($($e:tt),+ $(,)?) => {
        vec![$(serde_json::json!($e).to_string()),+].join("\n")
    };
}
