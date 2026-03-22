//! Welcome message builder and printer

use super::App;
use std::net::SocketAddr;

impl App {
    /// Prints a greeter message
    pub(super) fn print_welcome(&self, addr: SocketAddr) {
        let no_color = std::env::var_os("NO_COLOR").is_some();
        if let Some(output) = self.build_welcome(addr, no_color) {
            print!("{output}");
        }
    }

    fn build_welcome(&self, addr: SocketAddr, no_color: bool) -> Option<String> {
        if !self.show_greeter {
            return None;
        }

        let version = env!("CARGO_PKG_VERSION");

        #[cfg(not(feature = "tls"))]
        let url = format!("http://{addr}");
        #[cfg(feature = "tls")]
        let url = if self.tls_config.is_some() {
            format!("https://{addr}")
        } else {
            format!("http://{addr}")
        };

        let box_plain = format!(
            "\n╭───────────────────────────────────────────────╮\n\
             │                >> Volga v{version:<5}                │\n\
             │     Listening on: {url:<28}│\n\
             ╰───────────────────────────────────────────────╯\n"
        );

        let header = if no_color {
            box_plain
        } else {
            format!(
                "\n\x1b[1;34m╭───────────────────────────────────────────────╮\n\
                     │                >> Volga v{version:<5}                │\n\
                     │     Listening on: {url:<28}│\n\
                     ╰───────────────────────────────────────────────╯\x1b[0m\n"
            )
        };

        let routes = self.pipeline.endpoints().collect();
        let routes_str = if no_color {
            routes.to_plain_string()
        } else {
            routes.to_string()
        };
        Some(format!("{header}{routes_str}"))
    }
}

#[cfg(test)]
mod tests {
    use crate::App;

    #[test]
    fn it_returns_none_when_greeter_disabled() {
        let app = App::new().without_greeter();
        let addr = "0.0.0.0:7878".parse().unwrap();
        assert!(app.build_welcome(addr, false).is_none());
    }

    #[test]
    fn it_returns_some_when_greeter_enabled() {
        let app = App::new().with_greeter();
        let addr = "0.0.0.0:7878".parse().unwrap();
        assert!(app.build_welcome(addr, false).is_some());
    }

    #[test]
    fn it_contains_version() {
        let app = App::new().with_greeter();
        let addr = "0.0.0.0:7878".parse().unwrap();
        let output = app.build_welcome(addr, false).unwrap();
        assert!(output.contains(env!("CARGO_PKG_VERSION")));
    }

    #[test]
    fn it_contains_base_url() {
        let addr = "127.0.0.1:9090";
        let app = App::new().with_greeter().bind(addr);
        let output = app.build_welcome(addr.parse().unwrap(), false).unwrap();
        assert!(output.contains("http://127.0.0.1:9090"));
    }

    #[test]
    fn it_contains_ansi_codes_when_color_enabled() {
        let mut app = App::new().with_greeter();
        let addr = "0.0.0.0:7878".parse().unwrap();
        app.map_get("/health", || async {});
        let output = app.build_welcome(addr, false).unwrap();
        assert!(output.contains("\x1b[1;34m"));
        assert!(output.contains("\x1b[0m"));
    }

    #[test]
    fn it_omits_ansi_codes_in_box_when_no_color() {
        let app = App::new().with_greeter();
        let addr = "0.0.0.0:7878".parse().unwrap();
        let output = app.build_welcome(addr, true).unwrap();
        assert!(!output.contains('\x1b'));
    }

    #[test]
    fn it_omits_ansi_codes_in_routes_when_no_color() {
        let mut app = App::new().with_greeter();
        let addr = "0.0.0.0:7878".parse().unwrap();
        app.map_get("/health", || async {});
        let output = app.build_welcome(addr, true).unwrap();
        assert!(!output.contains('\x1b'));
        assert!(output.contains("GET"));
        assert!(output.contains("/health"));
    }
}
