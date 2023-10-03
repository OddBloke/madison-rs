#[macro_use]
extern crate rocket;

use std::cmp::Ordering;
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::collections::HashSet;
use std::fs::File;
use std::io::BufReader;

use fapt::commands;
use fapt::sources_list;
use fapt::system::System;

use tabled::{builder::Builder, settings::Style};

use serde::Deserialize;

use rayon::prelude::*;

pub type MadisonMapping = HashMap<String, HashMap<(String, String), HashSet<String>>>;

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

fn build_madison_mapping(
    system: &System,
    key_func: &key_func::KeyFunc,
) -> Result<MadisonMapping, anyhow::Error> {
    // Collect all the versions
    let versions: Vec<Vec<_>> = system
        .listings()?
        .par_iter()
        .map(|downloaded_list| -> Result<_, anyhow::Error> {
            let key = key_func(downloaded_list);
            let mut versions: HashMap<_, (String, HashSet<_>)> = HashMap::new();
            for section in system.open_listing(downloaded_list)? {
                let pkg = section?.as_pkg()?;
                if let Some(bin) = pkg.as_bin() {
                    let mut pkg_types = HashMap::new();
                    pkg_types
                        .entry(match bin.source.as_ref() {
                            Some(source_pkg_name) => source_pkg_name.clone(),
                            None => pkg.name.clone(),
                        })
                        .or_insert(HashSet::new())
                        .insert("source".to_string());
                    pkg_types.entry(pkg.name).or_insert(HashSet::new()).insert(
                        downloaded_list
                            .listing
                            .arch
                            .as_ref()
                            .unwrap_or(&"unknown!".to_string())
                            .clone(),
                    );
                    for (pkg_name, pkg_types) in pkg_types.into_iter() {
                        match versions.entry(pkg_name) {
                            Entry::Occupied(mut o) => {
                                let (current_version, types) = o.get_mut();
                                types.extend(pkg_types);
                                if deb_version::compare_versions(&pkg.version, current_version)
                                    == Ordering::Greater
                                {
                                    *current_version = pkg.version.clone()
                                }
                            }
                            Entry::Vacant(o) => {
                                o.insert((pkg.version.clone(), pkg_types));
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

    let mut merged_versions: MadisonMapping = HashMap::new();
    for (package, codename, codename_version, types) in versions.into_iter().flatten() {
        let pkg_merged_versions = merged_versions.entry(package).or_insert(HashMap::new());
        let key = (codename, codename_version);
        if let Some(current_value) = pkg_merged_versions.get_mut(&key) {
            (*current_value).extend(types.to_owned());
        } else {
            pkg_merged_versions.insert(key, types.to_owned());
        }
    }
    Ok(merged_versions)
}

fn generate_madison_foo(
    madison_mapping: &MadisonMapping,
    packages: &Vec<String>,
    suite: Option<String>,
) -> HashMap<String, Vec<Vec<String>>> {
    packages
        .par_iter()
        .filter_map(|package| {
            madison_mapping
                .get(package)
                .map(|entries| (package.clone(), entries))
        })
        .map(|(package, entries)| {
            let mut merged_vec = entries
                .into_iter()
                .filter(|((codename, _), _)| {
                    suite
                        .as_ref()
                        .map(|suite| codename == suite)
                        .unwrap_or(true)
                })
                .collect::<Vec<_>>();
            merged_vec.sort_by(|((codename1, v1), _), ((codename2, v2), _)| {
                match deb_version::compare_versions(v1, v2) {
                    Ordering::Equal => codename1.cmp(codename2),
                    other => other,
                }
            });
            (package, merged_vec)
        })
        .map(|(package, merged_vec)| {
            let lines: Vec<_> = merged_vec
                .into_iter()
                .map(|((codename, codename_version), types)| {
                    // Start with "source", append sorted architectures, join with ", "
                    let mut types = types.clone();
                    let mut type_parts: Vec<_> = types.take("source").into_iter().collect();
                    let mut arch_parts = types.iter().map(|s| s.clone()).collect::<Vec<_>>();
                    arch_parts.sort();
                    type_parts.extend(arch_parts);
                    vec![
                        package.to_owned(),
                        codename_version.to_string(),
                        codename.to_string(),
                        type_parts.join(", "),
                    ]
                })
                .collect();
            (package, lines)
        })
        .collect()
}

pub fn do_madison(
    madison_mapping: &MadisonMapping,
    packages: Vec<String>,
    suite: Option<String>,
) -> Result<String, anyhow::Error> {
    let mut package_lines = generate_madison_foo(madison_mapping, &packages, suite);
    let mut output_builder = Builder::default();
    for package in packages {
        let merged_vec = if let Some(merged_vec) = package_lines.remove(&package) {
            merged_vec
        } else {
            continue;
        };
        for line in merged_vec {
            output_builder.push_record(line);
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

    use crate::{build_madison_mapping, do_madison, init_system, key_func, MadisonConfig};

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
        let madison_mapping =
            build_madison_mapping(&system, key_func).expect("build madison mapping");
        print!(
            "{}",
            do_madison(&madison_mapping, vec![package], None).expect("generating madison table")
        );
    }
}

pub mod madison_web {

    use std::{
        collections::HashMap,
        sync::{Arc, RwLock},
        time::Duration,
    };

    use log::info;
    use rocket::{Build, Rocket};
    use rocket_dyn_templates::Template;
    use tokio::time::sleep;

    use crate::{
        build_madison_mapping, do_madison, init_system, key_func, MadisonConfig, MadisonMapping,
    };

    struct MadisonState {
        madison_mapping: Arc<RwLock<MadisonMapping>>,
    }

    #[get("/")]
    async fn index() -> Template {
        let context: HashMap<String, String> = HashMap::new();
        Template::render("index.html", &context)
    }

    #[get("/?<package>&<s>")]
    async fn madison(
        package: String,
        s: Option<String>,
        state: &rocket::State<MadisonState>,
    ) -> Result<String, rocket::response::Debug<anyhow::Error>> {
        let ro_mapping = state.madison_mapping.read().expect("read access failed");
        Ok(do_madison(
            &ro_mapping,
            package.split(" ").map(|s| s.to_string()).collect(),
            s,
        )?)
    }

    pub async fn rocket(key_func: &'static key_func::KeyFunc) -> Rocket<Build> {
        let rocket = rocket::build();
        let figment = rocket.figment();
        let config: MadisonConfig = figment.extract().expect("config");

        let system = init_system(config).await.expect("fapt System init");

        let mapping_lock = Arc::new(RwLock::new(HashMap::new()));
        let c_lock = mapping_lock.clone();
        tokio::task::spawn(async move {
            {
                // Take the lock immediately for initialisation
                let mut madison_mapping = c_lock.write().expect("write access failed");
                info!("Initialising madison mapping");
                *madison_mapping =
                    build_madison_mapping(&system, key_func).expect("build_madison_mapping");
            }

            loop {
                sleep(Duration::from_secs(60)).await;
                info!("Checking for updates");
                let did_update = system.update().await.unwrap();
                if did_update {
                    info!("Update happened: updating mapping");
                    let new_mapping =
                        build_madison_mapping(&system, key_func).expect("build_madison_mapping");
                    let mut madison_mapping = c_lock.write().expect("write access failed");
                    *madison_mapping = new_mapping
                }
            }
        });
        info!("Task spawned!");

        rocket
            .mount("/", routes![index, madison])
            .manage(MadisonState {
                madison_mapping: mapping_lock,
            })
            .attach(Template::try_custom(|engines| {
                for (name, content) in templates::TEMPLATES {
                    engines.tera.add_raw_template(name, content)?;
                }
                Ok(())
            }))
    }

    mod templates {
        const INDEX_TMPL: &str = r#"
            <html>
              <form method="get">
                <input id="urlInput" type="search" name="package" placeholder="package name" autofocus required>
                <input type="submit">
              </form>
            </html>
        "#;
        pub(super) const TEMPLATES: &[(&str, &str)] = &[("index.html", INDEX_TMPL)];
    }
}
