//! Пересобирать при изменении миграций (sqlx::migrate! встраивает их при компиляции).
fn main() {
    println!("cargo:rerun-if-changed=migrations");
}
