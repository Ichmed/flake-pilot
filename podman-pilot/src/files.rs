use std::{fs, path::Path};

use crate::{
    defaults,
    sudo::{CommandError, OutputResultExtension},
    user::User,
};

pub fn chmod(filename: &str, mode: &str, user: User) -> Result<(), CommandError> {
    /*!
    Chmod filename via sudo
    !*/
    user.sudo("chmod")
        .arg(mode)
        .arg(filename)
        .output()
        .report("chmod")?;
    Ok(())
}

pub fn mkdir(dirname: &str, mode: &str, user: User) -> Result<(), CommandError> {
    /*!
    Make directory via sudo
    !*/

    user.sudo("mkdir")
        .arg("-p")
        .arg("-m")
        .arg(mode)
        .arg(dirname)
        .output()
        .report("mkdir")?;
    Ok(())
}

pub fn gc(user: User) -> std::io::Result<()> {
    /*!
    Garbage collect CID files for which no container exists anymore
    !*/

    let paths: Vec<_> = fs::read_dir(defaults::CONTAINER_CID_DIR)?.collect();

    if paths.len() < defaults::GC_THRESHOLD {
        Ok(())
    } else {
        // Collect in two steps to prevent short circuiting so as many files as possible are cleaned up
        let results: Vec<_> = paths
            .into_iter()
            .map(|entry| gc_cid_file(entry?.path(), user).map(|_| ()))
            .collect();

        results.into_iter().collect()
    }
}

pub fn gc_cid_file<P>(container_cid_file: P, user: User) -> std::io::Result<bool>
where
    P: AsRef<Path>,
{
    /*!
    Check if container exists according to the specified
    container_cid_file. Garbage cleanup the container_cid_file
    if no longer present. Return a true value if the container
    exists, in any other case return false.
    !*/

    let cid = fs::read_to_string(&container_cid_file)?;
    let is_running = user
        .sudo("podman")
        .arg("container")
        .arg("exists")
        .arg(&cid)
        .status()?
        .success();

    if !is_running {
        fs::remove_file(container_cid_file)?;
    }
    Ok(is_running)
}
