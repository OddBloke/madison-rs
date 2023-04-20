use madison_rs::{key_func, madison_cli};

#[tokio::main]
async fn main() {
    madison_cli::cli(&key_func::codename).await
}
