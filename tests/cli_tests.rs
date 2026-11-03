use std::collections::BTreeSet;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn temp_db_url() -> (String, PathBuf) {
    let path =
        std::env::temp_dir().join(format!("expense_tracker_test_{}.db", uuid::Uuid::new_v4()));
    (format!("sqlite:{}", path.display()), path)
}

fn cleanup_db(path: &Path) {
    let _ = std::fs::remove_file(path);
    for suffix in ["-wal", "-shm", "-journal"] {
        let _ = std::fs::remove_file(format!("{}{suffix}", path.display()));
    }
}

fn run_cli_with_db(args: &[&str], script: &str, db_url: &str) -> (String, String, Option<i32>) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_expense_tracker"))
        .args(args)
        .env("DATABASE_URL", db_url)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(script.as_bytes()).unwrap();
    drop(stdin);

    let output = child.wait_with_output().unwrap();
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
        output.status.code(),
    )
}

fn run_cli(args: &[&str], script: &str) -> (String, String, Option<i32>) {
    let (db_url, db_path) = temp_db_url();
    let result = run_cli_with_db(args, script, &db_url);
    cleanup_db(&db_path);
    result
}

fn fixture_path() -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/monefy.csv")
        .to_str()
        .unwrap()
        .to_string()
}

async fn seed_fixture_rates(db_url: &str) {
    let pool = expense_tracker::common::db::connect(db_url).await.unwrap();
    let expenses = expense_tracker::common::services::expense_service::parse_csv(
        include_str!("fixtures/monefy.csv"),
        uuid::Uuid::new_v4(),
    )
    .unwrap();
    let dates: BTreeSet<chrono::NaiveDate> = expenses.iter().map(|e| e.date).collect();
    for date in dates {
        sqlx::query(
            "INSERT OR REPLACE INTO nbrb_rates (date, currency, base_currency, rate)
             VALUES (?, 'USD', 'BYN', '2.8230')",
        )
        .bind(date)
        .execute(&pool)
        .await
        .unwrap();
    }
    pool.close().await;
}

#[tokio::test]
async fn cli_happy_path() {
    let (db_url, db_path) = temp_db_url();
    seed_fixture_rates(&db_url).await;
    let script = format!(
        "2\nuser@example.com\npassword123\nupload {}\nstats\nexit\n",
        fixture_path()
    );
    let (stdout, _stderr, code) = run_cli_with_db(&["cli"], &script, &db_url);
    assert_eq!(code, Some(0));
    assert!(stdout.contains("Registered and logged in as user@example.com"));
    assert!(stdout.contains("Imported 41 expenses"));
    assert!(stdout.contains("Converted 40 expenses to USD"));
    assert!(stdout.contains("Total income"));
    assert!(stdout.contains("Bye!"));
    cleanup_db(&db_path);
}

#[test]
fn cli_wrong_password_then_register() {
    let script = "1\nuser@example.com\nwrongpass\n2\nuser@example.com\npassword123\nexit\n";
    let (stdout, _stderr, code) = run_cli(&["cli"], script);
    assert_eq!(code, Some(0));
    assert!(stdout.contains("Error:"));
    assert!(stdout.contains("Registered and logged in as user@example.com"));
}

#[test]
fn cli_upload_missing_file() {
    let script = "2\nuser@example.com\npassword123\nupload /nonexistent/x.csv\nexit\n";
    let (stdout, _stderr, code) = run_cli(&["cli"], script);
    assert_eq!(code, Some(0));
    assert!(stdout.contains("cannot read"));
}

#[test]
fn cli_unknown_argument_exits_nonzero() {
    let (_stdout, stderr, code) = run_cli(&["bogus"], "");
    assert_eq!(code, Some(2));
    assert!(stderr.contains("Unknown argument"));
}

#[cfg(debug_assertions)]
#[test]
fn cli_debug_user_option() {
    let (stdout, _stderr, code) = run_cli(&["cli"], "3\nstats\nexit\n");
    assert_eq!(code, Some(0));
    assert!(stdout.contains("Logged in as debug@example.com"));
    assert!(stdout.contains("No expenses found for the selected period."));
}

#[tokio::test]
async fn cli_data_persists_across_runs() {
    let (db_url, db_path) = temp_db_url();
    seed_fixture_rates(&db_url).await;

    let script = format!(
        "2\nuser@example.com\npassword123\nupload {}\nexit\n",
        fixture_path()
    );
    let (stdout, _stderr, code) = run_cli_with_db(&["cli"], &script, &db_url);
    assert_eq!(code, Some(0));
    assert!(stdout.contains("Imported 41 expenses"));
    assert!(stdout.contains("Converted 40 expenses to USD"));

    let (stdout, _stderr, code) = run_cli_with_db(
        &["cli"],
        "1\nuser@example.com\npassword123\nstats\nexit\n",
        &db_url,
    );
    assert_eq!(code, Some(0));
    assert!(stdout.contains("Logged in as user@example.com"));
    assert!(stdout.contains("Total income"));
    assert!(!stdout.contains("No expenses found"));

    cleanup_db(&db_path);
}

#[tokio::test]
async fn cli_reupload_dedup() {
    let (db_url, db_path) = temp_db_url();
    seed_fixture_rates(&db_url).await;

    let script = format!(
        "2\nuser@example.com\npassword123\nupload {}\nexit\n",
        fixture_path()
    );
    let (stdout, _stderr, code) = run_cli_with_db(&["cli"], &script, &db_url);
    assert_eq!(code, Some(0));
    assert!(stdout.contains("Imported 41 expenses"));
    assert!(!stdout.contains("duplicates"));

    let script = format!(
        "1\nuser@example.com\npassword123\nupload {}\nstats\nexit\n",
        fixture_path()
    );
    let (stdout, _stderr, code) = run_cli_with_db(&["cli"], &script, &db_url);
    assert_eq!(code, Some(0));
    assert!(stdout.contains("Imported 0 expenses"));
    assert!(stdout.contains("Skipped 41 duplicates (already imported)"));
    assert!(stdout.contains("Total income"));

    cleanup_db(&db_path);
}
