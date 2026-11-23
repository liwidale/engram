fn main() {
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../../../branding/icons/engram.ico")
            .set("ProductName", "Engram")
            .set("FileDescription", "Engram")
            .set("CompanyName", "Liwidale")
            .set("LegalCopyright", "Copyright (c) 2026 Liwidale");
        res.compile().expect("failed to embed Windows resources");
    }
    println!("cargo:rerun-if-changed=../../../branding/icons");
}
