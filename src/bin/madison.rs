use madison_rs::{do_madison, init_system};

fn main() {
    let package = std::env::args().nth(1).expect("no package name given");

    let system = init_system("./sources.list".to_string()).expect("fapt System init");
    print!(
        "{}",
        do_madison(package, &system).expect("generating madison table")
    );
}
