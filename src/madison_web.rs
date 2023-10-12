use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
    time::Duration,
};

use log::info;
use rocket::{Build, Rocket};
use rocket_dyn_templates::{context, Template};
use rocket_prometheus::{
    prometheus::{opts, IntCounter, IntCounterVec},
    PrometheusMetrics,
};
use tokio::time::sleep;

use crate::{
    build_madison_mapping, do_madison, generate_madison_structure, init_system, key_func,
    MadisonConfig, MadisonMapping,
};

mod templates;

#[derive(Clone)]
struct MadisonMetrics {
    update_attempts: IntCounter,
    update_failures: IntCounter,
    package_lookups: IntCounterVec,
}

impl MadisonMetrics {
    fn new() -> Result<Self, anyhow::Error> {
        Ok(Self {
            update_attempts: IntCounter::new(
                "madison_rs_apt_update_attempts",
                "Count of system apt update attempts",
            )?,
            update_failures: IntCounter::new(
                "madison_rs_apt_update_failures",
                "Count of failed system apt update attempts",
            )?,
            package_lookups: IntCounterVec::new(
                opts!(
                    "madison_rs_package_lookups",
                    "Count of packages looked up, and how"
                ),
                &["route", "package_name"],
            )?,
        })
    }

    fn register_with(self, prometheus: &PrometheusMetrics) -> Result<(), anyhow::Error> {
        let registry = prometheus.registry();
        registry.register(Box::new(self.update_attempts))?;
        registry.register(Box::new(self.update_failures))?;
        registry.register(Box::new(self.package_lookups))?;
        Ok(())
    }
}

struct MadisonState {
    madison_mapping: Arc<RwLock<MadisonMapping>>,
}

#[get("/")]
async fn index() -> Template {
    let context: HashMap<String, String> = HashMap::new();
    Template::render("index.html", &context)
}

fn get_packages(package_str: String, metrics: &MadisonMetrics, source: &str) -> Vec<String> {
    package_str
        .split(" ")
        .map(|s| {
            metrics
                .package_lookups
                .with_label_values(&[source, s])
                .inc();
            s.to_string()
        })
        .collect()
}

#[get("/?<package>&text=on&<s>")]
async fn madison(
    package: String,
    s: Option<String>,
    state: &rocket::State<MadisonState>,
    metrics: &rocket::State<MadisonMetrics>,
) -> Result<String, rocket::response::Debug<anyhow::Error>> {
    let ro_mapping = state.madison_mapping.read().expect("read access failed");
    let packages = get_packages(package, metrics, "rmadison");
    let mut madison = generate_madison_structure(&ro_mapping, &packages, s);
    Ok(do_madison(&mut madison, packages))
}

#[get("/?<package>&<s>")]
async fn madison_html(
    package: String,
    s: Option<String>,
    state: &rocket::State<MadisonState>,
    metrics: &rocket::State<MadisonMetrics>,
) -> Template {
    let ro_mapping = state.madison_mapping.read().expect("read access failed");
    let packages = get_packages(package, metrics, "html");
    Template::render(
        "package.html",
        context! {madison: generate_madison_structure(&ro_mapping, &packages, s)},
    )
}

pub async fn rocket(key_func: &'static key_func::KeyFunc) -> Rocket<Build> {
    let rocket = rocket::build();
    let figment = rocket.figment();
    let config: MadisonConfig = figment.extract().expect("config");
    let metrics = MadisonMetrics::new().unwrap();

    let system = init_system(&config).await.expect("fapt System init");

    let mapping_lock = Arc::new(RwLock::new(HashMap::new()));
    let c_lock = mapping_lock.clone();
    let task_metrics = metrics.clone();
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
            task_metrics.update_attempts.inc();
            let did_update = match system.update().await {
                Ok(val) => val,
                Err(e) => {
                    task_metrics.update_failures.inc();
                    warn!("Encountered error when updating: {}", e);
                    false
                }
            };
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
        metrics.clone().register_with(&prometheus).unwrap();
        app = app
            .attach(prometheus.clone())
            .mount("/metrics", prometheus)
            .manage(metrics)
    }
    app
}
