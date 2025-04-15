use std::{thread::sleep, time::Duration};

use anyhow::Result;
use clap::Parser;
use env_logger::Env;
use log::info;
use mvn_crawler_carl::{
    args::Args,
    http_stuff::{periodic_info, spawn_a_page},
    plan_merge::{do_merge, plan_merge_to_console},
    run_state::RunState,
};

fn main() -> Result<()> {
    env_logger::Builder::from_env(
        Env::default()
            .filter_or("MY_LOG_LEVEL", "info")
            .write_style_or("MY_LOG_STYLE", "always"),
    )
    .init();
    let args = Args::parse();

    let state = RunState::new(args.clone());

    // should we plan the merge
    if state.plan() {
        plan_merge_to_console(state.clone())?;
        return Ok(());
    }

    // should we do the real merge?
    if state.reify_artifact_db() {
        info!("Started updating artifact DB");
        do_merge(state.clone())?;
        return Ok(());
    }

    state.push_page(&state.repo_url()?);
    info!("Kicking off run");
    spawn_a_page(state.clone());
    periodic_info(state.clone());
    while state.thread_cnt() > 0 {
        sleep(Duration::from_millis(200));

        let num_threads = state.thread_cnt();
        {
            if num_threads < state.max_threads() && num_threads * 40 < state.queue_len() {
                spawn_a_page(state.clone());
            }
        }
    }

    info!("At {:?}, done with run {:?}", state.run_duration(), state);

    Ok(())
}
