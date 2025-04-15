use std::{
    thread::{self, sleep},
    time::Duration,
};

use crate::{
    plan_merge::version_from_metadata,
    response_data::{GOLD_FILE, ResponseData},
    run_state::State,
};
use anyhow::{Result, bail};
use log::{error, info};
use reqwest::blocking::{Client, ClientBuilder, Response};
use thousands::Separable;

pub fn build_client() -> Client {
    ClientBuilder::new()
        .user_agent("Spice Labs https://spicelabs.io")
        .build()
        .expect("Should build HTTP client")
}

/// if the current link queue is < 10,000 add the links to the link queue
/// otherwise process in current thread
pub fn should_do_links(links: &Vec<String>, state: State) -> bool {
    if links.len() <= 1 {
        true
    } else if state.queue_len() < 10_000 || links.len() > 15 {
        for link in links {
            state.push_page(&link);
        }

        false
    } else {
        true
    }
}

pub fn process_page(url: String, client: &mut Client, depth: usize, state: State) -> Result<usize> {
    let mut processed_cnt = 0;

    let page: ResponseData = get_url(&state.repo_url()?, &url, client, state.clone())?;

    if page.mime_type().starts_with("text/html") {
        let links = page.html_to_links();
        let gold_links: Vec<String> = links
            .iter()
            .filter(|v| v.ends_with(GOLD_FILE))
            .map(|v| v.to_string())
            .collect();

        let mut load_links = true;

        // we found a maven metadata file
        if gold_links.len() > 0 {
            // for each one (there should only by 1)
            for gold_link in gold_links {
                // get the file from the server
                match get_subbed_url(&gold_link, client, state.clone()) {
                    Ok(page) => {
                        match version_from_metadata(page.data()) {
                            Ok(_to_load) => {
                                // it's valid, save it and don't load links
                                load_links = false;

                                page.save()?;
                            }
                            Err(_e) => {
                                // if we can't parse the metadata, then continue
                                // into the page
                                load_links = true;
                                break;
                            }
                        }
                    }
                    Err(e) => {
                        error!("Failed to fetch {} err {:?}", gold_link, e)
                    }
                }
            }
        }

        if load_links {
            if should_do_links(&links, state.clone()) {
                for link in links {
                    if !link.ends_with(".xml") {
                        processed_cnt += 1;
                        match process_page(link.clone(), client, depth + 1, state.clone()) {
                            Ok(sub_cnt) => {
                                processed_cnt += sub_cnt;
                            }
                            Err(e) => {
                                error!("Failed to load {}, error {:?}", link, e);
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(processed_cnt)
}

/// based on the number of threads in 429, delay
fn delay_429(state: State) {
    let threads_in_429 = state.get_429_cnt();
    if threads_in_429 > 0 {
        // for every thread in 429, back off 100 ms for all request
        // threads
        sleep(Duration::from_millis(100 * threads_in_429));
    }
}

pub fn get_subbed_url(url: &str, client: &mut Client, state: State) -> Result<ResponseData> {
    let ret = match state.mirror_url() {
        Some(mirror) => {
            let first = format!("{}{}", mirror, &url[state.repo_url()?.len()..]);
            match get_url(&mirror, &first, client, state.clone()) {
                Ok(v) => Ok(v),
                Err(_) => get_url(&state.repo_url()?, url, client, state.clone()),
            }
        }
        None => get_url(&state.repo_url()?, url, client, state.clone()),
    };
    if ret.is_ok() {
        state.inc_asset_fetch_cnt();
    }
    ret
}

/// remove double '/' from the URL
fn fix_url(url: &str) -> String {
    let mut ret = String::new();
    let mut last_slash = false;
    for (idx, c) in url.chars().enumerate() {
        if idx > 8 {
            if c == '/' {
                if !last_slash {
                    ret.push(c);
                }
                last_slash = true;
            } else {
                last_slash = false;
                ret.push(c);
            }
        } else {
            ret.push(c);
        }
    }

    ret
}

pub fn get_url(
    server_prefix: &str,
    url: &str,
    client: &mut Client,
    state: State,
) -> Result<ResponseData> {
    delay_429(state.clone());

    let url = fix_url(url);

    let info: Response = {
        // loop 6 times trying to get the page...
        let mut try_cnt = 0;
        loop {
            let val = client.get(&url).send();
            match val {
                Ok(x) => {
                    break x;
                }
                Err(e) => {
                    if try_cnt > 5 {
                        bail!("Failed to get url {} error {:?}", url, e);
                    }
                    try_cnt += 1;
                }
            }
        }
    };

    let response_code = info.status();
    if response_code.as_u16() == 429 {
        let cnt_429 = state.inc_429_cnt();
        info!("429 count {} url {}", cnt_429, url);
        sleep(Duration::from_millis(350));
        let ret = get_url(server_prefix, &url, client, state.clone());
        state.dec_429_cnt();
        return ret;
    }

    if !info.status().is_success() {
        bail!("Failed to load {} status {}", url, info.status());
    }

    let cnt = state.inc_fetch_cnt();
    if cnt % 10_000 == 0 {
        info!("Fetch {} cnt {}", url, cnt.separate_with_commas());
    }
    let content_type = match info.headers().get("content-type") {
        Some(v) => v.to_str()?.to_string(),
        None => "????".to_string(),
    };
    let bytes = info.bytes()?;
    let v: Vec<u8> = bytes.into();
    ResponseData::new(
        url.to_string(),
        server_prefix,
        v,
        content_type.to_string(),
        state.clone(),
    )
}

pub fn spawn_a_page(state: State) {
    // increment the running thread before we return from this method
    // to avoid a race condition for shutting down the program
    let x = state.inc_running_threads();
    thread::spawn(move || {
        let mut client = build_client();
        let mut page_loop = 0;
        while let Some(page_to_process) = state.next_page() {
            page_loop += 1;
            match process_page(page_to_process.clone(), &mut client, 0, state.clone()) {
                Ok(processed_cnt) => {
                    if false && page_loop % 100 == 0 {
                        info!(
                            "Page process thread {} got to top, cnt {} page {} queue size {}",
                            x,
                            processed_cnt.separate_with_commas(),
                            page_to_process.separate_with_commas(),
                            state.queue_len().separate_with_commas()
                        );
                    }
                }
                Err(e) => error!(
                    "Page process thread {} got to top with error {:?}, {}",
                    x, e, page_to_process
                ),
            }
        }
        state.dec_running_threads();
    });
}

pub fn periodic_info(state: State) {
    thread::spawn(move || {
        while state.thread_cnt() > 0 {
            sleep(Duration::from_secs(30));
            info!(
                "At {:?} threads {} urls {} assets {} queue size {} loaded {}gb",
                state.run_duration(),
                state.thread_cnt(),
                state.urls_fetched().separate_with_commas(),
                state.assets_fetched().separate_with_commas(),
                state.queue_len().separate_with_commas(),
                (state.get_total_bytes() / (1024 * 1024 * 1024)).separate_with_commas()
            );
        }
    });
}
