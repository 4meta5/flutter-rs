# flutter-rs [![Gitter chat][gitter-badge]][gitter-url] [![Crates.io][crates-badge]][crates-url] [![MIT licensed][mit-badge]][mit-url]

<center><img src="https://raw.githubusercontent.com/gliheng/flutter-rs/master/www/images/logo.png" width="200" height="200" /></center>

**Build flutter desktop app in dart & rust**


This is the development repo. Head to [flutter-app-template](https://github.com/gliheng/flutter-app-template) for a running demo.

# Install
- Install latest [Rust](https://www.rust-lang.org)
- Install libglfw:
    - Mac: `brew install glfw`
    - linux: `apt install libglfw3`
- Install [flutter sdk](https://flutter.io)

- In *flutter-app* project, set flutter sdk version in *Cargo.toml*

```
[package.metadata.flutter]
version = "5af435098d340237c5e3a69bce6aaffd4e3bfe84"
```

    This commit version id can be found in bin/internal/engine.version file in flutter sdk folder.

- Run `scripts/run.py` to get a running example.
    Note: The first run is going to take a while to download rust dependecies and flutter engine.

# Features:
- Support Hot reload
- MethodChannel, EventChannel
- Async runtime using tokio
- Application icons
- System dialogs
- Clipboard support
- Cross platform support (mac & linux)
- Support distribution format: (mac app, mac dmg)

# Roadmap:

## 0.2
- Support setting default window background color.
- Loader UI and rebranding.
- Desktop integration: App menu, context menu, file dialogs.
- Flutter scroller should support desktop scroll event.
- Download dll from web?

# Contribution
To contribute to flutter-rs, please see [CONTRIBUTING](CONTRIBUTING.md).

[flutter-rs logo]: https://raw.githubusercontent.com/gliheng/flutter-rs/master/www/images/logo.svg
[gitter-badge]: https://badges.gitter.im/flutter-rs/community.svg
[gitter-url]: https://gitter.im/flutter-rs/community?utm_source=badge&utm_medium=badge&utm_campaign=pr-badge&utm_content=badge
[crates-badge]: https://img.shields.io/crates/v/flutter-engine.svg
[crates-url]: https://crates.io/crates/flutter-engine
[mit-badge]: https://img.shields.io/badge/license-MIT-blue.svg
[mit-url]: LICENSE-MIT