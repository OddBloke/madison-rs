use std::cmp::Ordering;
use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;

use log::info;

use fapt::commands;
use fapt::sources_list;
use fapt::system::System;

use tabled::{builder::Builder, Style};

use serde::Deserialize;

#[derive(Deserialize)]
pub struct MadisonConfig {
    pub sources_list: String,
}

pub fn init_system(config: MadisonConfig) -> Result<System, anyhow::Error> {
    // Setup the system
    let mut system = System::cache_only()?;
    commands::add_builtin_keys(&mut system);
    system.add_sources_entries(sources_list::read(BufReader::new(
        File::open(&config.sources_list).unwrap(),
    ))?);

    system.set_arches(vec!["amd64"]);
    system.update()?;
    Ok(system)
}

pub fn do_madison(package: String, system: &System) -> Result<String, anyhow::Error> {
    // Collect all the versions
    let mut versions: HashMap<String, String> = HashMap::new();
    system.listings()?.iter().map(|downloaded_list| -> Result<(), anyhow::Error> {
        for section in system.open_listing(&downloaded_list)? {
            let pkg = section?.as_pkg()?;
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
                        versions.insert(key.clone(), pkg.version.to_owned());
                    }
                }
            }
        }
        Ok(())
    }).for_each(drop);
    info!("{:?}", versions);

    let mut output_builder = Builder::default();
    let mut sorted_by_version: Vec<_> = versions.iter().collect();

    sorted_by_version.sort_by(|(_, v1), (_, v2)| deb_version::compare_versions(v1, v2));

    for (codename, codename_version) in sorted_by_version {
        output_builder.add_record(vec![
            package.to_owned(),
            codename_version.to_owned(),
            codename.to_owned(),
            "source".to_string(),
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
