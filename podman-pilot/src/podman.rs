//
// Copyright (c) 2022 Elektrobit Automotive GmbH
//
// This file is part of flake-pilot
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.
//
use crate::app_path::program_config_file;
use crate::config::{config, RuntimeSection};
use crate::defaults::debug;
use spinoff::{spinners, Color, Spinner};
use std::ffi::OsStr;
use std::fs;
use std::fs::File;
use std::io::Seek;
use std::io::SeekFrom;
use std::io::{Read, Write};
use std::ops::{Deref, DerefMut};
use std::path::Path;
use std::process::{exit, ExitStatus, Output};
use std::process::{Command, Stdio};
use std::{env, result};
use tempfile::tempfile;
use yaml_rust::Yaml;

use crate::defaults;

struct Sudo(Command);

impl Sudo {
    fn new() -> Self {
        Self(Command::new("sudo"))
    }

    fn void() -> Self {
        let mut n = Self::new();
        n.stderr(Stdio::null()).stdout(Stdio::null());
        n
    }

    fn user(&mut self, user: Option<&str>) -> &mut Self {
        if let Some(user) = user {
            self.0.arg("--user").arg(user);
        }
        self
    }

    fn flag(&mut self, condition: bool, arg: &str) -> &mut Self {
        if condition {
            self.0.arg(arg);
        }
        self
    }

    fn arg<S>(&mut self, arg: S) -> &mut Self
    where
        S: AsRef<OsStr>,
    {
        self.0.arg(arg);
        self
    }

    fn debug_print(&mut self) -> &mut Self {
        debug(&format!("{:?}", self.0.get_args()));
        self
    }
}

impl std::ops::Deref for Sudo {
    type Target = Command;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Sudo {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

pub struct Podman {
    pub program_name: String,
    pub cid: String,
    pub container_cid_file: String,
    pub user: Option<String>,
}

impl Podman {
    pub fn start(&self) -> Result<ExitStatus, FlakeError> {
        /*!
        Start container with the given container ID

        podman-pilot exits with the return code from podman after this function
        !*/
        let RuntimeSection {
            resume,
            attach,
            ..
        } = config()
            .container
            .runtime
            .as_ref()
            .cloned()
            .unwrap_or_default();


        let status_code = if self.is_running()? {
            if attach {
                // 1. Attach to running container
                self.attach_instance()?
            } else {
                // 2. Execute app in running container
                self.exec_instance()?
            }
        } else if resume {
            // 3. Startup resume type container and execute app
            let status_code = self.start_instance()?;
            if status_code.success() {
                self.exec_instance()?
            } else {
                status_code
            }
        } else {
            // 4. Startup container
            self.start_instance()?
        };

        Ok(status_code)
    }

    pub fn is_running(&self) -> Result<bool, PodmanError> {
        /*!
    Check if container with specified cid is running
    !*/
        let output = self.command()
            .arg("ps")
            .arg("--format")
            .arg("{{.ID}}")
            .debug_print()
            .output()
            .map_err(PodmanError::ExectuionFailed)?;

        Ok(String::from_utf8_lossy(&output.stdout)
            .lines()
            .any(|line| self.cid.starts_with(line)))

    }

    fn command(&self) -> Sudo {
        let mut sudo = Sudo::new();
        sudo.user(self.user.as_deref()).arg("podman");
        sudo
    }

    fn void(&self) -> Sudo {
        let mut sudo = self.command();
        sudo.stderr(Stdio::null());
        sudo.stdout(Stdio::null());
        sudo
    }

    pub fn start_instance(&self) -> Result<ExitStatus, FlakeError> {
        /*!
        Call container ID based podman commands
        !*/

        let mut command = self.command();
        command.arg("start");
        if config().resume() {
            command.arg("--attach");
        } else {
            command.stdout(Stdio::null());
        }

        Ok(command
            .arg(&self.cid)
            .output()
            .map_err(PodmanError::ExectuionFailed)?
            .status)
    }

    pub fn attach_instance(&self) -> Result<ExitStatus, FlakeError> {
        /*!
        Call container ID based podman commands
        !*/
        Ok(self
            .void()
            .arg("attach")
            .arg(&self.cid)
            .output()
            .map_err(PodmanError::ExectuionFailed)?
            .status)
    }

    pub fn create_instance(&self) -> Result<ExitStatus, FlakeError> {
        /*!
        Call container ID based podman commands
        !*/
        Ok(self
            .void()
            .arg("create")
            .arg(&self.cid)
            .output()
            .map_err(PodmanError::ExectuionFailed)?
            .status)
    }

    pub fn rm_instance(&self) -> Result<ExitStatus, FlakeError> {
        /*!
        Call container ID based podman commands
        !*/
        Ok(self
            .void()
            .arg("rm")
            .arg(&self.cid)
            .output()
            .map_err(PodmanError::ExectuionFailed)?
            .status)
    }

    pub fn exec_instance(&self) -> Result<ExitStatus, FlakeError> {
        /*!
        Call container ID based podman commands
        !*/
        Ok(self
            .command()
            .arg("exec")
            .arg("--interactive")
            .arg("--tty")
            .arg(&self.cid)
            .arg(
                config()
                    .container
                    .target_app_path
                    .unwrap_or(&self.program_name),
            )
            .args(env::args().skip(1).filter(|arg| !arg.starts_with('@')))
            .output()
            .map_err(PodmanError::ExectuionFailed)?
            .status)
    }
}

pub fn create(program_name: &String) -> Podman {
    /*!
    Create container for later execution of program_name.
    The container name and all other settings to run the program
    inside of the container are taken from the config file(s)

    CONTAINER_FLAKE_DIR/
       ├── program_name.d
       │   └── other.yaml
       └── program_name.yaml

    All commandline options will be passed to the program_name
    called in the container. An example program config file
    looks like the following:

    container:
      name: name
      target_app_path: path/to/program/in/container
      host_app_path: path/to/program/on/host

      # Optional base container to use with a delta 'container: name'
      # If specified the given 'container: name' is expected to be
      # an overlay for the specified base_container. podman-pilot
      # combines the 'container: name' with the base_container into
      # one overlay and starts the result as a container instance
      #
      # Default: not_specified
      base_container: name

      # Optional additional container layers on top of the
      # specified base container
      layers:
        - name_A
        - name_B

      runtime:
        # Run the container engine as a user other than the
        # default target user root. The user may be either
        # a user name or a numeric user-ID (UID) prefixed
        # with the ‘#’ character (e.g. #0 for UID 0). The call
        # of the container engine is performed by sudo.
        # The behavior of sudo can be controlled via the
        # file /etc/sudoers
        runas: root

        # Resume the container from previous execution.
        # If the container is still running, the app will be
        # executed inside of this container instance.
        #
        # Default: false
        resume: true|false

        # Attach to the container if still running, rather than
        # executing the app again. Only makes sense for interactive
        # sessions like a shell running as app in the container.
        #
        # Default: false
        attach: true|false

        podman:
          - --storage-opt size=10G
          - --rm
          - -ti

      Calling this method returns a vector including the
      container ID and and the name of the container ID
      file.

    include:
      tar:
        - tar-archive-file-name-to-include
    !*/
    let args: Vec<String> = env::args().collect();
    let mut layers: Vec<String> = Vec::new();

    // setup container ID file name
    let mut container_cid_file = format!("{}/{}", defaults::CONTAINER_CID_DIR, program_name);
    for arg in &args[1..] {
        if arg.starts_with('@') {
            // The special @NAME argument is not passed to the
            // actual call and can be used to run different container
            // instances for the same application
            container_cid_file = format!("{}{}", container_cid_file, arg);
        }
    }

    let RuntimeSection {
        runas,
        resume,
        attach,
        ..
    } = config()
        .container
        .runtime
        .as_ref()
        .cloned()
        .unwrap_or_default();

    container_cid_file = format!("{}.cid", container_cid_file);

    // check for includes
    let container_name = config().container.name;

    // setup base container if specified
    if config().container.base_container.is_some() {
        layers.extend(
            config().container.layers.iter().map(|x| (*x).to_owned()), //.inspect(|layer| debug(&format!("Adding layer: [{layer}]")))
        );
    };

    // setup app command path name to call
    let target_app_path = config().container.target_app_path.unwrap_or(&program_name);

    // setup container operation mode

    let mut app = Sudo::new();
    app.user(runas)
        .arg("podman")
        .arg("create")
        .arg("--cidfile")
        .arg(&container_cid_file);

    // Make sure CID dir exists
    init_cid_dir();

    // Check early return condition in resume mode
    if Path::new(&container_cid_file).exists()
        && gc_cid_file(&container_cid_file, runas)
        && (resume || attach)
    {
        // resume or attach mode is active and container exists
        // report ID value and its ID file name
        let cid = fs::read_to_string(&container_cid_file).expect("Error reading CID: {:?}");
        return Podman {
            program_name: program_name.clone(),
            cid,
            container_cid_file,
            user: runas.map(str::to_owned),
        };
    }

    // Garbage collect occasionally
    gc(runas);

    // Sanity check
    if Path::new(&container_cid_file).exists() {
        // we are about to create a container for which a
        // cid file already exists. podman create will fail with
        // an error but will also create the container which is
        // unwanted. Thus we check this condition here
        error!("Container id in use by another instance, consider @NAME argument");
        exit(1)
    }

    // create the container with configured runtime argumentsS
    if let Some(ref runtime) = config().container.runtime {
        app.args(runtime.podman.iter().flat_map(|x| x.splitn(2, ' ')));
    } else if resume {
        app.arg("-ti");
    } else {
        app.arg("--rm").arg("-ti");
    };

    // setup container name to use
    app.arg(config().container.base_container.unwrap_or(container_name));

    // setup entry point
    if resume {
        // create the container with a sleep entry point
        // to keep it in running state
        //
        // sleep "forever" ... I will be dead by the time this sleep ends ;)
        // keeps the container in running state to accept podman exec for
        // running the app multiple times with different arguments
        app.arg("sleep").arg("4294967295d");
    } else {
        if target_app_path != "/" {
            app.arg(target_app_path);
        }

        app.args(args[1..].iter().filter(|arg| !arg.starts_with('@')));
    }

    // create container
    debug(&format!("{:?}", app.get_args()));
    let spinner = Spinner::new(spinners::Line, "Launching flake...", Color::Yellow);
    match launch_flake(
        program_name,
        &mut app,
        &container_cid_file,
        runas,
        layers,
        container_name,
    ) {
        Ok(result) => {
            spinner.success("Launching flake");
            result
        }
        Err(err) => {
            spinner.fail("Flake launch has failed");
            todo!()
        }
    }
}

#[derive(Debug)]
pub enum FlakeError {
    ProvisionContainerFailed(std::io::Error),
    CreateContainerFailed,
    PodmanExecutionFailed(PodmanError),
    FailedToCreateTempFile(std::io::Error),
}

impl From<PodmanError> for FlakeError {
    fn from(value: PodmanError) -> Self {
        Self::PodmanExecutionFailed(value)
    }
}

fn launch_flake(
    program_name: &str,
    app: &mut Command,
    container_cid_file: &str,
    runas: Option<&str>,
    mut layers: Vec<String>,
    container_name: &str,
) -> Result<Podman, FlakeError> {
    let output = app.output().map_err(PodmanError::ExectuionFailed)?;
    if !output.status.success() {
        return Err(FlakeError::CreateContainerFailed);
    }
    let cid = String::from_utf8_lossy(&output.stdout)
        .trim_end_matches('\n')
        .to_owned();

    let result = Podman {
        program_name: program_name.to_owned(),
        cid: cid.clone(),
        container_cid_file: container_cid_file.to_owned(),
        user: runas.map(str::to_owned),
    };

    let tars = &config().tar;

    if config().is_delta_container() || !tars.is_empty() {
        debug("Mounting instance for provisioning workload");
        let mut provision_ok = true;
        let instance_mount_point = mount_container(&cid, runas, false)?;

        if config().is_delta_container() {
            // Create tmpfile to hold accumulated removed data
            let mut removed_files = tempfile().map_err(FlakeError::FailedToCreateTempFile)?;

            debug("Provisioning delta container...");
            update_removed_files(&instance_mount_point, &removed_files);
            debug(&format!("Adding main app [{container_name}] to layer list"));

            layers.push(container_name.to_owned());

            provision_ok = layers
                .iter()
                .inspect(|layer| debug(&format!("Syncing delta dependencies [{layer}]...")))
                .map(|layer| sync_layer(layer, &instance_mount_point, runas, &mut removed_files))
                .all(|ok| ok.unwrap()); //TODO

            if provision_ok {
                debug("Syncing host dependencies...");
                provision_ok = sync_host(&instance_mount_point, &removed_files, runas)
            }
            umount_container(&cid, runas, false);
        }

        if !tars.is_empty() && provision_ok {
            debug("Syncing includes...");
            provision_ok = sync_includes(&instance_mount_point, runas).unwrap();
            // todo
        }

        if !provision_ok {
            panic!("Failed to provision container")
        }
    }
    return Ok(result);
}

fn sync_layer(
    layer: &str,
    instance_mount_point: &str,
    user: Option<&str>,
    removed_files: &mut File,
) -> Result<bool, FlakeError> {
    let mount_point = mount_container(&layer, user, true)?;
    update_removed_files(&mount_point, removed_files);
    let ok = sync_delta(&mount_point, instance_mount_point, user).unwrap(); //todo
    umount_container(&layer, user, true);
    Ok(ok)
}

#[derive(Debug)]
pub enum PodmanError {
    ExectuionFailed(std::io::Error),
    MountFailed(String),
    // "Failed to execute podman image umount: {:?}"
    // let mut status_code = 255;
    UnmountFailed(std::io::Error),
}

fn mount_container(
    container_name: &str,
    user: Option<&str>,
    as_image: bool,
) -> Result<String, PodmanError> {
    /*!
    Mount container and return mount point
    !*/

    if as_image && !container_image_exists(container_name, user) {
        pull(container_name, user);
    }

    let output = Sudo::new()
        .user(user)
        .arg("podman")
        .flag(as_image, "image")
        .arg("mount")
        .arg(container_name)
        .debug_print()
        .output()
        .map_err(PodmanError::ExectuionFailed)?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout)
            .trim_end_matches('\n')
            .to_owned())
    } else {
        Err(PodmanError::MountFailed(
            String::from_utf8_lossy(&output.stderr)
                .trim_end_matches('\n')
                .to_owned(),
        ))
    }
}

pub fn umount_container(
    mount_point: &str,
    user: Option<&str>,
    as_image: bool,
) -> Result<ExitStatus, PodmanError> {
    /*!
    Umount container image
    !*/
    Sudo::void()
        .user(user)
        .arg("podman")
        .flag(as_image, "image")
        .arg("umount")
        .arg(mount_point)
        .debug_print()
        .status()
        .map_err(PodmanError::UnmountFailed)
}

#[derive(Debug)]
pub struct TarError(std::io::Error);

pub fn sync_includes(target: &String, user: Option<&str>) -> Result<bool, TarError> {
    /*!
    Sync custom include data to target path
    !*/
    let result: Result<Vec<_>, _> = config()
        .tar
        .iter()
        .map(|tar| {
            debug(&format!("Adding tar include: [{tar}]"));
            Sudo::new()
                .user(user)
                .arg("tar")
                .arg("-C")
                .arg(target)
                .arg("-xf")
                .arg(tar)
                .debug_print()
                .output()
                .map_err(TarError)
        })
        .collect();

    Ok(result?
        .iter()
        .map(|output| {
            debug(&String::from_utf8_lossy(&output.stdout));
            debug(&String::from_utf8_lossy(&output.stderr));
            output.status
        })
        .all(|status| status.success()))
}

#[derive(Debug)]
pub struct RSyncError(std::io::Error);

pub fn sync_delta(source: &String, target: &str, user: Option<&str>) -> Result<bool, RSyncError> {
    /*!
    Sync data from source path to target path
    !*/

    let output = Sudo::new()
        .user(user)
        .arg("rsync")
        .arg("-av")
        .arg(&format!("{}/", &source))
        .arg(&format!("{}/", &target))
        .debug_print()
        .output()
        .map_err(RSyncError)?;

    debug(&String::from_utf8_lossy(&output.stdout));
    Ok(output.status.success())
}

pub fn sync_host(target: &String, mut removed_files: &File, user: Option<&str>) -> bool {
    /*!
    Sync files/dirs specified in target/defaults::HOST_DEPENDENCIES
    from the running host to the target path
    !*/
    let mut removed_files_contents = String::new();
    let host_deps = format!("{}/{}", &target, defaults::HOST_DEPENDENCIES);
    removed_files.seek(SeekFrom::Start(0)).unwrap();
    match removed_files.read_to_string(&mut removed_files_contents) {
        Ok(_) => {
            if removed_files_contents.is_empty() {
                debug("There are no host dependencies to resolve");
                return true;
            }
            match File::create(&host_deps) {
                Ok(mut removed) => match removed.write_all(removed_files_contents.as_bytes()) {
                    Ok(_) => {}
                    Err(error) => {
                        panic!("Write failed {}: {:?}", host_deps, error);
                    }
                },
                Err(error) => {
                    panic!("Error creating {}: {:?}", host_deps, error);
                }
            }
        }
        Err(error) => {
            panic!("Error reading from file descriptor: {:?}", error);
        }
    }
    let mut call = Sudo::new();
    call.user(user)
        .arg("rsync")
        .arg("-av")
        .arg("--ignore-missing-args")
        .arg("--files-from")
        .arg(&host_deps)
        .arg("/")
        .arg(&format!("{}/", &target));
    let status_code;
    debug(&format!("{:?}", call.get_args()));
    match call.output() {
        Ok(output) => {
            debug(&String::from_utf8_lossy(&output.stdout));
            status_code = output.status.code().unwrap();
        }
        Err(error) => {
            panic!("Failed to execute rsync: {:?}", error)
        }
    }
    if status_code == 0 {
        return true;
    }
    false
}

pub fn init_cid_dir() {
    if !Path::new(defaults::CONTAINER_CID_DIR).is_dir() {
        if !chmod(defaults::CONTAINER_DIR, "755", "root") {
            panic!(
                "Failed to set permissions 755 on {}",
                defaults::CONTAINER_DIR
            );
        }
        if !mkdir(defaults::CONTAINER_CID_DIR, "777", "root") {
            panic!("Failed to create CID dir: {}", defaults::CONTAINER_CID_DIR);
        }
    }
}

pub fn container_running(cid: &str, user: Option<&str>) -> Result<bool, PodmanError> {
    /*!
    Check if container with specified cid is running
    !*/
    let output = Sudo::new()
        .user(user)
        .arg("podman")
        .arg("ps")
        .arg("--format")
        .arg("{{.ID}}")
        .debug_print()
        .output()
        .map_err(PodmanError::ExectuionFailed)?;

    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .any(|line| cid.starts_with(line)))
}

pub fn container_image_exists(name: &str, user: Option<&str>) -> bool {
    /*!
    Check if container image is present in local registry
    !*/
    let mut exists_status = false;
    let mut exists = Command::new("sudo");
    if let Some(user) = user {
        exists.arg("--user").arg(user);
    }
    exists.arg("podman").arg("image").arg("exists").arg(name);
    debug(&format!("{:?}", exists.get_args()));
    match exists.status() {
        Ok(status) => {
            if status.code().unwrap() == 0 {
                exists_status = true
            }
        }
        Err(error) => {
            panic!("Failed to execute podman image exists: {:?}", error)
        }
    }
    exists_status
}

pub fn pull(uri: &str, user: Option<&str>) {
    /*!
    Call podman pull and prune with the provided uri
    !*/
    let mut pull = Command::new("sudo");
    if let Some(user) = user {
        pull.arg("--user").arg(user);
    }
    pull.arg("podman").arg("pull").arg(uri);
    debug(&format!("{:?}", pull.get_args()));
    match pull.output() {
        Ok(output) => {
            if !output.status.success() {
                panic!(
                    "Failed to fetch container: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            } else {
                let mut prune = Command::new("sudo");
                if let Some(user) = user {
                    prune.arg("--user").arg(user);
                }
                prune.arg("podman").arg("image").arg("prune").arg("--force");
                match prune.status() {
                    Ok(status) => debug(&format!("{:?}", status)),
                    Err(error) => debug(&format!("{:?}", error)),
                }
            }
        }
        Err(error) => {
            panic!("Failed to call podman pull: {}", error)
        }
    }
}

pub fn update_removed_files(target: &String, mut accumulated_file: &File) {
    /*!
    Take the contents of the given removed_file and append it
    to the accumulated_file
    !*/
    let host_deps = format!("{}/{}", &target, defaults::HOST_DEPENDENCIES);
    debug(&format!("Looking up host deps from {}", host_deps));
    if Path::new(&host_deps).exists() {
        match fs::read_to_string(&host_deps) {
            Ok(data) => {
                debug("Adding host deps...");
                debug(&String::from_utf8_lossy(data.as_bytes()));
                match accumulated_file.write_all(data.as_bytes()) {
                    Ok(_) => {}
                    Err(error) => {
                        panic!("Writing to descriptor failed: {:?}", error);
                    }
                }
            }
            Err(error) => {
                // host_deps file exists but could not be read
                panic!("Error reading {}: {:?}", host_deps, error);
            }
        }
    }
}

pub fn gc_cid_file(container_cid_file: &String, user: Option<&str>) -> bool {
    /*!
    Check if container exists according to the specified
    container_cid_file. Garbage cleanup the container_cid_file
    if no longer present. Return a true value if the container
    exists, in any other case return false.
    !*/
    let mut cid_status = false;
    match fs::read_to_string(container_cid_file) {
        Ok(cid) => {
            let mut exists = Command::new("sudo");
            if let Some(user) = user {
                exists.arg("--user").arg(user);
            }
            exists
                .arg("podman")
                .arg("container")
                .arg("exists")
                .arg(&cid);
            match exists.status() {
                Ok(status) => {
                    if status.code().unwrap() != 0 {
                        match fs::remove_file(container_cid_file) {
                            Ok(_) => {}
                            Err(error) => {
                                error!("Failed to remove CID: {:?}", error)
                            }
                        }
                    } else {
                        cid_status = true
                    }
                }
                Err(error) => {
                    error!("Failed to execute podman container exists: {:?}", error)
                }
            }
        }
        Err(error) => {
            error!("Error reading CID: {:?}", error);
        }
    }
    cid_status
}

pub fn chmod(filename: &str, mode: &str, user: &str) -> bool {
    /*!
    Chmod filename via sudo
    !*/
    let mut call = Command::new("sudo");
    if !user.is_empty() {
        call.arg("--user").arg(user);
    }
    call.arg("chmod").arg(mode).arg(filename);
    match call.status() {
        Ok(_) => {}
        Err(error) => {
            error!("Failed to chmod: {}: {:?}", filename, error);
            return false;
        }
    }
    true
}

pub fn mkdir(dirname: &str, mode: &str, user: &str) -> bool {
    /*!
    Make directory via sudo
    !*/
    let mut call = Command::new("sudo");
    if !user.is_empty() {
        call.arg("--user").arg(user);
    }
    call.arg("mkdir").arg("-p").arg("-m").arg(mode).arg(dirname);
    match call.status() {
        Ok(_) => {}
        Err(error) => {
            error!("Failed to mkdir: {}: {:?}", dirname, error);
            return false;
        }
    }
    true
}

pub fn gc(user: Option<&str>) {
    /*!
    Garbage collect CID files for which no container exists anymore
    !*/
    let mut cid_file_names: Vec<String> = Vec::new();
    let mut cid_file_count: i32 = 0;
    let paths = fs::read_dir(defaults::CONTAINER_CID_DIR).unwrap();
    for path in paths {
        cid_file_names.push(format!("{}", path.unwrap().path().display()));
        cid_file_count += 1;
    }
    if cid_file_count <= defaults::GC_THRESHOLD {
        return;
    }
    for container_cid_file in cid_file_names {
        gc_cid_file(&container_cid_file, user);
    }
}
