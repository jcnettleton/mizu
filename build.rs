fn main() {
    if std::env::var_os("CARGO_CFG_WINDOWS").is_some() {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/Mizu_Taskbar_Icon.ico");
        res.compile().expect("failed to compile Windows icon resource");
    }
}
