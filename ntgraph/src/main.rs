mod open;

fn main() {
    let code = open::run();
    std::process::exit(code);
}
