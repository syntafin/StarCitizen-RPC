fn main() {
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("images/STARCITIZEN_WHITE.ico");
        res.compile().expect("Windows-Icon-Ressource konnte nicht kompiliert werden");
    }
}
