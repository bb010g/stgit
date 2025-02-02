// SPDX-License-Identifier: GPL-2.0-only

//! `stg diff` implementation.

use std::path::PathBuf;

use anyhow::Result;
use clap::{Arg, ArgMatches, ValueHint};

use crate::{
    argset,
    revspec::{parse_stgit_revision, Error as RevError},
    stupid::Stupid,
};

pub(super) const STGIT_COMMAND: super::StGitCommand = super::StGitCommand {
    name: "diff",
    category: super::CommandCategory::PatchInspection,
    make,
    run,
};

fn make() -> clap::Command<'static> {
    clap::Command::new(STGIT_COMMAND.name)
        .about("Show a diff")
        .long_about(
            "Show the diff (default) or diffstat between the current working copy \
             or a tree-ish object and another tree-ish object (defaulting to HEAD). \
             File names can also be given to restrict the diff output. The \
             tree-ish object has the format accepted by the 'stg id' command.",
        )
        .arg(
            Arg::new("pathspecs")
                .help("Limit diff to files matching path(s)")
                .value_name("path")
                .multiple_values(true)
                .value_parser(clap::value_parser!(PathBuf))
                .value_hint(ValueHint::AnyPath),
        )
        .arg(
            Arg::new("range")
                .long("range")
                .short('r')
                .help("Show the diff between specified revisions")
                .long_help(
                    "Show diff between specified revisions. \
                     Revisions ranges are specified as 'rev1[..[rev2]]'. \
                     The revisions may be standard Git revision specifiers or \
                     patches.",
                )
                .value_name("revspec"),
        )
        .arg(
            Arg::new("stat")
                .long("stat")
                .short('s')
                .help("Show the stat instead of the diff"),
        )
        .arg(argset::diff_opts_arg())
}

fn run(matches: &ArgMatches) -> Result<()> {
    let repo = git2::Repository::open_from_env()?;
    let config = repo.config()?;

    let revspec = if let Some(range_str) = matches.get_one::<String>("range") {
        if let Some((rev1, rev2)) = range_str.split_once("..") {
            if rev1.is_empty() {
                return Err(RevError::InvalidRevision(range_str.to_string()).into());
            }
            let rev1 = parse_stgit_revision(&repo, Some(rev1), None)?;
            if rev2.is_empty() {
                format!("{}..", rev1.id())
            } else {
                let rev2 = parse_stgit_revision(&repo, Some(rev2), None)?;
                format!("{}..{}", rev1.id(), rev2.id())
            }
        } else {
            let rev1 = parse_stgit_revision(&repo, Some(range_str), None)?;
            rev1.id().to_string()
        }
    } else {
        "HEAD".to_string()
    };

    repo.stupid().diff(
        &revspec,
        matches.get_many::<PathBuf>("pathspecs"),
        matches.contains_id("stat"),
        crate::color::use_color(matches),
        &argset::get_diff_opts(matches, &config, false, false),
    )
}
