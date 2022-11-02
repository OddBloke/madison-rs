#[macro_use]
extern crate rocket;

use std::cmp::Ordering;
use std::collections::HashMap;
use std::collections::HashSet;
use std::fs::File;
use std::io::BufReader;

use log::info;

use fapt::commands;
use fapt::sources_list;
use fapt::system::System;

use tabled::{builder::Builder, Style};

use serde::Deserialize;

use rayon::prelude::*;

#[derive(Deserialize)]
pub struct MadisonConfig {
    pub sources_list: String,
    pub extra_key_paths: Vec<String>,
}

pub fn init_system(config: MadisonConfig) -> Result<System, anyhow::Error> {
    // Setup the system
    let mut system = System::cache_only()?;
    for path in config.extra_key_paths {
        system.add_keys_from(File::open(path)?)?;
    }
    commands::add_builtin_keys(&mut system);
    system.add_sources_entries(sources_list::read(BufReader::new(
        File::open(&config.sources_list).unwrap(),
    ))?);

    system.set_arches(vec!["amd64"]);
    system.update()?;
    Ok(system)
}

pub fn do_madison(
    package: String,
    system: &System,
    key_func: &key_func::KeyFunc,
) -> Result<String, anyhow::Error> {
    // Collect all the versions
    let versions: Vec<_> = system
        .listings()?
        .par_iter()
        .map(|downloaded_list| -> Result<_, anyhow::Error> {
            let mut types = HashSet::new();
            let key = key_func(downloaded_list);
            let mut version: Option<String> = None;
            for section in system.open_listing(downloaded_list)? {
                let pkg = section?.as_pkg()?;
                if let Some(bin) = pkg.as_bin() {
                    let pkg_type = if bin.source.as_ref() == Some(&package) {
                        Some("source".to_string())
                    } else if pkg.name == package {
                        downloaded_list
                            .listing
                            .arch
                            .as_ref()
                            .map_or(Some("unknown!".to_string()), |arch| Some(arch.to_owned()))
                    } else {
                        None
                    };
                    if let Some(pkg_type) = pkg_type {
                        types.insert(pkg_type);
                        if let Some(current_value) = &version {
                            if deb_version::compare_versions(&pkg.version, current_value)
                                == Ordering::Greater
                            {
                                version = Some(pkg.version);
                            }
                        } else {
                            version = Some(pkg.version);
                        }
                    }
                }
            }
            Ok((key, version, types))
        })
        .filter_map(|res| {
            if let Ok((name, version, types)) = res {
                version.map(|version| Ok((name, version, types)))
            } else {
                Some(Err(res.expect_err("unreachable")))
            }
        })
        .collect::<Result<_, _>>()?;
    info!("{:?}", versions);

    let mut merged_versions: HashMap<(String, String), HashSet<_>> = HashMap::new();
    for (codename, codename_version, types) in versions {
        let key = (codename, codename_version);
        if let Some(current_value) = merged_versions.get_mut(&key) {
            (*current_value).extend(types);
        } else {
            merged_versions.insert(key, types);
        }
    }

    let mut merged_vec = merged_versions.into_iter().collect::<Vec<_>>();
    merged_vec.sort_by(|((codename1, v1), _), ((codename2, v2), _)| {
        match deb_version::compare_versions(v1, v2) {
            Ordering::Equal => codename1.cmp(codename2),
            other => other,
        }
    });

    let mut output_builder = Builder::default();
    for ((codename, codename_version), mut types) in merged_vec {
        // Start with "source", append sorted architectures, join with ", "
        let mut type_parts: Vec<_> = types.take("source").into_iter().collect();
        let mut arch_parts = types.into_iter().collect::<Vec<_>>();
        arch_parts.sort();
        type_parts.extend(arch_parts);
        let type_output = type_parts.join(", ");

        output_builder.add_record(vec![
            package.to_owned(),
            codename_version.to_string(),
            codename.to_string(),
            type_output,
        ]);
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

    pub fn cli(key_func: &key_func::KeyFunc) {
        let package = std::env::args().nth(1).expect("no package name given");
        let config: CliConfig = Figment::new()
            .merge(Toml::file("Rocket.toml"))
            .extract()
            .expect("reading Rocket.toml configuration");

        let system = init_system(config.global).expect("fapt System init");
        print!(
            "{}",
            do_madison(package, &system, key_func).expect("generating madison table")
        );
    }
}

pub mod madison_web {

    use fapt::system::System;
    use rocket::{Build, Rocket};

    use crate::{do_madison, init_system, key_func, MadisonConfig};

    struct MadisonState {
        key_func: &'static key_func::KeyFunc,
        system: System,
    }

    #[get("/?<package>")]
    fn madison(
        package: String,
        state: &rocket::State<MadisonState>,
    ) -> Result<String, rocket::response::Debug<anyhow::Error>> {
        let system = &state.system;
        system.update()?;
        Ok(do_madison(package, system, &state.key_func)?)
    }

    pub fn rocket(key_func: &'static key_func::KeyFunc) -> Rocket<Build> {
        let rocket = rocket::build();
        let figment = rocket.figment();
        let config: MadisonConfig = figment.extract().expect("config");

        let system = init_system(config).expect("fapt System init");

        rocket
            .mount("/", routes![madison])
            .manage(MadisonState { key_func, system })
    }
}
