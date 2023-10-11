use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
    time::Duration,
};

use log::info;
use rocket::{Build, Rocket};
use rocket_dyn_templates::Template;
use rocket_prometheus::PrometheusMetrics;
use tokio::time::sleep;

use crate::{
    build_madison_mapping, do_madison, generate_madison_structure, init_system, key_func,
    MadisonConfig, MadisonMapping,
};

struct MadisonState {
    madison_mapping: Arc<RwLock<MadisonMapping>>,
}

#[get("/")]
async fn index() -> Template {
    let context: HashMap<String, String> = HashMap::new();
    Template::render("index.html", &context)
}

#[get("/?<package>&text=on&<s>")]
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
    ))
}

#[get("/?<package>&<s>")]
async fn madison_html(
    package: String,
    s: Option<String>,
    state: &rocket::State<MadisonState>,
) -> Template {
    let mut context = HashMap::new();
    let ro_mapping = state.madison_mapping.read().expect("read access failed");
    context.insert(
        "madison",
        generate_madison_structure(
            &ro_mapping,
            &package.split(" ").map(|s| s.to_string()).collect(),
            s,
        ),
    );
    Template::render("package.html", &context)
}

pub async fn rocket(key_func: &'static key_func::KeyFunc) -> Rocket<Build> {
    let rocket = rocket::build();
    let figment = rocket.figment();
    let config: MadisonConfig = figment.extract().expect("config");

    let system = init_system(&config).await.expect("fapt System init");

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

    let mut app = rocket
        .mount("/", routes![index, madison, madison_html])
        .manage(MadisonState {
            madison_mapping: mapping_lock,
        })
        .attach(Template::try_custom(|engines| {
            let loaded_templates: Vec<_> = engines
                .tera
                .get_template_names()
                .map(|s| s.to_string())
                .collect();
            for (name, content) in templates::TEMPLATES {
                if !loaded_templates.contains(&name.to_string()) {
                    engines.tera.add_raw_template(name, content)?;
                }
            }
            Ok(())
        }));
    if config.enable_metrics {
        let prometheus = PrometheusMetrics::new();
        app = app.attach(prometheus.clone()).mount("/metrics", prometheus)
    }
    app
}

mod templates {
    const PACKAGE_MACROS: &str = r#"
      {% macro package_row(record) %}
      <tr>
        <td>{{ record.package }}</td>
        <td>{{ record.version }}</td>
        <td>{{ record.codename }}</td>
        <td>{{ record.architectures }}</td>
      </tr>
      {% endmacro package_row %}
    "#;
    const PACKAGE_TABLE: &str = r#"
      <table>
        <thead>
          <th>Package</th>
          <th>Version</th>
          <th></th>
          <th>Architecture</th>
        </thead>
      {% for _, package_records in madison %}
        {% for record in package_records %}
          {{ package_macros::package_row(record=record) }}
        {% endfor %}
      {% endfor %}
      </table>
    "#;
    const SEARCH_FORM: &str = r#"
      <form method="get">
        <input id="urlInput" type="search" name="package" placeholder="package name" autofocus required>
        <input type="submit">
      </form>
    "#;

    const BASE_TMPL: &str = r#"
        <html>
          {% block body %}{% endblock %}
          {% include "footer" ignore missing %}
        </html>
    "#;
    const INDEX_TMPL: &str = r#"
        {% extends "base.html" %}
        {% block body %}{% include "search-form" %}{% endblock %}
    "#;
    const PACKAGE_TMPL: &str = r#"
        {% extends "base.html" %}
        {% import "package-macros" as package_macros %}
        {% block body %}{% include "package-table" %}{% endblock %}
    "#;
    pub(super) const TEMPLATES: &[(&str, &str)] = &[
        ("package-macros", PACKAGE_MACROS),
        ("package-table", PACKAGE_TABLE),
        ("search-form", SEARCH_FORM),
        ("base.html", BASE_TMPL),
        ("index.html", INDEX_TMPL),
        ("package.html", PACKAGE_TMPL),
    ];
}
