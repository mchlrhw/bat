// `error_chain!` can recurse deeply
#![recursion_limit = "1024"]

#[macro_use]
extern crate error_chain;

#[macro_use]
extern crate clap;

#[macro_use]
extern crate lazy_static;

extern crate ansi_term;
extern crate atty;
extern crate console;
extern crate directories;
extern crate git2;
extern crate syntect;

mod app;
mod assets;
mod controller;
mod decorations;
mod diff;
mod line_range;
mod output;
mod printer;
mod style;
mod terminal;

use std::collections::HashSet;
use std::io;
use std::io::stdout;
use std::io::Write;
use std::path::Path;
use std::process;

use ansi_term::Colour::Green;
use ansi_term::Style;

use app::{App, Config, InputFile};
use assets::{clear_assets, config_dir, HighlightingAssets};
use controller::Controller;
use style::{OutputComponent, OutputComponents};

mod errors {
    error_chain! {
        foreign_links {
            Clap(::clap::Error);
            Io(::std::io::Error);
            SyntectError(::syntect::LoadingError);
            ParseIntError(::std::num::ParseIntError);
        }
    }

    pub fn handle_error(error: &Error) {
        match error {
            &Error(ErrorKind::Io(ref io_error), _)
                if io_error.kind() == super::io::ErrorKind::BrokenPipe =>
            {
                super::process::exit(0);
            }
            _ => {
                use ansi_term::Colour::Red;
                eprintln!("{}: {}", Red.paint("[bat error]"), error);
            }
        };
    }
}

use errors::*;

fn run_cache_subcommand(matches: &clap::ArgMatches) -> Result<()> {
    if matches.is_present("init") {
        let source_dir = matches.value_of("source").map(Path::new);
        let target_dir = matches.value_of("target").map(Path::new);

        let blank = matches.is_present("blank");

        let assets = HighlightingAssets::from_files(source_dir, blank)?;
        assets.save(target_dir)?;
    } else if matches.is_present("clear") {
        clear_assets();
    } else if matches.is_present("config-dir") {
        writeln!(stdout(), "{}", config_dir())?;
    }

    Ok(())
}

pub fn list_languages(assets: &HighlightingAssets, term_width: usize) -> Result<()> {
    let mut languages = assets
        .syntax_set
        .syntaxes()
        .iter()
        .filter(|syntax| !syntax.hidden && !syntax.file_extensions.is_empty())
        .collect::<Vec<_>>();
    languages.sort_by_key(|lang| lang.name.to_uppercase());

    let longest = languages
        .iter()
        .map(|syntax| syntax.name.len())
        .max()
        .unwrap_or(32); // Fallback width if they have no language definitions.

    let comma_separator = ", ";
    let separator = " ";
    // Line-wrapping for the possible file extension overflow.
    let desired_width = term_width - longest - separator.len();

    for lang in languages {
        write!(
            stdout(),
            "{:width$}{}",
            lang.name,
            separator,
            width = longest
        )?;

        // Number of characters on this line so far, wrap before `desired_width`
        let mut num_chars = 0;

        let mut extension = lang.file_extensions.iter().peekable();
        while let Some(word) = extension.next() {
            // If we can't fit this word in, then create a line break and align it in.
            let new_chars = word.len() + comma_separator.len();
            if num_chars + new_chars >= desired_width {
                num_chars = 0;
                write!(stdout(), "\n{:width$}{}", "", separator, width = longest)?;
            }

            num_chars += new_chars;
            write!(stdout(), "{}", Green.paint(&word[..]))?;
            if extension.peek().is_some() {
                write!(stdout(), "{}", comma_separator)?;
            }
        }
        writeln!(stdout())?;
    }

    Ok(())
}

pub fn list_themes(assets: &HighlightingAssets, cfg: &Config) -> Result<()> {
    let themes = &assets.theme_set.themes;
    let mut config = cfg.clone();
    let mut style = HashSet::new();
    style.insert(OutputComponent::Plain);
    config.files = vec![InputFile::ThemePreviewFile];
    config.output_components = OutputComponents(style);
    for (theme, _) in themes.iter() {
        writeln!(
            stdout(),
            "Theme: {}\n",
            Style::new().bold().paint(theme.to_string())
        )?;
        config.theme = theme.to_string();
        let _controller = Controller::new(&config, &assets).run();
        writeln!(stdout())?;
    }

    Ok(())
}

/// Returns `Err(..)` upon fatal errors. Otherwise, returns `Some(true)` on full success and
/// `Some(false)` if any intermediate errors occurred (were printed).
fn run() -> Result<bool> {
    let app = App::new();

    match app.matches.subcommand() {
        ("cache", Some(cache_matches)) => {
            run_cache_subcommand(cache_matches)?;
            Ok(true)
        }
        _ => {
            let config = app.config()?;
            let assets = HighlightingAssets::new();

            if app.matches.is_present("list-languages") {
                list_languages(&assets, config.term_width)?;

                Ok(true)
            } else if app.matches.is_present("list-themes") {
                list_themes(&assets, &config)?;

                Ok(true)
            } else {
                let controller = Controller::new(&config, &assets);
                controller.run()
            }
        }
    }
}

fn main() {
    let result = run();

    match result {
        Err(error) => {
            handle_error(&error);
            process::exit(1);
        }
        Ok(false) => {
            process::exit(1);
        }
        Ok(true) => {
            process::exit(0);
        }
    }
}
