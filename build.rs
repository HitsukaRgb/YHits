fn main() {
    if cfg!(target_os = "windows") {
        let mut res = winres::WindowsResource::new();
        res.set_icon("src\\assets\\icon.ico");
        res.set("ProductName", "YHits");
        res.set("FileDescription", "YHits");
        res.compile().expect("embed Windows application icon");
    }
}
