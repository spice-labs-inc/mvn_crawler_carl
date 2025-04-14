use std::path::PathBuf;

use anyhow::{Result, bail};
use clap::{Parser, arg, command};

/// Simple program to greet a person
#[derive(Parser, Debug, Clone)]
#[command(version, about, long_about = None)]
pub struct Args {
    /// URL of the Maven Repo
    #[arg(short, long)]
    repo: Option<String>,

    /// where to put the result
    #[arg(short, long)]
    crawl_db: PathBuf,

    /// a URL to substitute when fetching XML, jars, etc
    #[arg(short, long)]
    mirror: Option<String>,

    // /// should load JARs or simply keep the metadata around
    // #[arg(long, default_value_t = false, action)]
    // load_jars: bool,
    /// plan the download
    #[arg(long, default_value_t = false, action)]
    plan: bool,

    /// the directory where the artifacts are stored
    #[arg(long)]
    artifact_db: Option<PathBuf>,

    /// update the artifact_db from the latest crawl
    #[arg(long, default_value_t = false, action)]
    reify_artifact_db: bool,

    /// maximum number of threads, default to 200
    #[arg(long)]
    max_threads: Option<usize>,
}

impl Args {
    /// update the artifact_db from the latest crawl
    pub fn reify_artifact_db(&self) -> bool {
        self.reify_artifact_db
    }

    pub fn max_threads(&self) -> usize {
        self.max_threads.unwrap_or(200)
    }
    /// Substitute a URL when fetching an asset
    pub fn mirror_url(&self) -> &Option<String> {
        &self.mirror
    }
    pub fn repo_url(&self) -> Option<String> {
        self.repo.clone()
    }

    /// get the destination for the crawl data
    pub fn crawl_db(&self) -> PathBuf {
        self.crawl_db.clone()
    }

    /// should we plan what happens on a merge?
    pub fn plan(&self) -> bool {
        self.plan
    }

    pub fn artifact_db(&self) -> Result<PathBuf> {
        match &self.artifact_db {
            Some(v) => Ok(v.clone()),
            None => {
                bail!("Artifact db directory must be specified with the `--artifact-db` parameter")
            }
        }
    }
}
