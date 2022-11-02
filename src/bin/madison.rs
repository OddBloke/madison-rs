use madison_rs::{key_func, madison_cli};

fn main() {
    madison_cli::cli(&key_func::codename)
}
