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
    let versions: HashMap<String, String> = system
        .listings()?
        .par_iter()
        .map(
            |downloaded_list| -> Result<(String, Option<String>), anyhow::Error> {
                let key = key_func(downloaded_list);
                let mut version: Option<String> = None;
                for section in system.open_listing(downloaded_list)? {
                    let pkg = section?.as_pkg()?;
                    if let Some(bin) = pkg.as_bin() {
                        let resolved_source = if let Some(bin_src) = &bin.source {
                            bin_src
                        } else {
                            &pkg.name
                        };
                        if resolved_source == &package {
                            if let Some(current_value) = &version {
                                if deb_version::compare_versions(current_value, &pkg.version)
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
                Ok((key, version))
            },
        )
        .filter_map(|res| res.ok())
        .filter_map(|(key, version)| version.map(|version| (key, version)))
        .collect();
    info!("{:?}", versions);

    let mut output_builder = Builder::default();
    let mut sorted_by_version: Vec<_> = versions.iter().collect();

    sorted_by_version.sort_by(|(_, v1), (_, v2)| deb_version::compare_versions(v1, v2));

    for (codename, codename_version) in sorted_by_version {
        output_builder.add_record(vec![
            package.to_owned(),
            codename_version.to_string(),
            codename.to_string(),
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
