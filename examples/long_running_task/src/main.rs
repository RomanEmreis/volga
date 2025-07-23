//! Run with:
//!
//! ```no_rust
//! cargo run -p long_running_task
//! ```

use tokio::time::{interval, Duration};
use volga::{App, CancellationToken};

async fn long_running_task() {
    let mut interval = interval(Duration::from_millis(1000));
    loop {
        interval.tick().await;
        println!("doing something");
    }
}

async fn another_long_running_task(cancellation_token: CancellationToken) {
    let mut interval = interval(Duration::from_millis(1000));
    loop {
        tokio::select! {
            _ = cancellation_token.cancelled() => {
                println!("Task was cancelled");
                break;
            },
            _ = interval.tick() => {
                println!("doing something");
            }
        }
    }
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    // Start the server
    let mut app = App::new();

    // Example of a long-running task
    app.map_get("/long-task", |cancellation_token: CancellationToken| async move {
        let cancellation_token = cancellation_token.into_inner();
        tokio::select! {
            _ = cancellation_token.cancelled() => {
                println!("Task was cancelled");
            },
            _ = long_running_task() => ()
        }
        "done"
    });

    // Example of a long-running task with a spawned task
    app.map_get("/another-long-task", |cancellation_token: CancellationToken| async move {
        let long_running_task = tokio::task::spawn(async move {
            another_long_running_task(cancellation_token.clone()).await;
        });

        long_running_task.await.unwrap();

        "done"
    });

    app.run().await
}