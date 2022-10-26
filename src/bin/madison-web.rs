#[macro_use]
extern crate rocket;

use fapt::system::System;

use madison_rs::{do_madison, init_system, MadisonConfig};

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
    let config: MadisonConfig = figment.extract().expect("config");

    let system = init_system(config).expect("fapt System init");

    rocket
        .mount("/", routes![madison])
        .manage(MadisonState { system })
}
