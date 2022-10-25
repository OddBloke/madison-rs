#[macro_use]
extern crate rocket;

use std::cmp::Ordering;
use std::collections::HashMap;

use fapt::commands;
use fapt::sources_list;
use fapt::system::System;

const SOURCES_LIST: &str = "
deb http://ca.archive.ubuntu.com/ubuntu/ jammy main
deb http://ca.archive.ubuntu.com/ubuntu/ jammy-updates main
";

#[get("/?<package>")]
fn madison(package: String) -> &'static str {
    // Setup the system
    let mut system = System::cache_only().unwrap();
    commands::add_builtin_keys(&mut system);
    commands::add_sources_entries_from_str(&mut system, SOURCES_LIST).unwrap();
    system.set_arches(vec!["amd64"]);
    system.update().unwrap();

    // Collect all the versions
    let mut versions: HashMap<String, String> = HashMap::new();
    for downloaded_list in system.listings().unwrap() {
        for section in system.open_listing(&downloaded_list).unwrap() {
            let pkg = section.unwrap().as_pkg().unwrap();
            if let Some(bin) = pkg.as_bin() {
                let resolved_source = if let Some(bin_src) = &bin.source {
                    bin_src
                } else {
                    &pkg.name
                };
                if resolved_source == &package {
                    let key = downloaded_list.release.req.codename.to_owned();
                    if let Some(current_value) = versions.get_mut(&key) {
                        if deb_version::compare_versions(current_value, &pkg.version)
                            == Ordering::Greater
                        {
                            *current_value = pkg.version.to_owned();
                        }
                    } else {
                        versions.insert(key, pkg.version.to_owned());
                    }
                }
            }
        }
    }
    println!("{:?}", versions);

    "Hello, world!"
}

#[launch]
fn rocket() -> _ {
    rocket::build().mount("/", routes![madison])
}
