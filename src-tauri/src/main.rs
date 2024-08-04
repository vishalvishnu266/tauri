// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Arc;
use broadcast::channel;
use rusqlite::{Connection, Result};
use serde::{Deserialize, Serialize};
use tauri::State;
use tokio::sync::{broadcast, mpsc, Mutex};
use tokio::task;
use tokio::time::{sleep, Duration};
#[derive(Debug,Clone)]
enum DbRequest {
    Execute(String),
    Query(String,broadcast::Sender<Result<String, String>>),
}

#[derive(Debug, Serialize, Deserialize)]
struct UserInfo {
    name: String,
    email: Option<String>,
    age: Option<String>,
}
pub struct SharedState {
    tx: broadcast::Sender<String>,
    db_request_tx: broadcast::Sender<DbRequest>,
}

#[tauri::command]
fn hello(value: UserInfo) -> String {
    // Log the extracted data
    println!("Parsed name: {}", value.name);
    println!("Parsed email: {:?}", value.email);
    println!("Parsed age: {:?}", value.age);

    // Prepare the response
    let response = format!(
        "Hello, {}! Your email is {} and you are {} years old. You've been greeted from Rust!",
        value.name,
        value.email.as_deref().unwrap_or("not provided"),
        value.age.as_deref().unwrap_or("not provided")
    );

    // Log the response being sent
    println!("Sending response: {}", response);

    response
}

#[tauri::command]
async fn greet(_value: UserInfo, state: State<'_, Arc<Mutex<SharedState>>>) -> Result<String, ()> {
    let (response_tx, mut response_rx) =channel(1);
    let request = DbRequest::Query(
        "SELECT name FROM users ".to_string(),
        response_tx,
    );
    let shared_state = state.lock().await;


    // Send request and await response
    let _ = shared_state.db_request_tx.send(request);

    let user_name = match response_rx.recv().await {
        Ok(name) => name.unwrap(),
        _ => "Unknown".to_string(),
    };

    let message = format!("Hello, {:?}! You've been greeted from Rust!", user_name);

    // Assuming tx is another channel for sending messages
    let tx = shared_state.tx.clone();
    let _ = tx.send(message.clone());

    Ok(message)
}

#[tauri::command]
fn noinput() -> String {
    "Hello, You've been greeted from Rust!".to_string()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // enable_wal_mode("your-database-file.db").await?;


    // Retrieve and print the row
    let (tx, _rx) = channel(16); // Buffer size of 16 messages
    let (db_request_tx, mut db_request_rx) = channel(16);
    let shared_state = Arc::new(Mutex::new(SharedState {
        tx,
        db_request_tx
    }));

    // tokio::spawn(async {
    //     loop {
    //         // Print a message
    //         println!("This message prints every 3 seconds.");
    //
    //         // Sleep for 3 seconds
    //         sleep(Duration::from_secs(3)).await;
    //     }
    // });
    tokio::spawn(async move {
        let conn = Connection::open("/home/x/new.db").unwrap();
        conn.execute(
            "CREATE TABLE IF NOT EXISTS users (
                name TEXT PRIMARY KEY,
                email TEXT,
                age TEXT
            )",
            [],
        ).expect("Error creating table");

        while let Ok(request) = db_request_rx.recv().await {
            match request {
                DbRequest::Execute(sql) => {
                    conn.execute(&sql, []).expect("Error executing SQL");
                }
                DbRequest::Query(query, sender) => {
                    println!("{}",query);
                    let mut stmt = conn.prepare(&query).unwrap();
                    let result: Result<String, _> = stmt.query_row([], |row| row.get(0))
                        .map_err(|e| e.to_string());
                    match &result {
                        Ok(value) => {
                            println!("Query result: {}", value);
                        }
                        Err(e) => {
                            eprintln!("Query error: {}", e);
                        }
                    }
                    let _ = sender.send(result);
                }
            }
        }
    });

    // Message receiver task
    let shared_state_clone = shared_state.clone();
    tokio::spawn(async move {
        loop {
            let mut rx = {
                let guard = shared_state_clone.lock().await;
                guard.tx.subscribe()
            };

            match rx.recv().await {
                Ok(message) => println!("Received: {}", message),
                Err(e) => eprintln!("Error receiving message: {}", e),
            }
        }
    });

    tauri::async_runtime::set(tokio::runtime::Handle::current());

    tauri::Builder::default()
        .manage(shared_state)
        .invoke_handler(tauri::generate_handler![greet, hello, noinput])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
    Ok(())
}

async fn enable_wal_mode(db_path: &str) -> Result<()> {
    task::block_in_place(move || {
        // Open a connection to the SQLite database
        let conn = Connection::open(db_path)?;

        // Enable WAL mode
        let journal_mode: String =
            conn.query_row("PRAGMA journal_mode=WAL;", [], |row| row.get(0))?;

        // Print the current journal mode to confirm it's set to WAL
        println!("Journal mode: {}", journal_mode);

        Ok(())
    })
}
