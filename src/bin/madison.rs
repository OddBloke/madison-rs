use figment::providers::{Format, Toml};
use figment::Figment;
use serde::Deserialize;

use madison_rs::{do_madison, init_system, key_func, MadisonConfig};

#[derive(Deserialize)]
struct CliConfig {
    global: MadisonConfig,
}

fn main() {
    let package = std::env::args().nth(1).expect("no package name given");
    let config: CliConfig = Figment::new()
        .merge(Toml::file("Rocket.toml"))
        .extract()
        .expect("reading Rocket.toml configuration");

    let system = init_system(config.global).expect("fapt System init");
    print!(
        "{}",
        do_madison(package, &system, &key_func::codename).expect("generating madison table")
    );
}
