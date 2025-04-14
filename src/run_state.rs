use std::{
    collections::VecDeque,
    fs::create_dir_all,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
    time::{Duration, Instant, SystemTime},
};

use anyhow::{Result, bail};
use chrono::prelude::*;

use crate::args::Args;

/// The state of the running job
/// An `Arc` of this gets passed everywhere
/// so there's no global shared state
#[derive(Debug)]
pub struct RunState {
    args: Args,
    fetch_cnt: AtomicUsize,
    asset_fetch_cnt: AtomicUsize,
    threads_in_429: AtomicU64,
    queue: Mutex<VecDeque<String>>,
    running_threads: AtomicUsize,
    total_added_pages: AtomicUsize,
    total_bytes: AtomicUsize,
    start: Instant,
    start_time: SystemTime,
}

impl RunState {
    pub fn get_total_bytes(&self) -> usize {
        self.total_bytes.load(Ordering::Relaxed)
    }

    pub fn add_to_total_bytes(&self, bytes: usize) -> usize {
        self.total_bytes.fetch_add(bytes, Ordering::Relaxed) + bytes
    }
    pub fn get_429_cnt(&self) -> u64 {
        self.threads_in_429.load(Ordering::Relaxed)
    }

    pub fn inc_fetch_cnt(&self) -> usize {
        self.fetch_cnt.fetch_add(1, Ordering::Relaxed) + 1
    }

    pub fn dec_429_cnt(&self) -> u64 {
        self.threads_in_429.fetch_sub(1, Ordering::Relaxed) - 1
    }
    pub fn inc_429_cnt(&self) -> u64 {
        self.threads_in_429.fetch_add(1, Ordering::Relaxed) + 1
    }

    pub fn inc_running_threads(&self) -> usize {
        self.running_threads.fetch_add(1, Ordering::Relaxed) + 1
    }

    pub fn dec_running_threads(&self) -> usize {
        self.running_threads.fetch_sub(1, Ordering::Relaxed) - 1
    }

    pub fn inc_asset_fetch_cnt(&self) -> usize {
        self.asset_fetch_cnt.fetch_add(1, Ordering::Relaxed) + 1
    }
    pub fn new(args: Args) -> Arc<RunState> {
        Arc::new(RunState {
            args,
            total_bytes: AtomicUsize::new(0),
            fetch_cnt: AtomicUsize::new(0),
            asset_fetch_cnt: AtomicUsize::new(0),
            threads_in_429: AtomicU64::new(0),
            queue: Mutex::new(VecDeque::new()),
            running_threads: AtomicUsize::new(0),
            total_added_pages: AtomicUsize::new(0),
            start: Instant::now(),
            start_time: SystemTime::now(),
        })
    }

    pub fn current_time_millis() -> i64 {
        let dt: DateTime<Utc> = SystemTime::now().into();
        dt.timestamp_millis()
    }

    pub fn start_date_string(&self) -> String {
        let utc: DateTime<Utc> = self.start_time.into();
        utc.format("%Y_%m_%d_%H_%M_%S").to_string()
    }

    pub fn mirror_url(&self) -> &Option<String> {
        self.args.mirror_url()
    }

    pub fn repo_url(&self) -> Result<String> {
        match self.args.repo_url() {
            Some(v) => Ok(v),
            None => bail!("Repo URL not specified"),
        }
    }

    /// maximum number of threads
    pub fn max_threads(&self) -> usize {
        self.args.max_threads()
    }

    pub fn run_duration(&self) -> Duration {
        Instant::now().duration_since(self.start)
    }

    /// the directory to put info in
    pub fn crawl_db_dest_dir(&self) -> PathBuf {
        let sub_dir = format!("{}_crawl_db", self.start_date_string());
        let ret = self.args.crawl_db().join(sub_dir);
        if !ret.exists() {
            create_dir_all(&ret).expect("Should be able to create directory");
        }

        ret
    }

    /// should we "plan" what happens?
    pub fn plan(&self) -> bool {
        self.args.plan()
    }

    pub fn queue_len(&self) -> usize {
        self.queue.lock().expect("Lock queue").len()
    }
    pub fn thread_cnt(&self) -> usize {
        self.running_threads.load(Ordering::Relaxed)
    }
    pub fn urls_fetched(&self) -> usize {
        self.fetch_cnt.load(Ordering::Relaxed)
    }

    /// get the directory that contains the artifacts
    pub fn artifact_db(&self) -> Result<PathBuf> {
        self.args.artifact_db()
    }

    /// update the artifact_db from the latest crawl
    pub fn reify_artifact_db(&self) -> bool {
        self.args.reify_artifact_db()
    }

    /// get the directory that contains the latest crawl
    pub fn latest_crawl(&self) -> Result<PathBuf> {
        let dir = self.args.crawl_db();
        if !dir.exists() || !dir.is_dir() {
            bail!("The crawl directory {:?} isn't a directory", dir);
        }

        let mut sub_files: Vec<PathBuf> = vec![];

        for entry in dir.read_dir()? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                let file_name = entry.path().to_path_buf();
                sub_files.push(file_name);
            }
        }
        sub_files.sort();
        match sub_files.last() {
            None => {
                bail!("Could not find any entries in {:?}", dir);
            }
            Some(pb) => Ok(pb.clone()),
        }
    }

    pub fn assets_fetched(&self) -> usize {
        self.asset_fetch_cnt.load(Ordering::Relaxed)
    }

    pub fn push_page(&self, page: &str) {
        self.total_added_pages.fetch_add(1, Ordering::Relaxed);
        let mut queue = self.queue.lock().expect("Lock queue");
        queue.push_back(page.to_string())
    }

    pub fn next_page(&self) -> Option<String> {
        let mut queue = self.queue.lock().expect("Lock queue");
        queue.pop_front()
    }
}

pub type State = Arc<RunState>;
