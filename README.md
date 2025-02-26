# Krustmote

A Kodi remote control written in rust using the [iced](https://github.com/iced-rs/iced/) GUI library.

## Features
Can browse your Movies, TV, video sources Files, and tell kodi to play them. 
Files also inserts `videoDB://` so you can browse recentlyadded/etc.
Shows playing controls whenever an item is playing, and regular arrows/ok/back/etc when it's connected.
Capable of working offline to browse the Movies/TV in the database.

### Still to do:
Movie / TV / Episode details views.
Look in to other media types (Music, PVR, Addons, etc) (addons may be as simple as `addons://sources/etc`)
Connect to / switch between multiple different kodi instances.

### Currently in beta status. 
It currently spawns a console window for debug output stuff. A lot of junk is written to it.

I'm building this on Windows, likely works on Linux/macOS too but I have never tried it.
If I change the version number in `Cargo.toml` I probably made a change that means you should clear imagecache and delete krustmote.db

It first tries directories-next to use proper directories, if that fails:
Tries to create `./krustmote.db` in local directory. 
Tries to use `./imagecache/` to cache thumbnails/posters (will fail entirely if that directory doesn't exist)

Some of the program is designed to allow mutliple kodi instances in the future, but the database is currently not. It always assumes you're connected to the *same* kodi instance so the database movie/tv/etc entries will probably be all broken and may crash the program.