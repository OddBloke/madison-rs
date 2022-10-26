#[macro_use]
extern crate rocket;

use rocket::serde::Deserialize;

use fapt::system::System;

use madison_rs::{do_madison, init_system};

#[derive(Deserialize)]
#[serde(crate = "rocket::serde")]
struct Config {
    sources_list: String,
}

struct MadisonState {
    system: System,
}

#[get("/?<package>")]
fn madison(
    package: String,
    state: &rocket::State<MadisonState>,
) -> Result<String, rocket::response::Debug<anyhow::Error>> {
    let system = &state.system;
    system.update()?;
    Ok(do_madison(package, system)?)
}

#[launch]
fn rocket() -> _ {
    let rocket = rocket::build();
    let figment = rocket.figment();
    let config: Config = figment.extract().expect("config");

    let system = init_system(config.sources_list).expect("fapt System init");

    rocket
        .mount("/", routes![madison])
        .manage(MadisonState { system })
}
