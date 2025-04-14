use anyhow::Result;
use scraper::{Html, Selector};
use std::{
    fs::{File, create_dir_all, remove_file},
    io::{Read, Write},
    path::PathBuf,
};

use log::error;

use crate::run_state::State;

pub struct ResponseData {
    url: String,
    data: Vec<u8>,
    mime_type: String,
    state: State,
}

impl ResponseData {
    pub fn new(
        url: String,
        data: Vec<u8>,
        mime_type: String,
        state: State,
    ) -> Result<ResponseData> {
        state.repo_url()?;
        state.add_to_total_bytes(data.len());
        Ok(ResponseData {
            url,
            data,
            mime_type,
            state,
        })
    }
    pub fn data(&self) -> &Vec<u8> {
        &self.data
    }

    pub fn mime_type(&self) -> String {
        self.mime_type.clone()
    }
    pub fn base_url(&self) -> String {
        self.state.repo_url().expect("This has been pre-vetted")
    }

    // pub fn dest(&self) -> PathBuf {
    //     self.state.args.dest()
    // }

    pub fn same_data(&self, other: &Option<Vec<u8>>) -> bool {
        match other {
            None => false,
            Some(other_data) => other_data == &self.data,
        }
    }

    pub fn remove_file(&self) {
        let path = self.file_path();
        if path.exists() && path.is_file() {
            match remove_file(&path) {
                Ok(_) => {}
                Err(e) => {
                    error!("Failed to delete {:?}, error {:?}", path, e);
                }
            }
        }
    }

    pub fn load(&self) -> Option<Vec<u8>> {
        let path = self.file_path();
        if path.exists() && path.is_file() {
            let ret = match File::open(&path) {
                Ok(mut f) => {
                    let mut v = vec![];
                    match f.read_to_end(&mut v) {
                        Ok(_) => Some(v),
                        Err(_e) => None,
                    }
                }
                _ => None,
            };

            return ret;
        }

        None
    }

    pub fn file_path(&self) -> PathBuf {
        let path_str = format!("{}", &self.url[self.base_url().len()..]);
        let path = self.state.crawl_db_dest_dir().join(path_str);
        path
    }
    pub fn save(&self) -> Result<()> {
        let path = self.file_path();
        let dir = path.parent().expect("Get parent");
        create_dir_all(dir)?;
        let mut file = File::create(path)?;
        file.write_all(&self.data)?;
        Ok(())
    }
    /// Take an HTML page and find all the down-links on the page
    pub fn html_to_links(&self) -> Vec<String> {
        let mut ret = vec![];
        if let Ok(string) = String::from_utf8(self.data.clone()) {
            let document = Html::parse_document(&string);
            let gimme_a = Selector::parse("a").expect("Should be able to parse 'a'");
            for a in document.select(&gimme_a) {
                if let Some(href) = a.attr("href") {
                    if href.len() > 1
                        && !href.starts_with(".")
                        && (!href.starts_with("http") || href.starts_with(&self.base_url()))
                        && (href.ends_with("/") || href.ends_with(GOLD_FILE))
                    {
                        let target = if href.starts_with(&self.base_url()) {
                            href.to_string()
                        } else {
                            format!("{}{}", self.url, href)
                        };
                        if target == self.url {
                            println!("url {} page {}", self.url, string);
                            panic!("Yak");
                        }
                        ret.push(target);
                    }
                }
            }
        }

        ret
    }
}
pub const GOLD_FILE: &str = "maven-metadata.xml";
