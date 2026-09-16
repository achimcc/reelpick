//! The chrome texts, in the configured language.
//!
//! Everything reelpick writes itself — teaser, reason, article — is already in
//! `config.language`, because the model is asked to write it there. The words
//! around it were English regardless, which made a German page read like a
//! translation that stopped halfway. This module holds those words, the number
//! and month formats that go with them, and nothing else: two languages, a
//! plain struct, no i18n crate for a dozen strings.

/// The chrome texts of one language. `&'static str` throughout — these are
/// literals, never user input, so nothing here can allocate or fail.
#[derive(Debug, Clone, Copy)]
pub struct Strings {
    pub home: &'static str,
    pub all_picks: &'static str,
    /// The link under the fragment on the embedding page.
    pub all_picks_so_far: &'static str,
    /// The bracketed section heading of the history page.
    pub all_picks_heading: &'static str,
    pub history_lead: &'static str,
    pub no_pick: &'static str,
    /// Includes its separator, because German wants a colon and English does not.
    pub director: &'static str,
    pub minutes: &'static str,
    pub votes_before: &'static str,
    pub votes_after: &'static str,
    pub pick_of: &'static str,
    pub watch: &'static str,
    pub previous: &'static str,
    pub next: &'static str,
    pub footer_note: &'static str,
    months: [&'static str; 12],
    /// The separator before the tenths of a rating: `7,9` here, `7.9` in English.
    decimal: char,
    /// The separator between thousands: `1.234` here, `1,234` in English.
    group: char,
    /// Only the two plural forms need to know which language they are in.
    german: bool,
}

const GERMAN: Strings = Strings {
    home: "Startseite",
    all_picks: "Alle Tipps",
    all_picks_so_far: "Alle bisherigen Tipps",
    all_picks_heading: "[ ALLE TIPPS ]",
    history_lead: "Jeden Tag ein anderer Film aus der Bibliothek — hier alle bisherigen.",
    no_pick: "Noch kein Tipp.",
    director: "Regie: ",
    minutes: "Min.",
    votes_before: "bei ",
    votes_after: " Stimmen",
    pick_of: "Tipp vom ",
    watch: "In Jellyfin ansehen",
    previous: "Vortag",
    next: "Folgetag",
    footer_note: "Morgen früh sucht der Server den nächsten Film aus. Ohne Konto, ohne Cloud.",
    months: [
        "Januar",
        "Februar",
        "März",
        "April",
        "Mai",
        "Juni",
        "Juli",
        "August",
        "September",
        "Oktober",
        "November",
        "Dezember",
    ],
    decimal: ',',
    group: '.',
    german: true,
};

const ENGLISH: Strings = Strings {
    home: "Home",
    all_picks: "All picks",
    all_picks_so_far: "All picks so far",
    all_picks_heading: "[ ALL PICKS ]",
    history_lead: "A different film from the library every day — every one of them so far.",
    no_pick: "No pick yet.",
    director: "Directed by ",
    minutes: "min",
    votes_before: "from ",
    votes_after: " votes",
    pick_of: "Pick of ",
    watch: "Watch in Jellyfin",
    previous: "Previous day",
    next: "Next day",
    footer_note: "Tomorrow morning the server picks the next one. No account, no cloud.",
    months: [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ],
    decimal: '.',
    group: ',',
    german: false,
};

impl Strings {
    /// `de` (with or without a region) is German, everything else English —
    /// the fallback is a language, not an error: a page in the wrong language
    /// is still a page.
    pub fn for_language(language: &str) -> Strings {
        let primary = language.split(['-', '_']).next().unwrap_or("");
        if primary.eq_ignore_ascii_case("de") {
            GERMAN
        } else {
            ENGLISH
        }
    }

    /// `2026-09` becomes `[ SEPTEMBER 2026 ]`. An unparseable key is passed
    /// through rather than dropped: a heading nobody planned for is still
    /// better than a month of picks without one.
    pub fn month_heading(&self, year_month: &str) -> String {
        let (year, month) = match year_month.split_once('-') {
            Some((y, m)) => (y, m),
            None => return format!("[ {} ]", year_month.to_uppercase()),
        };
        match month.parse::<usize>() {
            Ok(m) if (1..=12).contains(&m) => {
                format!("[ {} {} ]", self.months[m - 1].to_uppercase(), year)
            }
            _ => format!("[ {} ]", year_month.to_uppercase()),
        }
    }

    /// A rating with one decimal, in the separator of the language.
    pub fn decimal(&self, value: f64) -> String {
        let plain = format!("{value:.1}");
        if self.decimal == '.' {
            plain
        } else {
            plain.replace('.', &self.decimal.to_string())
        }
    }

    /// `6500` becomes `6.500` in German and `6,500` in English.
    pub fn integer(&self, value: i64) -> String {
        let digits = value.unsigned_abs().to_string();
        let mut out = String::with_capacity(digits.len() + digits.len() / 3 + 1);
        if value < 0 {
            out.push('-');
        }
        for (i, c) in digits.chars().enumerate() {
            if i > 0 && (digits.len() - i).is_multiple_of(3) {
                out.push(self.group);
            }
            out.push(c);
        }
        out
    }

    /// The subtitle of the history: how many picks there have been.
    pub fn picks_so_far(&self, count: usize) -> String {
        match (self.german, count) {
            (true, 1) => "Bisher ein Tipp.".to_string(),
            (true, n) => format!("Bisher {} Tipps.", self.integer(n as i64)),
            (false, 1) => "One pick so far.".to_string(),
            (false, n) => format!("{} picks so far.", self.integer(n as i64)),
        }
    }
}

/// The host out of a URL, without pulling in a URL parser for one line of
/// prompt decoration. Everything before `://` is a scheme, everything from the
/// first `/`, `?` or `#` on is a path, and anything before an `@` is userinfo.
/// A port stays: `home.example:8443` is what a terminal would print too.
pub fn host_of(url: &str) -> &str {
    let after_scheme = url
        .split_once("://")
        .map_or(url, |(_, rest)| rest)
        .trim_start_matches("//");
    let authority = after_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or(after_scheme);
    let host = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    if host.is_empty() { "localhost" } else { host }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn german_is_german_and_everything_else_is_english() {
        assert_eq!(Strings::for_language("de").watch, "In Jellyfin ansehen");
        assert_eq!(Strings::for_language("de-DE").watch, "In Jellyfin ansehen");
        assert_eq!(Strings::for_language("en").watch, "Watch in Jellyfin");
        assert_eq!(Strings::for_language("fr").watch, "Watch in Jellyfin");
        assert_eq!(Strings::for_language("").watch, "Watch in Jellyfin");
    }

    #[test]
    fn numbers_carry_the_separators_of_their_language() {
        let de = Strings::for_language("de");
        let en = Strings::for_language("en");
        assert_eq!(de.decimal(7.4), "7,4");
        assert_eq!(en.decimal(7.4), "7.4");
        assert_eq!(de.integer(1234), "1.234");
        assert_eq!(en.integer(1234), "1,234");
        assert_eq!(de.integer(999), "999");
        assert_eq!(de.integer(1234567), "1.234.567");
        assert_eq!(en.integer(-1234), "-1,234");
        assert_eq!(de.integer(0), "0");
    }

    #[test]
    fn a_month_key_becomes_a_heading_in_the_right_language() {
        assert_eq!(
            Strings::for_language("de").month_heading("2026-09"),
            "[ SEPTEMBER 2026 ]"
        );
        assert_eq!(
            Strings::for_language("en").month_heading("2026-01"),
            "[ JANUARY 2026 ]"
        );
        // Nothing to parse: the key survives instead of the page losing a heading.
        assert_eq!(
            Strings::for_language("en").month_heading("2026-13"),
            "[ 2026-13 ]"
        );
        assert_eq!(Strings::for_language("en").month_heading(""), "[  ]");
    }

    #[test]
    fn counting_picks_keeps_the_singular() {
        let de = Strings::for_language("de");
        let en = Strings::for_language("en");
        assert_eq!(de.picks_so_far(1), "Bisher ein Tipp.");
        assert_eq!(de.picks_so_far(1200), "Bisher 1.200 Tipps.");
        assert_eq!(en.picks_so_far(1), "One pick so far.");
        assert_eq!(en.picks_so_far(2), "2 picks so far.");
    }

    #[test]
    fn the_host_comes_out_of_whatever_the_operator_configured() {
        assert_eq!(host_of("https://home.example/"), "home.example");
        assert_eq!(host_of("https://rusty-vault.de"), "rusty-vault.de");
        assert_eq!(
            host_of("http://user@host.example:8443/x?y#z"),
            "host.example:8443"
        );
        assert_eq!(host_of("//home.example/x"), "home.example");
        // The default `home_url` is a bare path — a prompt still needs a name.
        assert_eq!(host_of("/"), "localhost");
        assert_eq!(host_of(""), "localhost");
    }
}
