use std::env;

fn main() {
    let mut build = cc::Build::new();
    build
        .cpp(true)
        .std("c++17")
        .file("src/aes.cpp")
        .file("src/sha.cpp")
        .include("src")
        .warnings(false);

    if env::var("TARGET")
        .map(|target| target.contains("msvc"))
        .unwrap_or(false)
    {
        build.flag_if_supported("/std:c++17");
    } else {
        if build.is_flag_supported("-std=c++17").unwrap_or(false) {
            build.flag("-std=c++17");
        }
        if build.is_flag_supported("-maes").unwrap_or(false) {
            build.flag("-maes");
        }
        if build.is_flag_supported("-msha").unwrap_or(false) {
            build.flag("-msha");
        }
    }

    build.compile("hf_native");
}
