use std::io::{BufRead, Write};

use validator::Validate;

#[cfg(debug_assertions)]
use crate::common::debug_setup;
use crate::common::error::{AppError, AppResult};
use crate::common::models::{LoginRequest, RegisterRequest, User};
use crate::common::services::auth_service;
use crate::common::state::AppState;

use super::commands::{self, Outcome};

pub(crate) async fn run(
    input: &mut dyn BufRead,
    output: &mut dyn Write,
    state: AppState,
) -> AppResult<()> {
    write_line(output, "Expense Tracker CLI (in-memory session)")?;

    let user = match authenticate(input, output, &state).await? {
        Some(user) => user,
        None => {
            write_line(output, "Bye!")?;
            return Ok(());
        }
    };
    write_line(output, "Type 'help' for available commands.")?;

    while let Some(line) = read_line(input, output, "expense> ")? {
        if line.is_empty() {
            continue;
        }

        let command = match commands::parse_command(&line) {
            Ok(command) => command,
            Err(e) => {
                write_line(output, &format!("Error: {e}"))?;
                continue;
            }
        };

        match commands::execute(&state, user.id, &command).await {
            Ok(Outcome::Output(text)) => write_line(output, &text)?,
            Ok(Outcome::Exit) => break,
            Err(e) => write_line(output, &format!("Error: {e}"))?,
        }
    }

    write_line(output, "Bye!")?;
    Ok(())
}

async fn authenticate(
    input: &mut dyn BufRead,
    output: &mut dyn Write,
    state: &AppState,
) -> AppResult<Option<User>> {
    loop {
        write_line(output, "1) Login")?;
        write_line(output, "2) Register")?;
        #[cfg(debug_assertions)]
        write_line(
            output,
            &format!("3) Continue as debug user ({})", debug_setup::DEBUG_EMAIL),
        )?;
        let Some(choice) = read_line(input, output, "Choose: ")? else {
            return Ok(None);
        };
        match choice.as_str() {
            "1" => {
                if let Some(user) = login_flow(input, output, state).await? {
                    return Ok(Some(user));
                }
            }
            "2" => {
                if let Some(user) = register_flow(input, output, state).await? {
                    return Ok(Some(user));
                }
            }
            #[cfg(debug_assertions)]
            "3" => {
                debug_setup::seed_debug_user(state).await;
                let Some(user) = state
                    .users
                    .read()
                    .await
                    .get(&debug_setup::DEBUG_USER_ID)
                    .cloned()
                else {
                    return Err(AppError::Internal(
                        "debug user missing after seeding".into(),
                    ));
                };
                write_line(output, &format!("Logged in as {}", user.email))?;
                return Ok(Some(user));
            }
            other => {
                #[cfg(debug_assertions)]
                let options = "1, 2 or 3";
                #[cfg(not(debug_assertions))]
                let options = "1 or 2";
                write_line(
                    output,
                    &format!("Error: unknown option '{other}', enter {options}"),
                )?;
            }
        }
    }
}

async fn login_flow(
    input: &mut dyn BufRead,
    output: &mut dyn Write,
    state: &AppState,
) -> AppResult<Option<User>> {
    let Some(email) = read_line(input, output, "Email: ")? else {
        return Ok(None);
    };
    let Some(password) = read_line(input, output, "Password: ")? else {
        return Ok(None);
    };

    let req = LoginRequest { email, password };
    if let Err(e) = req.validate() {
        write_line(output, &format!("Error: {}", AppError::from(e)))?;
        return Ok(None);
    }

    match auth_service::login(
        state,
        &req.email,
        &req.password,
        &state.jwt_secret,
        state.jwt_expires_in,
    )
    .await
    {
        Ok((_token, user)) => {
            write_line(output, &format!("Logged in as {}", user.email))?;
            Ok(Some(user))
        }
        Err(e) => {
            write_line(output, &format!("Error: {e}"))?;
            Ok(None)
        }
    }
}

async fn register_flow(
    input: &mut dyn BufRead,
    output: &mut dyn Write,
    state: &AppState,
) -> AppResult<Option<User>> {
    let Some(email) = read_line(input, output, "Email: ")? else {
        return Ok(None);
    };
    let Some(password) = read_line(input, output, "Password: ")? else {
        return Ok(None);
    };

    let req = RegisterRequest { email, password };
    if let Err(e) = req.validate() {
        write_line(output, &format!("Error: {}", AppError::from(e)))?;
        return Ok(None);
    }

    match auth_service::register(
        state,
        req.email,
        req.password,
        &state.jwt_secret,
        state.jwt_expires_in,
    )
    .await
    {
        Ok((_token, user)) => {
            write_line(output, &format!("Registered and logged in as {}", user.email))?;
            Ok(Some(user))
        }
        Err(e) => {
            write_line(output, &format!("Error: {e}"))?;
            Ok(None)
        }
    }
}

fn write_line(output: &mut dyn Write, text: &str) -> AppResult<()> {
    writeln!(output, "{text}").map_err(|e| AppError::Internal(format!("write failed: {e}")))
}

fn read_line(
    input: &mut dyn BufRead,
    output: &mut dyn Write,
    prompt: &str,
) -> AppResult<Option<String>> {
    write!(output, "{prompt}").map_err(|e| AppError::Internal(format!("write failed: {e}")))?;
    output
        .flush()
        .map_err(|e| AppError::Internal(format!("write failed: {e}")))?;
    let mut buf = String::new();
    match input.read_line(&mut buf) {
        Ok(0) => Ok(None),
        Ok(_) => Ok(Some(buf.trim().to_string())),
        Err(e) => Err(AppError::Internal(format!("read failed: {e}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::config::Config;

    async fn run_script(script: &str) -> String {
        let state = AppState::from_config(&Config {
            jwt_secret: "test_secret".into(),
            jwt_expires_in: 3600,
            port: 0,
        });
        let mut input = script.as_bytes();
        let mut output = Vec::new();
        run(&mut input, &mut output, state).await.unwrap();
        String::from_utf8(output).unwrap()
    }

    #[tokio::test]
    async fn test_register_stats_exit() {
        let out = run_script("2\nnew@example.com\npassword123\nstats\nexit\n").await;
        assert!(out.contains("Registered and logged in as new@example.com"));
        assert!(out.contains("No expenses found for the selected period."));
        assert!(out.contains("Bye!"));
    }

    #[tokio::test]
    async fn test_register_upload_stats() {
        let out = run_script(
            "2\nnew@example.com\npassword123\nupload tests/fixtures/monefy.csv\nstats\nexit\n",
        )
        .await;
        assert!(out.contains("Imported 41 expenses from tests/fixtures/monefy.csv"));
        assert!(out.contains("Total income"));
        assert!(out.contains("Bye!"));
    }

    #[tokio::test]
    async fn test_login_failure_then_register() {
        let out = run_script(
            "1\nnew@example.com\nwrongpass\n2\nnew@example.com\npassword123\nexit\n",
        )
        .await;
        assert!(out.contains("Error:"));
        assert!(out.contains("Registered and logged in as new@example.com"));
    }

    #[tokio::test]
    async fn test_unknown_command_continues() {
        let out =
            run_script("2\nnew@example.com\npassword123\nfoobar\nhelp\nexit\n").await;
        assert!(out.contains("unknown command 'foobar'"));
        assert!(out.contains("Commands:"));
        assert!(out.contains("Bye!"));
    }

    #[tokio::test]
    async fn test_eof_exits_cleanly() {
        let out = run_script("2\nnew@example.com\npassword123\n").await;
        assert!(out.contains("Registered and logged in as new@example.com"));
        assert!(out.contains("Bye!"));
    }

    #[cfg(debug_assertions)]
    #[tokio::test]
    async fn test_debug_user_option() {
        let out = run_script("3\nstats\nexit\n").await;
        assert!(out.contains("3) Continue as debug user (debug@example.com)"));
        assert!(out.contains("Logged in as debug@example.com"));
        assert!(out.contains("No expenses found for the selected period."));
        assert!(out.contains("Bye!"));
    }
}
