//! Welcome message builder and printer

use super::App;
use std::net::SocketAddr;

/// Inner width of the greeter box, borders excluded.
///
/// The box grows past this when a long version or address needs the room, and never
/// shrinks below it.
const BOX_WIDTH: usize = 47;

/// Left indent of the address line inside the box.
const URL_INDENT: usize = 5;

/// Draws the greeter box around a centered `title` and an indented `subtitle`.
///
/// The width is computed instead of being baked into the border literals, so a version
/// or an address wider than the default widens the whole box rather than pushing the
/// right edge out of line.
fn draw_box(title: &str, subtitle: &str) -> String {
    let title_len = title.chars().count();
    let subtitle_len = subtitle.chars().count();

    // Both content lines keep at least one space before the right border
    let width = BOX_WIDTH
        .max(title_len + 2)
        .max(URL_INDENT + subtitle_len + 1);

    let border = "─".repeat(width);
    let title_left = " ".repeat((width - title_len) / 2);
    let title_right = " ".repeat(width - title_len - title_left.len());
    let subtitle_left = " ".repeat(URL_INDENT);
    let subtitle_right = " ".repeat(width - URL_INDENT - subtitle_len);

    format!(
        "╭{border}╮\n\
         │{title_left}{title}{title_right}│\n\
         │{subtitle_left}{subtitle}{subtitle_right}│\n\
         ╰{border}╯"
    )
}

impl App {
    /// The greeter message to print, or `None` where the greeter is switched off
    ///
    /// Handed over rather than printed, so that the caller decides when it is said: this
    /// message is what a reader takes for "the server is up"
    pub(super) fn welcome(&self, addr: SocketAddr) -> Option<String> {
        let no_color = std::env::var_os("NO_COLOR").is_some();
        self.build_welcome(addr, no_color)
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

        let content = draw_box(
            &format!(">> Volga v{version}"),
            &format!("Listening on: {url}"),
        );

        let header = if no_color {
            format!("\n{content}\n")
        } else {
            format!("\n\x1b[1;34m{content}\x1b[0m\n")
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

    /// The four lines of the box, borders included, from a colorless greeter
    fn box_lines(output: &str) -> Vec<&str> {
        output
            .lines()
            .filter(|line| line.starts_with(['\u{256d}', '\u{2502}', '\u{2570}']))
            .collect()
    }

    #[test]
    fn it_closes_the_box_on_the_same_column_on_every_line() {
        let app = App::new().with_greeter();
        let addr = "0.0.0.0:7878".parse().unwrap();
        let output = app.build_welcome(addr, true).unwrap();

        let lines = box_lines(&output);
        let widths = lines
            .iter()
            .map(|line| line.chars().count())
            .collect::<Vec<_>>();

        assert_eq!(lines.len(), 4);
        assert!(
            widths.windows(2).all(|w| w[0] == w[1]),
            "the box is ragged: {widths:?}"
        );
    }

    #[test]
    fn it_widens_the_box_for_an_address_the_default_width_cannot_hold() {
        let addr = "[2001:db8:85a3:8d3:1319:8a2e:370:7348]:65535";
        let app = App::new().with_greeter().bind(addr);
        let output = app.build_welcome(addr.parse().unwrap(), true).unwrap();

        let lines = box_lines(&output);
        let widths = lines
            .iter()
            .map(|line| line.chars().count())
            .collect::<Vec<_>>();

        assert!(output.contains(addr), "the address was cut off: {output}");
        assert!(
            widths.windows(2).all(|w| w[0] == w[1]),
            "the box is ragged: {widths:?}"
        );
        assert!(
            widths[0] > 49,
            "the box did not grow for the address: {widths:?}"
        );
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
