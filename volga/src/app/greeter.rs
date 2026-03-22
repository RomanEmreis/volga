//! Welcome message builder and printer

use super::App;

impl App {
    /// Prints a greeter message
    pub(super) fn print_welcome(&self) {
        let no_color = std::env::var_os("NO_COLOR").is_some();
        if let Some(output) = self.build_welcome(no_color) {
            print!("{output}");
        }
    }

    fn build_welcome(&self, no_color: bool) -> Option<String> {
        if !self.show_greeter {
            return None;
        }

        let version = env!("CARGO_PKG_VERSION");
        let addr = self.connection.socket;

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
        assert!(app.build_welcome(false).is_none());
    }

    #[test]
    fn it_returns_some_when_greeter_enabled() {
        let app = App::new().with_greeter();
        assert!(app.build_welcome(false).is_some());
    }

    #[test]
    fn it_contains_version() {
        let app = App::new().with_greeter();
        let output = app.build_welcome(false).unwrap();
        assert!(output.contains(env!("CARGO_PKG_VERSION")));
    }

    #[test]
    fn it_contains_base_url() {
        let app = App::new().with_greeter().bind("127.0.0.1:9090");
        let output = app.build_welcome(false).unwrap();
        assert!(output.contains("http://127.0.0.1:9090"));
    }

    #[test]
    fn it_contains_ansi_codes_when_color_enabled() {
        let mut app = App::new().with_greeter();
        app.map_get("/health", || async {});
        let output = app.build_welcome(false).unwrap();
        assert!(output.contains("\x1b[1;34m"));
        assert!(output.contains("\x1b[0m"));
    }

    #[test]
    fn it_omits_ansi_codes_in_box_when_no_color() {
        let app = App::new().with_greeter();
        let output = app.build_welcome(true).unwrap();
        assert!(!output.contains('\x1b'));
    }

    #[test]
    fn it_omits_ansi_codes_in_routes_when_no_color() {
        let mut app = App::new().with_greeter();
        app.map_get("/health", || async {});
        let output = app.build_welcome(true).unwrap();
        assert!(!output.contains('\x1b'));
        assert!(output.contains("GET"));
        assert!(output.contains("/health"));
    }
}
