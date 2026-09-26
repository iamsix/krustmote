use std::io;

fn main() -> io::Result<()> {
    // Only run this build script when compiling for Windows
    if std::env::var_os("CARGO_CFG_WINDOWS").is_some() {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("icon.ico"); // Path to your .ico file
        res.compile()?;
    }
    Ok(())
}
