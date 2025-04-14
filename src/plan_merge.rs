use std::{
    collections::HashSet,
    fs::{File, create_dir_all},
    io::{Read, Write},
    path::PathBuf,
    thread,
    time::Instant,
};

use anyhow::{Result, bail};

use flume::{Receiver, Sender};
use log::{error, info};
use walkdir::WalkDir;
use xmltree::Element as XmlElement;

use crate::{
    http_stuff::{build_client, get_subbed_url, periodic_info},
    response_data::GOLD_FILE,
    run_state::State,
};

pub fn base_path_from_group_and_artifact(group_id: &str, artifact_id: &str) -> String {
    format!("{}/{}/", group_id.replace(".", "/"), artifact_id,)
}
pub fn version_from_metadata(metadata: &Vec<u8>) -> Result<(String, String, Vec<String>)> {
    let md = XmlElement::parse(metadata.as_slice())?;

    // if let Some(md) = xml.get_child("metadata") {
    let group_id: Option<String> = md
        .get_child("groupId")
        .and_then(|e| e.get_text().map(|t| t.to_string()));
    let artifact_id = md
        .get_child("artifactId")
        .and_then(|e| e.get_text().map(|t| t.to_string()));
    let versions: Option<Vec<String>> = md
        .get_child("versioning")
        .and_then(|e| e.get_child("versions"))
        .map(|e| {
            e.children
                .iter()
                .flat_map(|n| n.as_element())
                .filter(|e| e.name == "version")
                .flat_map(|e| e.get_text().map(|t| t.to_string()))
                .collect()
        });

    match (group_id, artifact_id, versions) {
        (Some(group), Some(artifact), Some(vers)) if vers.len() > 0 => {
            let base_path = base_path_from_group_and_artifact(&group, &artifact);
            let mut ret = vec![];
            for v in vers {
                for s in suffixes() {
                    let url = format!("{}{}/{}-{}{}", base_path, v, artifact, v, s);
                    ret.push(url);
                }
            }

            Ok((group, artifact, ret))
        }
        (group_id, artifact_id, versions) => {
            bail!(
                "Could not find all the fields group {:?} artifact {:?} versions {:?}",
                group_id,
                artifact_id,
                versions
            )
        }
    }
}
pub fn suffixes() -> Vec<&'static str> {
    vec![".jar", "-javadoc.jar", "-sources.jar", ".pom"]
}

#[derive(Debug, Clone)]
pub struct MergeGroup {
    entries: Vec<MergeEntry>,
    group_id: String,
    artifact_id: String,
}

#[derive(Debug, Clone)]
pub struct MergeEntry {
    pub source_url: Option<String>,
    pub source_file: Option<PathBuf>,
    pub dest_file: PathBuf,
    pub state: State,
}

fn read_stream_and_do_merge(rx: Receiver<MergeGroup>, _state: State) -> Result<()> {
    let mut client = build_client();

    let mut loop_cnt = 0;

    for merge_grp in rx {
        let mut files = vec![];
        let start = Instant::now();
        for to_process in &merge_grp.entries {
            match to_process {
                MergeEntry {
                    source_url: Some(source_url),
                    source_file: None,
                    dest_file,
                    state,
                } => {
                    let url = format!("{}/{}", state.repo_url()?, source_url);
                    match get_subbed_url(&url, &mut client, state.clone()) {
                        Ok(loaded) => {
                            create_dir_all(match dest_file.parent() {
                                Some(f) => f,
                                None => bail!("Couldn't get parent directory for {:?}", dest_file),
                            })?;
                            let mut out_file = File::create(&dest_file)?;
                            out_file.write_all(&loaded.data())?;
                        }
                        Err(_) => {
                            // log ?? dunno
                        }
                    }
                    files.push(dest_file);
                }
                MergeEntry {
                    source_url: None,
                    source_file: Some(source_file),
                    dest_file,
                    state: _,
                } => {
                    let mut bytes = vec![];
                    let mut in_file = File::open(&source_file)?;
                    in_file.read_to_end(&mut bytes)?;
                    create_dir_all(match dest_file.parent() {
                        Some(f) => f,
                        None => bail!("Couldn't get parent directory for {:?}", dest_file),
                    })?;
                    let mut out_file = File::create(&dest_file)?;
                    out_file.write_all(&bytes)?;

                    files.push(dest_file);
                }
                me => {
                    bail!("Got weird merge entry {:?}", me);
                }
            }
        }

        loop_cnt += 1;
        if loop_cnt % 500 == 0 || merge_grp.entries.len() > 500 {
            info!(
                "Done {}/{} cnt {}, took {:?}",
                merge_grp.group_id,
                merge_grp.artifact_id,
                merge_grp.entries.len(),
                Instant::now().duration_since(start)
            );
        }
    }
    Ok(())
}

pub fn do_merge(state: State) -> Result<()> {
    let (tx, rx) = flume::bounded(30);

    for x in 0..state.max_threads() {
        let rx_clone = rx.clone();
        let state_clone = state.clone();
        thread::spawn(move || {
            state_clone.inc_running_threads();
            match read_stream_and_do_merge(rx_clone, state_clone.clone()) {
                Ok(_) => info!("Thread {} terminated normally", x),
                Err(e) => error!("Thread {} terminated abnormally {:?}", x, e),
            }
            state_clone.dec_running_threads();
        });
    }
    drop(rx);

    periodic_info(state.clone());

    plan_merge(tx, state)
}

pub fn plan_merge_to_console(state: State) -> Result<()> {
    let (tx, rx) = flume::bounded::<MergeGroup>(100);
    let _state_clone = state.clone();
    thread::spawn(move || {
        for x in rx {
            for y in x.entries {
                println!("{:?}", y);
            }
        }
    });

    plan_merge(tx, state)
}

pub fn plan_merge(dest: Sender<MergeGroup>, state: State) -> Result<()> {
    let crawl_db = state.latest_crawl()?;
    let start = Instant::now();
    let artifact_db = state.artifact_db()?;
    let mut meta_data_in_crawl = vec![];
    info!("Planning merge... looking at {:?}", crawl_db);
    for entry in WalkDir::new(&crawl_db).into_iter().filter_map(|e| e.ok()) {
        if entry.path().file_name().and_then(|f| f.to_str()) == Some(GOLD_FILE) {
            meta_data_in_crawl.push(entry.path().to_path_buf());
        }
    }
    info!(
        "In {:?} found {} entries",
        crawl_db,
        meta_data_in_crawl.len()
    );

    for (crawl_id, crawl_md) in meta_data_in_crawl.iter().enumerate() {
        let mut md_bytes = vec![];
        // in a block so the file gets closed
        {
            let mut f = File::open(&crawl_md)?;
            f.read_to_end(&mut md_bytes)?;
        }
        let (group_id, artifact_id, add_files) = version_from_metadata(&md_bytes)?;
        if crawl_id > 0 && crawl_id % 1000 == 0 {
            let run_time = Instant::now().duration_since(start).as_secs() as f64;
            let total = meta_data_in_crawl.len();
            let multiplier = (total as f64) / (crawl_id as f64);
            let est_hrs = (run_time * multiplier) / (60f64 * 60f64);
            let est_gb =
                (state.get_total_bytes() as f64 * multiplier) / (1024f64 * 1024f64 * 1024f64);
            info!(
                "Crawl entry {} of {}, {}/{} est time {} hrs, est total xfer {} gb",
                crawl_id,
                meta_data_in_crawl.len(),
                group_id,
                artifact_id,
                est_hrs,
                est_gb
            );
        }
        let path_to = base_path_from_group_and_artifact(&group_id, &artifact_id);
        let mut art_bytes = vec![];
        let artifact_gold_file = artifact_db.join(format!("{}/{}", path_to, GOLD_FILE));
        {
            match File::open(&artifact_gold_file) {
                Err(_) => {} // don't read bytes
                Ok(mut f) => {
                    let _ = f.read_to_end(&mut art_bytes);
                }
            }
        }

        // if the two files are the same, process the next one
        if art_bytes == md_bytes {
            continue;
        }

        let (_art_group_id, _art_artifact_id, art_add_files) =
            match version_from_metadata(&art_bytes) {
                Ok(v) => v,
                Err(_) => (group_id.clone(), artifact_id.clone(), vec![]),
            };

        let mut diff_files = HashSet::new();
        // all the potential files from the current crawl's maven metadata
        for v in &add_files {
            diff_files.insert(v.clone());
        }

        // subtract the files from the artifact DB's maven metadata
        for v in &art_add_files {
            diff_files.remove(v);
        }

        let mut to_send = vec![];

        for url in diff_files {
            let dest_file = artifact_db.join(&url);
            to_send.push(MergeEntry {
                source_url: Some(url),
                source_file: None,
                dest_file,
                state: state.clone(),
            });
        }

        to_send.push(MergeEntry {
            source_url: None,
            source_file: Some(crawl_md.clone()),
            dest_file: artifact_gold_file,
            state: state.clone(),
        });

        dest.send(MergeGroup {
            entries: to_send,
            group_id,
            artifact_id,
        })?;
    }

    Ok(())
}
