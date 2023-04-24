/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::env;
use std::error::Error;
use std::io;
use std::path::PathBuf;

extern crate clap;
#[macro_use]
extern crate log;
extern crate proc_macro2;
#[macro_use]
extern crate serde;
extern crate serde_json;
#[macro_use]
extern crate quote;
#[macro_use]
extern crate syn;
extern crate toml;

use clap::{ArgAction, Parser};

mod bindgen;
mod logging;

use crate::bindgen::{Builder, Cargo, Config, Language, Profile, Style};

/// Generate C bindings for a Rust library
#[derive(Debug, Parser)]
#[clap(about)]
struct Args {
    /// Enable verbose logging
    #[clap(short, action = ArgAction::Count)]
    verbosity: u8,
    /// Generate bindings and compare it to the existing bindings file and error if they are different
    #[clap(long)]
    verify: bool,
    /// Specify path to a `cbindgen.toml` config to use
    #[clap(short, long, value_name = "PATH")]
    config_path: Option<PathBuf>,
    /// Specify the language to output bindings in
    #[clap(short, long = "lang", value_name = "LANGUAGE")]
    language: Option<Language>,
    /// Whether to add C++ compatibility to generated C bindings
    #[clap(long)]
    cpp_compat: bool,
    /// Only fetch dependencies needed by the target platform. The target platform defaults to the
    /// host platform; set TARGET to override.
    #[clap(long)]
    only_target_dependencies: bool,
    /// Specify the declaration style to use for bindings
    #[clap(short, long)]
    style: Option<Style>,
    /// Whether to parse dependencies when generating bindings
    #[clap(short, long)]
    parse_dependencies: bool,
    /// Whether to use a new temporary directory for expanding macros. Affects performance, but
    /// might be required in certain build processes.
    #[clap(long)]
    clean: bool,
    /// A crate directory or source file to generate bindings for. In general this is the folder
    /// where the Cargo.toml file of source Rust library resides.
    input: Option<PathBuf>,
    /// If generating bindings for a crate, the specific crate to generate bindings for
    #[clap(long = "crate")]
    crate_name: Option<String>,
    /// The file to output the bindings to
    #[clap(long, short, value_name = "PATH")]
    output: Option<PathBuf>,
    /// Specify the path to the Cargo.lock file explicitly. If this qs not specified, the
    /// Cargo.lock file is searched for in the same folder as the Cargo.toml file. This option is
    /// useful for projects that use workspaces.
    #[clap(long, value_name = "PATH")]
    lockfile: Option<String>,
    /// Specify the path to the output of a `cargo metadata` command that allows to get dependency
    /// information. This is useful because cargo metadata may be the longest part of cbindgen
    /// runtime, and you may want to share it across cbindgen invocations. By default cbindgen will
    /// run `cargo metadata --all-features --format-version 1 --manifest-path
    /// <path/to/crate/Cargo.toml>
    #[clap(long, value_name = "PATH")]
    metadata: Option<PathBuf>,
    /// Specify the profile to use when expanding macros. Has no effect otherwise.
    #[clap(long)]
    profile: Option<Profile>,
    /// Report errors only (overrides verbosity options).
    #[clap(long, short)]
    quiet: bool,
}

fn main() {
    let args = Args::parse();

    match run(args) {
        Ok(()) => (),
        Err(e) => {
            error!("{}", e);
            error!("{}", e);
        }
    }
}

fn run(args: Args) -> Result<(), Box<dyn Error>> {
    let Args {
        verbosity,
        verify,
        config_path,
        language,
        cpp_compat,
        only_target_dependencies,
        style,
        parse_dependencies,
        clean,
        input,
        crate_name,
        output,
        lockfile,
        metadata,
        profile,
        quiet,
    } = args;
    if output.is_none() && verify {
        error!(
            "Cannot verify bindings against `stdout`, please specify a file to compare against."
        );
        std::process::exit(2);
    }

    // Initialize logging
    if quiet {
        logging::ErrorLogger::init().unwrap();
    } else {
        match verbosity {
            0 => logging::WarnLogger::init().unwrap(),
            1 => logging::InfoLogger::init().unwrap(),
            _ => logging::TraceLogger::init().unwrap(),
        }
    }

    // Find the input directory
    let input = input.unwrap_or_else(|| env::current_dir().unwrap());

    let apply_config_overrides = move |config: &mut Config| {
        // We allow specifying a language to override the config default. This is
        // used by compile-tests.
        if let Some(lang) = language {
            config.language = lang;
        }

        if cpp_compat {
            config.cpp_compat = true;
        }

        if only_target_dependencies {
            config.only_target_dependencies = true;
        }

        if let Some(style) = style {
            config.style = style;
        }

        if let Some(profile) = profile {
            config.parse.expand.profile = profile;
        }

        if parse_dependencies {
            config.parse.parse_deps = true;
        }
    };

    // If a file is specified then we load it as a single source
    let bindings = if input.is_dir() {
        // Load any config specified or search in the input directory
        let mut config = config_path
            .map(|c| Config::from_file(c).unwrap())
            .unwrap_or_else(|| Config::from_root_or_default(&input));

        apply_config_overrides(&mut config);

        Builder::new()
            .with_config(config)
            .with_src(input)
            .generate()?
    } else {
        // We have to load a whole crate, so we use cargo to gather metadata
        let lib = Cargo::load(
            &input,
            lockfile.as_deref(),
            crate_name.as_deref(),
            true,
            clean,
            only_target_dependencies,
            metadata.as_deref(),
        )?;

        // Load any config specified or search in the binding crate directory
        let mut config = config_path
            .map(|c| Config::from_file(c).unwrap())
            .unwrap_or_else(|| {
                let binding_crate_dir = lib.find_crate_dir(&lib.binding_crate_ref());

                if let Some(binding_crate_dir) = binding_crate_dir {
                    Config::from_root_or_default(binding_crate_dir)
                } else {
                    // This shouldn't happen
                    Config::from_root_or_default(input)
                }
            });

        apply_config_overrides(&mut config);

        Builder::new()
            .with_config(config)
            .with_cargo(lib)
            .generate()?
    };

    // Write the bindings file
    match output {
        Some(file) => {
            let changed = bindings.write_to_file(&file);

            if verify && changed {
                error!("Bindings changed: {}", file.display());
                std::process::exit(2);
            }
        }
        _ => {
            bindings.write(io::stdout());
        }
    }

    Ok(())
}
