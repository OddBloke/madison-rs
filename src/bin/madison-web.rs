use madison_rs::{key_func, madison_web};

#[rocket::main]
async fn main() -> Result<(), rocket::Error> {
    let _rocket = madison_web::rocket(&key_func::codename)
        .await
        .launch()
        .await?;
    Ok(())
}
