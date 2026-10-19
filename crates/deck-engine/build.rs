fn main() {
    cc::Build::new()
        .cpp(true)
        .std("c++17")
        .opt_level(3)
        .include("vendor")
        .include("vendor/signalsmith-stretch")
        .file("native/stretch.cpp")
        .compile("pulsedeck_stretch");
    println!("cargo:rerun-if-changed=native/stretch.cpp");
    println!("cargo:rerun-if-changed=vendor");
}
