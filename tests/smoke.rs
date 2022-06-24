use std::fs::File;
use std::io;
use std::path::Path;
use std::process::Command;
use core::result::Result::Ok;

use anyhow::*;
use log::*;
use memmap::Mmap;
use rayon::prelude::*;

use piz::read::*;
use piz::result::ZipError;

#[test]
fn smoke() -> Result<()> {
    let _ = env_logger::builder().is_test(true).try_init();

    let inputs = [
        "tests/inputs/hello.zip",
        "tests/inputs/hello-prefixed.zip",
        "tests/inputs/zip64.zip",
    ];

    if inputs.iter().any(|i| !Path::new(i).exists()) {
        Command::new("tests/create-inputs.sh")
            .status()
            .expect("Couldn't set up input files");
    }

    for input in &inputs {
        read_zip(input)?;
    }

    Ok(())
}

fn read_zip(zip_path: &str) -> Result<()> {
    info!("Memory mapping {:#?}", zip_path);
    let zip_file = File::open(zip_path).context("Couldn't open zip file")?;
    let mapping = unsafe { Mmap::map(&zip_file).context("Couldn't mmap zip file")? };

    let archive = ZipArchive::with_prepended_data(&mapping)
        .context("Couldn't load archive")?
        .0;

    // Make sure we can treeify the entries (i.e., they form a valid directory)
    let tree = as_tree(archive.entries())?;

    match zip_path {
        "tests/inputs/hello.zip" | "tests/inputs/hello-prefixed.zip" => {
            tree.lookup("hello/hi.txt")?;
            tree.lookup("hello/rip.txt")?;
            tree.lookup("hello/sr71.txt")?;

            let no_such_file = Path::new("no/such/file");
            match tree.lookup(no_such_file) {
                Err(ZipError::NoSuchFile(p)) => {
                    assert_eq!(no_such_file, p);
                }
                Err(other) => panic!("Got incorrect error from path with no file: {:?}", other),
                Ok(_) => panic!("Got a file back from a path with no file"),
            };
            let no_such_file = Path::new("top-level-no-such-file");
            match tree.lookup(no_such_file) {
                Err(ZipError::NoSuchFile(p)) => {
                    assert_eq!(no_such_file, p);
                }
                Err(other) => panic!("Got incorrect error from path with no file: {:?}", other),
                Ok(_) => panic!("Got a file back from a path with no file"),
            };

            let invalid_path = Path::new("../nope");
            match tree.lookup(invalid_path) {
                Err(ZipError::InvalidPath(_)) => { /* Cool. */ }
                Err(other) => panic!("Got incorrect error from invalid path: {:?}", other),
                Ok(_) => panic!("Got a file back from invalid path"),
            };
        }
        "tests/inputs/zip64.zip" => {
            tree.lookup("zip64/zero100")?;
            tree.lookup("zip64/zero4400")?;
            tree.lookup("zip64/zero5000")?;
        }
        wut => unreachable!("{}", wut),
    };

    // Try reading out each file in the archive.
    // (When the reader gets dropped, the file's CRC32 will be checked
    // against the one stored in the archive.)
    tree.files()
        .map(|e| archive.read(e))
        .par_bridge()
        .try_for_each::<_, Result<()>>(|reader| {
            let mut sink = io::sink();
            io::copy(&mut reader?, &mut sink)?;
            Ok(())
        })?;
    Ok(())
}
