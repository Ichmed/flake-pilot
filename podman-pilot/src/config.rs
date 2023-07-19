use serde::Deserialize;
use std::{env, path::PathBuf, sync::OnceLock, fs};

use crate::{defaults, user::User};

static CONFIG: OnceLock<Config> = OnceLock::new();

/// Returns the config singleton
/// 
/// Will initialize the config on first call and return the cached version afterwards
pub fn config() -> &'static Config<'static> {
    CONFIG.get_or_init(load_config)
}

fn get_base_path() -> PathBuf {
    which::which(env::args().next().expect("Arg 0 must be present")).expect("Symlink should exist")
}

fn load_config() -> Config<'static> {
    let base_path = get_base_path();
    let base_path  = base_path.file_name().unwrap().to_str().unwrap();
    let content = fs::read_to_string(format!("{}/{}.yaml", defaults::CONTAINER_FLAKE_DIR, base_path));
    
    // Leak the data to make it static
    // Safety: This does not cause a reocurring memory leak since `load_config` is only called once
    let content = Box::leak(content.unwrap().into_boxed_str());
    serde_yaml::from_str(content).unwrap()
}

#[derive(Deserialize)]
pub struct Config<'a> {
    #[serde(borrow)]
    pub container: ContainerSection<'a>,
    pub tar: Vec<&'a str>
}

impl<'a> Config<'a> {
    pub fn is_delta_container(&self) -> bool {
        self.container.base_container.is_some()
    }

    pub fn resume(&self) -> bool {
        match self.container.runtime.as_ref() {
            Some(runtime) => runtime.resume,
            None => false,
        }
    }
}

#[derive(Deserialize)]
pub struct ContainerSection<'a> {
    /// Mandatory registration setup
    /// Name of the container in the local registry
    pub name: &'a str,

    /// Path of the program to call inside of the container (target)
    pub target_app_path: Option<&'a str>,

    /// Path of the program to register on the host
    pub host_app_path: &'a str,

    /// Optional base container to use with a delta 'container: name'
    ///
    /// If specified the given 'container: name' is expected to be
    /// an overlay for the specified base_container. podman-pilot
    /// combines the 'container: name' with the base_container into
    /// one overlay and starts the result as a container instance
    ///
    /// Default: not_specified
    pub base_container: Option<&'a str>,

    /// Optional additional container layers on top of the
    /// specified base container
    #[serde(default)]
    pub layers: Vec<&'a str>,

    /// Optional registration setup
    /// Container runtime parameters
    #[serde(default)]
    pub runtime: Option<RuntimeSection<'a>>,
}

#[derive(Deserialize, Default, Clone)]
pub struct RuntimeSection<'a> {
    /// Run the container engine as a user other than the
    /// default target user root. The user may be either
    /// a user name or a numeric user-ID (UID) prefixed
    /// with the ‘#’ character (e.g. #0 for UID 0). The call
    /// of the container engine is performed by sudo.
    /// The behavior of sudo can be controlled via the
    /// file /etc/sudoers
    #[serde(borrow)]
    pub runas: User<'a>,

    /// Resume the container from previous execution.
    ///
    /// If the container is still running, the app will be
    /// executed inside of this container instance.
    ///
    /// Default: false
    #[serde(default)]
    pub resume: bool,

    /// Attach to the container if still running, rather than
    /// executing the app again. Only makes sense for interactive
    /// sessions like a shell running as app in the container.
    ///
    /// Default: false
    #[serde(default)]
    pub attach: bool,

    /// Caller arguments for the podman engine in the format:
    /// - PODMAN_OPTION_NAME_AND_OPTIONAL_VALUE
    ///
    /// For details on podman options please consult the
    /// podman documentation.
    #[serde(default)]
    pub podman: Vec<&'a str>,
}
