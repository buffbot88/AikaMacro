fn main() {
    println!("cargo:rerun-if-changed=app.manifest");
    if std::env::var_os("CARGO_CFG_WINDOWS").is_some() {
        let mut resource = winresource::WindowsResource::new();
        resource.set_manifest_file("app.manifest");
        resource.compile().expect("failed to embed app.manifest");
    }
}
