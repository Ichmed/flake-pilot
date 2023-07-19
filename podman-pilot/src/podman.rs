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

use crate::config::{config, RuntimeSection};
use crate::defaults::debug;
use crate::files::{chmod, gc, gc_cid_file, mkdir};
use crate::sudo::{
    CommandError, OutputExtension, OutputResultExtension, ProcessError, ProcessResultExtension,
    Sudo,
};
use crate::user::User;
use spinoff::{spinners, Color, Spinner};


use std::fmt::Display;
use std::fs;
use std::fs::File;

use std::io::{Read, Write};

use std::env;
use std::path::Path;
use std::process::Stdio;
use std::process::{ExitCode, Termination};
use tempfile::tempfile;

use crate::defaults;

pub struct Podman<'a> {
    pub program_name: String,
    pub cid: String,
    pub container_cid_file: String,
    pub user: User<'a>,
}

impl<'a> Podman<'a> {
    pub fn start(&self) -> Result<(), FlakeError> {
        /*!
        Start container with the given container ID

        podman-pilot exits with the return code from podman after this function
        !*/
        let RuntimeSection { resume, attach, .. } = config()
            .container
            .runtime
            .as_ref()
            .cloned()
            .unwrap_or_default();

        if self.is_running()? {
            if attach {
                // 1. Attach to running container
                self.attach_instance()?
            } else {
                // 2. Execute app in running container
                self.exec_instance()?
            }
        } else if resume {
            // 3. Startup resume type container and execute app
            self.start_instance()?;
            self.exec_instance()?
        } else {
            // 4. Startup container
            self.start_instance()?
        }

        Ok(())
    }

    pub fn is_running(&self) -> Result<bool, CommandError> {
        /*!
        Check if container with specified cid is running
        !*/
        let output = self
            .command()
            .arg("ps")
            .arg("--format")
            .arg("{{.ID}}")
            .debug_print()
            .output()
            .report("podman")?;

        Ok(output
            .lossy_stdout()
            .lines()
            .any(|line| self.cid.starts_with(line)))
    }

    fn command(&self) -> Sudo {
        self.user.sudo("podman")
    }

    fn void(&self) -> Sudo {
        let mut sudo = self.command();
        sudo.stderr(Stdio::null());
        sudo.stdout(Stdio::null());
        sudo
    }

    pub fn start_instance(&self) -> Result<(), FlakeError> {
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

        command.arg(&self.cid).output().report("podman")?;
        Ok(())
    }

    pub fn attach_instance(&self) -> Result<(), CommandError> {
        /*!
        Call container ID based podman commands
        !*/
        self.void()
            .arg("attach")
            .arg(&self.cid)
            .output()
            .report("podman")?;

        Ok(())
    }

    pub fn create_instance(&self) -> Result<(), FlakeError> {
        /*!
        Call container ID based podman commands
        !*/
        self.void()
            .arg("create")
            .arg(&self.cid)
            .output()
            .report("podman")?;
        Ok(())
    }

    pub fn rm_instance(&self) -> Result<(), FlakeError> {
        /*!
        Call container ID based podman commands
        !*/
        self.void()
            .arg("rm")
            .arg(&self.cid)
            .output()
            .report("podman")?;
        Ok(())
    }

    pub fn exec_instance(&self) -> Result<(), FlakeError> {
        /*!
        Call container ID based podman commands
        !*/
        self.command()
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
            .report("podman")?;
        Ok(())
    }
}

pub fn create(program_name: &String) -> Result<Podman, FlakeError> {
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

    let (args, mut name): (Vec<String>, Vec<String>) =
        env::args().skip(1).partition(|arg| arg.starts_with('@'));
    let name = name.pop().unwrap_or_default();

    if !name.is_empty() {
        // TODO: Error or Warning
    }

    // setup container ID file name

    let cid_dir = defaults::CONTAINER_CID_DIR;
    // The special @NAME argument is not passed to the
    // actual call and can be used to run different container
    // instances for the same application

    let container_cid_file = format!("{cid_dir}/{program_name}{name}.cid");

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

    // check for includes
    let container_name = config().container.name;

    // setup app command path name to call

    // setup container operation mode

    let mut app = runas.sudo("podman");
    app.arg("create").arg("--cidfile").arg(&container_cid_file);

    // Make sure CID dir exists
    init_cid_dir()?;

    // Check early return condition in resume mode
    if Path::new(&container_cid_file).exists()
        && gc_cid_file(&container_cid_file, runas).map_err(FlakeError::GarbageCollectionFailed)?
        && (resume || attach)
    {
        // resume or attach mode is active and container exists
        // report ID value and its ID file name
        let cid =
            fs::read_to_string(&container_cid_file).map_err(FlakeError::UnableToReadCidFile)?;
        return Ok(Podman {
            program_name: program_name.clone(),
            cid,
            container_cid_file,
            user: runas,
        });
    }

    // Garbage collect occasionally
    // ignore errors
    let _ = gc(runas);

    // Sanity check
    if Path::new(&container_cid_file).exists() {
        // we are about to create a container for which a
        // cid file already exists. podman create will fail with
        // an error but will also create the container which is
        // unwanted. Thus we check this condition here
        return Err(FlakeError::AlreadyRunning);
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
        let target_app_path = config().container.target_app_path.unwrap_or(program_name);
        app.flag(target_app_path != "/", target_app_path);
        app.args(args[1..].iter().filter(|arg| !arg.starts_with('@')));
    }

    // create container
    debug(&format!("{:?}", app.get_args()));
    let spinner = Spinner::new(spinners::Line, "Launching flake...", Color::Yellow);
    match launch_flake(
        program_name,
        app,
        &container_cid_file,
        runas,
        container_name,
    ) {
        Ok(result) => {
            spinner.success("Launching flake");
            Ok(result)
        }
        Err(err) => {
            spinner.fail("Flake launch has failed");
            Err(err)
        }
    }
}

#[derive(Debug)]
pub enum FlakeError {
    GarbageCollectionFailed(std::io::Error),
    UnableToReadCidFile(std::io::Error),
    ProvisionContainerFailed,
    CreateContainerFailed,
    FailedToCreateTempFile(std::io::Error),
    AlreadyRunning,
    FailedToUpdateRemovedFile(std::io::Error),
    CommandError(CommandError),
}

impl Display for FlakeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyRunning => f.write_str("Container id in use by another instance, consider @NAME argument"),
            _ => f.write_str(&format!("{:?}", self))
        }   
    }
}


impl Termination for FlakeError {
    /// If the error is a Command error try to return the underlying ExitCode
    ///
    /// Otherwise return ExitCode::FAILURE
    fn report(self) -> std::process::ExitCode {
        match self {
            FlakeError::CommandError(CommandError {
                error: ProcessError::Fail(result, _),
                ..
            }) => match result.code() {
                Some(int) => (int as u8).into(),
                None => ExitCode::FAILURE,
            },
            _ => ExitCode::FAILURE,
        }
    }
}

impl From<CommandError> for FlakeError {
    fn from(value: CommandError) -> Self {
        Self::CommandError(value)
    }
}

fn launch_flake<'a>(
    program_name: &str,
    mut app: Sudo,
    container_cid_file: &str,
    user: User<'a>,
    container_name: &str,
) -> Result<Podman<'a>, FlakeError> {
    let output = app.output().report("podman")?;

    if !output.status.success() {
        return Err(FlakeError::CreateContainerFailed);
    }

    let cid = output.lossy_stdout().trim_end_matches('\n').to_owned();

    let has_tars = !config().tar.is_empty();

    if config().is_delta_container() || has_tars {
        debug("Mounting instance for provisioning workload");
        let instance_mount_point = mount_container(&cid, user, false)?;

        if config().is_delta_container() {
            // Create tmpfile to hold accumulated removed data
            let mut removed_files = tempfile().map_err(FlakeError::FailedToCreateTempFile)?;

            debug("Provisioning delta container...");
            update_removed_files(&instance_mount_point, &removed_files)?;
            debug(&format!("Adding main app [{container_name}] to layer list"));

            config()
                .container
                .layers
                .iter()
                .inspect(|layer| debug(&format!("Adding layer: [{layer}]")))
                .chain(Some(&container_name))
                .inspect(|layer| debug(&format!("Syncing delta dependencies [{layer}]...")))
                .try_for_each(|layer| sync_layer(layer, &instance_mount_point, user, &mut removed_files))?;

            debug("Syncing host dependencies...");
            let result = sync_host(&instance_mount_point, &removed_files, user);
            umount_container(&cid, user, false)?;
            result?;
        }

        if has_tars {
            debug("Syncing includes...");
            sync_includes(&instance_mount_point, user)?;
        }
    }
    Ok(Podman {
        program_name: program_name.to_owned(),
        cid,
        container_cid_file: container_cid_file.to_owned(),
        user,
    })
}

fn sync_layer(
    layer: &str,
    instance_mount_point: &str,
    user: User,
    removed_files: &mut File,
) -> Result<(), FlakeError> {
    let mount_point = mount_container(layer, user, true)?;
    update_removed_files(&mount_point, removed_files)?;
    let result = sync_delta(&mount_point, instance_mount_point, user);
    umount_container(layer, user, true)?;
    Ok(result?)
}

fn mount_container(
    container_name: &str,
    user: User,
    as_image: bool,
) -> Result<String, CommandError> {
    /*!
    Mount container and return mount point
    !*/

    if as_image && !container_image_exists(container_name, user)? {
        pull(container_name, user)?;
    }

    let output = user
        .sudo("podman")
        .flag(as_image, "image")
        .arg("mount")
        .arg(container_name)
        .debug_print()
        .output()
        .report("podman")?;

    Ok(output.lossy_stdout().trim_end_matches('\n').to_owned())
}

pub fn umount_container(mount_point: &str, user: User, as_image: bool) -> Result<(), CommandError> {
    /*!
    Umount container image
    !*/
    user.sudo("podman")
        .void()
        .flag(as_image, "image")
        .arg("umount")
        .arg(mount_point)
        .debug_print()
        .output()
        .report("podman")?;
    Ok(())
}

pub fn sync_includes(target: &String, user: User) -> Result<(), CommandError> {
    /*!
    Sync custom include data to target path
    !*/
    config()
        .tar
        .iter()
        .map(|tar| {
            debug(&format!("Adding tar include: [{tar}]"));
            user.sudo("tar")
                .arg("-C")
                .arg(target)
                .arg("-xf")
                .arg(tar)
                .debug_print()
                .output()
                .debug_print()
                .report("tar")
        })
        .try_for_each(|x| x.map(|_| ()))
}

pub fn sync_delta(source: &String, target: &str, user: User) -> Result<(), CommandError> {
    /*!
    Sync data from source path to target path
    !*/

    user.sudo("rsync")
        .arg("-av")
        .arg(&format!("{}/", &source))
        .arg(&format!("{}/", &target))
        .debug_print()
        .output()
        .debug_print()
        .report("rsync")?;
    Ok(())
}

pub fn sync_host(
    target: &String,
    mut removed_files: &File,
    user: User,
) -> Result<(), CommandError> {
    /*!
    Sync files/dirs specified in target/defaults::HOST_DEPENDENCIES
    from the running host to the target path
    !*/
    let host_deps = format!("{}/{}", &target, defaults::HOST_DEPENDENCIES);

    let content = {
        let mut content = String::new();
        removed_files
            .read_to_string(&mut content)
            .map_err(ProcessError::IO)
            .from("rsync")?;
        content
    };

    if content.is_empty() {
        return Ok(());
    }
    File::create(&host_deps)
        .map_err(ProcessError::IO)
        .from("rsync")?
        .write_all(content.as_bytes())
        .map_err(ProcessError::IO)
        .from("rsync")?;

    user.sudo("rsync")
        .arg("-av")
        .arg("--ignore-missing-args")
        .arg("--files-from")
        .arg(&host_deps)
        .arg("/")
        .arg(&format!("{}/", &target))
        .debug_print()
        .output()
        .debug_print()
        .report("rsync")?;
    Ok(())
}

pub fn init_cid_dir() -> Result<(), CommandError> {
    if !Path::new(defaults::CONTAINER_CID_DIR).is_dir() {
        chmod(defaults::CONTAINER_DIR, "755", User::root())?;
        mkdir(defaults::CONTAINER_CID_DIR, "777", User::root())?;
    }
    Ok(())
}

pub fn container_running(cid: &str, user: User) -> Result<bool, CommandError> {
    /*!
    Check if container with specified cid is running
    !*/
    let output = user
        .sudo("podman")
        .arg("ps")
        .arg("--format")
        .arg("{{.ID}}")
        .debug_print()
        .output()
        .report("podman")?;

    Ok(output
        .lossy_stdout()
        .lines()
        .any(|line| cid.starts_with(line)))
}

pub fn container_image_exists(name: &str, user: User) -> Result<bool, CommandError> {
    /*!
    Check if container image is present in local registry
    !*/

    Ok(user
        .sudo("podman")
        .arg("image")
        .arg("exists")
        .arg(name)
        .debug_print()
        .status()
        .map_err(ProcessError::IO)
        .from("podman")?
        .success())
}

pub fn pull(uri: &str, user: User) -> Result<(), CommandError> {
    /*!
    Call podman pull and prune with the provided uri
    !*/

    user
        .sudo("podman")
        .arg("pull")
        .arg(uri)
        .debug_print()
        .output()
        .report("podman pull")?;

    Ok(())
}

pub fn update_removed_files(
    target: &String,
    mut accumulated_file: &File,
) -> Result<(), FlakeError> {
    /*!
    Take the contents of the given removed_file and append it
    to the accumulated_file
    !*/
    let host_deps = format!("{}/{}", &target, defaults::HOST_DEPENDENCIES);
    let data = fs::read_to_string(host_deps).map_err(FlakeError::FailedToUpdateRemovedFile)?;
    debug("Adding host deps...");
    debug(&String::from_utf8_lossy(data.as_bytes()));
    accumulated_file
        .write_all(data.as_bytes())
        .map_err(FlakeError::FailedToUpdateRemovedFile)?;
    Ok(())
}
