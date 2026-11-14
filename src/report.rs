pub fn info(msg: &str) {
    println!("[git-merge-safe] {msg}");
}

pub fn warn(msg: &str) {
    eprintln!("[git-merge-safe] {msg}");
}
