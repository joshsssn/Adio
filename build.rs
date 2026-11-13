fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico")
            .set("FileDescription", "Adio - lecteur audio portable")
            .set("ProductName", "Adio");
        if let Err(e) = res.compile() {
            println!("cargo:warning=icone non integree: {e}");
        }
    }
}
