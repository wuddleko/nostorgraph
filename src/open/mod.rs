mod browse;
mod document;
mod link;
mod note;
mod source;

pub fn run() -> i32 {
    let _arguments: Vec<String> = std::env::args().skip(1).collect();
    0
}
