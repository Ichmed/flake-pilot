use std::{fs, path::Path, process::Stdio};

use crate::{command::CommandExtTrait, error::FlakeError, user::User};

fn create_dir(p: impl AsRef<Path>) -> Result<(), std::io::Error> {
    let p = p.as_ref();
    if !p.exists() {
        fs::create_dir_all(p)?
    }
    Ok(())
}

pub fn mount(
    src: impl AsRef<Path>,
    dst: impl AsRef<Path>,
    user: User,
) -> Result<(), FlakeError> {
    create_dir(&src)?;
    create_dir(&dst)?;

    let mut mount_image = user.run("mount");
    mount_image.arg(src.as_ref()).arg(dst.as_ref());
    mount_image.perform()?;
    Ok(())
}

pub fn mount_overlay(
    lowerdir: impl AsRef<Path>,
    upperdir: impl AsRef<Path>,
    workdir: impl AsRef<Path>,
    dst: impl AsRef<Path>,
    user: User,
) -> Result<(), FlakeError> {
    create_dir(&lowerdir)?;
    create_dir(&upperdir)?;
    create_dir(&workdir)?;
    create_dir(&dst)?;

    let lowerdir = format!("lowerdir={}", lowerdir.as_ref().to_string_lossy());
    let upperdir = format!("upperdir={}", upperdir.as_ref().to_string_lossy());
    let workdir = format!("workdir={}", workdir.as_ref().to_string_lossy());

    let mut mount_overlay = user.run("mount");
    mount_overlay
        .arg("-t")
        .arg("overlay")
        .arg("overlayfs")
        .arg("-o")
        .arg(format!("{lowerdir},{upperdir},{workdir}"))
        .arg(dst.as_ref());
    mount_overlay.perform()?;
    Ok(())
}

pub fn unmount(target: impl AsRef<Path>, user: User) -> Result<(), FlakeError> {
    let mut umount = user.run("umount");
    umount.stderr(Stdio::null());
    umount.stdout(Stdio::null());
    umount.arg(target.as_ref());
    umount.perform()?;
    Ok(())
}
