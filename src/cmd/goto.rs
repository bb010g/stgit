// SPDX-License-Identifier: GPL-2.0-only

//! `stg goto` implementation.

use std::str::FromStr;

use anyhow::{anyhow, Result};
use clap::{builder::ValueParser, Arg, ArgMatches};

use crate::{
    argset,
    color::get_color_stdout,
    patchname::PatchName,
    patchrange,
    repo::RepositoryExtended,
    stack::{Stack, StackStateAccess},
    stupid::Stupid,
};

pub(super) const STGIT_COMMAND: super::StGitCommand = super::StGitCommand {
    name: "goto",
    category: super::CommandCategory::StackManipulation,
    make,
    run,
};

fn make() -> clap::Command<'static> {
    clap::Command::new(STGIT_COMMAND.name)
        .about("Go to patch by pushing or popping as necessary")
        .arg(argset::keep_arg())
        .arg(argset::merged_arg())
        .arg(
            Arg::new("patch")
                .help("Patch to go to")
                .required(true)
                .value_parser(ValueParser::new(PatchName::from_str)),
        )
}

fn run(matches: &ArgMatches) -> Result<()> {
    let repo = git2::Repository::open_from_env()?;
    let stack = Stack::from_branch(&repo, None)?;
    let stupid = repo.stupid();

    let patch_arg = matches.get_one::<PatchName>("patch").unwrap();
    let opt_keep = matches.contains_id("keep");
    let opt_merged = matches.contains_id("merged");

    repo.check_repository_state()?;
    let statuses = stupid.statuses(None)?;
    statuses.check_conflicts()?;
    stack.check_head_top_mismatch()?;
    if !opt_keep {
        statuses.check_index_and_worktree_clean()?;
    }

    let patchname = match patchrange::parse_single(patch_arg, &stack, patchrange::Allow::Visible) {
        Ok(patchname) => Ok(patchname),
        Err(e @ patchrange::Error::PatchNotKnown { .. }) => {
            let oid_prefix: &str = patch_arg.as_ref();
            if oid_prefix.len() >= 4 && oid_prefix.chars().all(|c| c.is_ascii_hexdigit()) {
                let oid_matches: Vec<&PatchName> = stack
                    .all_patches()
                    .filter(|pn| {
                        stack
                            .get_patch(pn)
                            .commit
                            .id()
                            .to_string()
                            .chars()
                            .zip(oid_prefix.chars())
                            .all(|(a, b)| a.eq_ignore_ascii_case(&b))
                    })
                    .collect();

                match oid_matches.len() {
                    0 => Err(anyhow!("No patch associated with `{oid_prefix}`")),
                    1 => Ok(oid_matches[0].clone()),
                    _ => {
                        println!("Possible patches:");
                        for pn in oid_matches {
                            println!("  {pn}");
                        }
                        Err(anyhow!("Ambiguous commit id `{oid_prefix}`"))
                    }
                }
            } else {
                Err(e.into())
            }
        }
        Err(e) => Err(e.into()),
    }?;

    stack
        .setup_transaction()
        .use_index_and_worktree(true)
        .with_output_stream(get_color_stdout(matches))
        .transact(|trans| {
            if let Some(pos) = trans.applied().iter().position(|pn| pn == &patchname) {
                let applied = trans.applied()[0..=pos].to_vec();
                let mut unapplied = trans.applied()[pos + 1..].to_vec();
                unapplied.extend(trans.unapplied().iter().cloned());
                trans.reorder_patches(Some(&applied), Some(&unapplied), None)
            } else {
                let pos = trans
                    .unapplied()
                    .iter()
                    .position(|pn| pn == &patchname)
                    .expect("already determined patch exists and not hidden or applied");

                let to_apply: Vec<PatchName> = trans.unapplied()[0..pos + 1].to_vec();
                trans.push_patches(&to_apply, opt_merged)?;
                Ok(())
            }
        })
        .execute("goto")?;

    Ok(())
}
