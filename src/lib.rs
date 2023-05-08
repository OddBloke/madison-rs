#[macro_use]
extern crate rocket;

use std::cmp::Ordering;
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::collections::HashSet;
use std::fs::File;
use std::io::BufReader;

use log::info;

use fapt::commands;
use fapt::sources_list;
use fapt::system::System;

use tabled::{builder::Builder, settings::Style};

use serde::Deserialize;

use rayon::prelude::*;

#[derive(Deserialize)]
pub struct MadisonConfig {
    pub sources_list: String,
    pub extra_key_paths: Vec<String>,
    pub arches: Vec<String>,
}

pub async fn init_system(config: MadisonConfig) -> Result<System, anyhow::Error> {
    // Setup the system
    let mut system = System::cache_only()?;
    for path in config.extra_key_paths {
        system.add_keys_from(File::open(path)?)?;
    }
    commands::add_builtin_keys(&mut system);
    system.add_sources_entries(sources_list::read(BufReader::new(
        File::open(&config.sources_list).unwrap(),
    ))?);

    system.set_arches(config.arches);
    system.update().await?;
    Ok(system)
}

pub fn do_madison(
    packages: Vec<String>,
    system: &System,
    key_func: &key_func::KeyFunc,
    suite: Option<String>,
) -> Result<String, anyhow::Error> {
    // Collect all the versions
    let versions: Vec<Vec<_>> = system
        .listings()?
        .par_iter()
        .map(|downloaded_list| -> Result<_, anyhow::Error> {
            let key = key_func(downloaded_list);
            if let Some(suite) = suite.as_ref() {
                if &key != suite {
                    return Ok(vec![]);
                }
            }
            let mut versions: HashMap<_, (String, HashSet<_>)> = HashMap::new();
            for section in system.open_listing(downloaded_list)? {
                let pkg = section?.as_pkg()?;
                if let Some(bin) = pkg.as_bin() {
                    let mut pkg_type = None;
                    if let Some(source_pkg_name) = bin.source.as_ref() {
                        if packages.contains(&&source_pkg_name) {
                            pkg_type = Some((source_pkg_name.clone(), "source".to_string()))
                        };
                    };
                    if pkg_type == None && packages.contains(&&pkg.name) {
                        pkg_type = downloaded_list.listing.arch.as_ref().map_or(
                            Some((pkg.name.to_string(), "unknown!".to_string())),
                            |arch| Some((pkg.name.to_string(), arch.to_owned())),
                        )
                    };
                    if let Some((pkg_name, pkg_type)) = pkg_type {
                        match versions.entry(pkg_name) {
                            Entry::Occupied(mut o) => {
                                let (current_version, types) = o.get_mut();
                                types.insert(pkg_type);
                                if deb_version::compare_versions(&pkg.version, current_version)
                                    == Ordering::Greater
                                {
                                    *current_version = pkg.version
                                }
                            }
                            Entry::Vacant(o) => {
                                o.insert((pkg.version, HashSet::from([pkg_type])));
                            }
                        }
                    }
                }
            }
            Ok(versions
                .iter()
                .map(|(package_name, (version, types))| {
                    (
                        package_name.clone(),
                        key.clone(),
                        version.clone(),
                        types.clone(),
                    )
                })
                .collect())
        })
        .collect::<Result<_, _>>()?;
    info!("{:?}", versions);

    let mut merged_versions: HashMap<String, HashMap<(String, String), HashSet<String>>> =
        HashMap::new();
    for (package, codename, codename_version, types) in versions.into_iter().flatten() {
        let pkg_merged_versions = merged_versions.entry(package).or_insert(HashMap::new());
        let key = (codename, codename_version);
        if let Some(current_value) = pkg_merged_versions.get_mut(&key) {
            (*current_value).extend(types.to_owned());
        } else {
            pkg_merged_versions.insert(key, types.to_owned());
        }
    }

    let mut merged_vecs: HashMap<_, Vec<_>> = merged_versions
        .into_par_iter()
        .map(|(package, entries)| {
            let mut merged_vec = entries.into_iter().collect::<Vec<_>>();
            merged_vec.sort_by(|((codename1, v1), _), ((codename2, v2), _)| {
                match deb_version::compare_versions(v1, v2) {
                    Ordering::Equal => codename1.cmp(codename2),
                    other => other,
                }
            });
            (package, merged_vec)
        })
        .collect();

    let mut output_builder = Builder::default();
    for package in packages {
        let merged_vec = if let Some(merged_vec) = merged_vecs.remove(&package) {
            merged_vec
        } else {
            continue;
        };
        for ((codename, codename_version), mut types) in merged_vec {
            // Start with "source", append sorted architectures, join with ", "
            let mut type_parts: Vec<_> = types.take("source").into_iter().collect();
            let mut arch_parts = types.into_iter().collect::<Vec<_>>();
            arch_parts.sort();
            type_parts.extend(arch_parts);
            let type_output = type_parts.join(", ");

            output_builder.push_record(vec![
                package.to_owned(),
                codename_version.to_string(),
                codename.to_string(),
                type_output,
            ]);
        }
    }
    Ok(format!(
        "{}\n",
        output_builder
            .build()
            .with(Style::empty().vertical('|'))
            .to_string()
            .lines()
            .map(|line| line.trim())
            .collect::<Vec<&str>>()
            .join("\n")
    ))
}

pub mod key_func {
    use fapt::system::DownloadedList;

    pub type KeyFunc = dyn Fn(&DownloadedList) -> String + Sync + 'static;

    pub fn codename(list: &DownloadedList) -> String {
        list.release.req.codename.to_owned()
    }

    pub fn component(list: &DownloadedList) -> String {
        list.listing.component.to_owned()
    }
}

pub mod madison_cli {
    use figment::providers::{Format, Toml};
    use figment::Figment;
    use serde::Deserialize;

    use crate::{do_madison, init_system, key_func, MadisonConfig};

    #[derive(Deserialize)]
    struct CliConfig {
        global: MadisonConfig,
    }

    pub async fn cli(key_func: &key_func::KeyFunc) {
        let package = std::env::args().nth(1).expect("no package name given");
        let config: CliConfig = Figment::new()
            .merge(Toml::file("Rocket.toml"))
            .extract()
            .expect("reading Rocket.toml configuration");

        let system = init_system(config.global).await.expect("fapt System init");
        print!(
            "{}",
            do_madison(vec![package], &system, key_func, None).expect("generating madison table")
        );
    }
}

pub mod madison_web {

    use std::{
        collections::{hash_map::Entry, HashMap},
        sync::Mutex,
    };

    use fapt::system::System;
    use rocket::{Build, Rocket};

    use crate::{do_madison, init_system, key_func, MadisonConfig};

    struct MadisonState {
        key_func: &'static key_func::KeyFunc,
        system: System,
        cached_results: Mutex<HashMap<(String, Option<String>), String>>,
    }

    #[get("/?<package>&<s>")]
    async fn madison(
        package: String,
        s: Option<String>,
        state: &rocket::State<MadisonState>,
    ) -> Result<String, rocket::response::Debug<anyhow::Error>> {
        let system = &state.system;
        let updated = system.update().await?;
        let mut cached_results = state.cached_results.lock().unwrap();
        if updated {
            cached_results.drain();
        }

        let key = (package.clone(), s.clone());
        Ok(match cached_results.entry(key) {
            Entry::Occupied(o) => o.get().to_owned(),
            Entry::Vacant(v) => v
                .insert(do_madison(
                    package.split(" ").map(|s| s.to_string()).collect(),
                    system,
                    &state.key_func,
                    s,
                )?)
                .to_owned(),
        })
    }

    pub async fn rocket(key_func: &'static key_func::KeyFunc) -> Rocket<Build> {
        let rocket = rocket::build();
        let figment = rocket.figment();
        let config: MadisonConfig = figment.extract().expect("config");

        let system = init_system(config).await.expect("fapt System init");

        rocket.mount("/", routes![madison]).manage(MadisonState {
            key_func,
            system,
            cached_results: Mutex::new(HashMap::new()),
        })
    }
}
