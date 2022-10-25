#[macro_use]
extern crate rocket;

use std::cmp::Ordering;
use std::collections::HashMap;

use fapt::commands;
use fapt::sources_list;
use fapt::system::System;

#[get("/")]
fn madison() -> &'static str {
    // Setup the system
    let mut system = System::cache_only().unwrap();
    commands::add_builtin_keys(&mut system);
    system.add_sources_entries(vec![sources_list::Entry {
        src: false,
        url: "http://ca.archive.ubuntu.com/ubuntu/".to_string(),
        suite_codename: "jammy".to_string(),
        components: vec!["main".to_string()],
        arch: None,
    }]);
    system.add_sources_entries(vec![sources_list::Entry {
        src: false,
        url: "http://ca.archive.ubuntu.com/ubuntu/".to_string(),
        suite_codename: "jammy-updates".to_string(),
        components: vec!["main".to_string()],
        arch: None,
    }]);
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
                if resolved_source == "python3-defaults" {
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
