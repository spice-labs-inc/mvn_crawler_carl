# [Maven Crawler Carl](https://en.wikipedia.org/wiki/Dungeon_Crawler_Carl)

Crawls maven repsitories, first by building an index of the packages
in a particular repository, then, in a separate command, the
crawl and the artifact DB are reified: the abstract crawl DB is
materialized into all the package version artifacts. 

## Data stores

There are two data stores in Maven Crawler Carl:

* Crawl DB -- the versioned (by date) crawl of a maven repository.
  The crawl db should be stored in a separate directory hierarchy
  from the artifact DB. Each crawl has a separate directory
  prefixed by the data and time of the crawl. For example:
  `2025_04_13_14_43_04_crawl_db`
* Artifact DB -- the database of the artifacts downloaded. There
  should only by one Artifact DB per maven repository

## Building

[Install Rust](https://rustup.rs/). Then `cargo build --release`

The resulting binary can be found in `target/release/mvn_crawl`

## Doing a crawl

A crawl (finding all the `maven-metadata.xml` files in a maven repo)
is a relatively fast operation. It takes about 15 minutes to crawl all
of Maven Central and get the metadata files.

To run a crawl: `mvn_crawl --crawl-db ~/data/maven/crawl_db/central --repo https://repo1.maven.org/maven2/ --mirror https://maven-central-eu.storage-download.googleapis.com/maven2/`

Where:

* `--crawl-db` is the directory to put the crawl data in (the list of packages)
* `--repo` is the Maven repository to crawl
* `--mirror` is the _optional_ mirror to load artifacts from. For example,
   Maven central is mirrored by Google. The HTML pages in Maven central are
  not mirrored, but the artifacts are mirrored. To reduce pressure on Maven
  central (or other maven repos that have mirrors), use the mirror for
  loading all `.xml`, `.jar`, `.pom`, etc. files. 

The resulting crawl will be in the `--crawl_db` in a subdirectory with
the crawl date.

## Plan

To see what artifacts will be downloaded in the reify phase, you
can run a plan:

`mvn_crawl --crawl-db ~/data/maven/crawl_db/central --repo https://repo1.maven.org/maven2/ --mirror https://maven-central-eu.storage-download.googleapis.com/maven2/ --artifact-db data/maven/artifact_db/central --plan`

This will print to the console all the artifacts that will be downloaded and all of
the `maven-metadata.xml` file (thus all the packages) that will be updated.

Note that the code will find the most recent crawl in the Crawl DB.

The algorithm finds all the packages in the Crawl DB and compares the versions
of those packages with the versions (as defined by the contents of the 
`maven-metadata.xml` file) in the Artifact DB.

## Reifying

To turn a crawl into the artifacts represented by a crawl, reify the crawl
into the artifact DB:

`mvn_crawl --crawl-db ~/data/maven/crawl_db/central --repo https://repo1.maven.org/maven2/ --mirror https://maven-central-eu.storage-download.googleapis.com/maven2/ --artifact-db data/maven/artifact_db/central --reify-artifact-db`

This will execute the above plan. Note that if this process is interrupted,
it can be resumed.

Why?

The `maven-metadata.xml` file is the last item to be copied into the Artifact DB.
This is kinda a "transaction commit." So, if the reification process is
iterrupted, when it is restarted, only the packages that have differing
`maven-metadata.xml` files are processed.

## Other parameters

`--max-threads` -- the maximum number of threads to use. Default 200. Not
  really a reason to change.
  
