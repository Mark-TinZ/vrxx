use std::process::Command;

fn main() {
    let output = Command::new("curl")
        .arg("-s")
        .arg("-w")
        .arg("%{time_total}")
        .arg("--connect-timeout")
        .arg("2")
        .arg("http://192.0.2.1")
        .output()
        .unwrap();

    println!("Success: {}", output.status.success());
    println!("Output: {}", String::from_utf8_lossy(&output.stdout));
}