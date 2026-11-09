fn main() {
    println!("cargo:rerun-if-changed=assets/tabex.ico");
    println!("cargo:rerun-if-changed=build.rs");
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/tabex.ico")
            .set("ProductName", "Tabex")
            .set("FileDescription", "Tabex - Explorador de arquivos com grupos de abas")
            .set("LegalCopyright", "MIT")
            .set("OriginalFilename", "tabex.exe");
        if let Err(e) = res.compile() {
            println!("cargo:warning=não foi possível embutir o ícone/recursos do Windows: {e}");
        }
    }
}
