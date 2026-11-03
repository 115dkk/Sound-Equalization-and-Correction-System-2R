#![deny(unsafe_code)]

fn main() {
    println!("cargo:rerun-if-changed=src/ffi.rs");
    println!("cargo:rerun-if-changed=build.rs");

    csbindgen::Builder::default()
        .input_extern_file("src/ffi.rs")
        .csharp_dll_name("secs2r_native")
        .csharp_namespace("Secs2R")
        .csharp_class_name("NativeMethods")
        .csharp_class_accessibility("internal")
        .generate_csharp_file("../Secs2R/NativeMethods.g.cs")
        .expect("could not generate NativeMethods.g.cs");
}
