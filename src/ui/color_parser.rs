use ratatui::style::Color;

pub fn parse_color(s: &str) -> Color {
    let s = s.trim().to_lowercase();
    match s.as_str() {
        "reset" => Color::Reset,
        "black" => Color::Black,
        "red" => Color::Red,
        "green" => Color::Green,
        "yellow" => Color::Yellow,
        "blue" => Color::Blue,
        "magenta" => Color::Magenta,
        "cyan" => Color::Cyan,
        "gray" => Color::Gray,
        "darkgray" => Color::DarkGray,
        "lightred" => Color::LightRed,
        "lightgreen" => Color::LightGreen,
        "lightyellow" => Color::LightYellow,
        "lightblue" => Color::LightBlue,
        "lightmagenta" => Color::LightMagenta,
        "lightcyan" => Color::LightCyan,
        "white" => Color::White,
        _ => {
            if s.contains(',') {
                let parts: Vec<&str> = s.split(',').collect();
                if parts.len() == 3 {
                    if let (Ok(r), Ok(g), Ok(b)) = (
                        parts[0].trim().parse(),
                        parts[1].trim().parse(),
                        parts[2].trim().parse(),
                    ) {
                        return Color::Rgb(r, g, b);
                    }
                }
            }
            Color::Reset
        }
    }
}

#[cfg(test)]
mod tests {
    use super::parse_color;
    use ratatui::style::Color;

    #[test]
    fn parses_named_colors_case_insensitive() {
        assert_eq!(parse_color("Blue"), Color::Blue);
        assert_eq!(parse_color("lightcyan"), Color::LightCyan);
        assert_eq!(parse_color("DaRkGrAy"), Color::DarkGray);
    }

    #[test]
    fn parses_rgb_values() {
        assert_eq!(parse_color("1,2,3"), Color::Rgb(1, 2, 3));
        assert_eq!(parse_color(" 10 , 20 , 30 "), Color::Rgb(10, 20, 30));
    }

    #[test]
    fn invalid_values_fall_back_to_reset() {
        assert_eq!(parse_color("not-a-color"), Color::Reset);
        assert_eq!(parse_color("1,2"), Color::Reset);
        assert_eq!(parse_color("1,2,3,4"), Color::Reset);
    }
}
