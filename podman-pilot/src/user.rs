use crate::sudo::Sudo;

#[derive(Default, serde::Deserialize, Clone, Copy)]
pub struct User<'a>(Option<&'a str>);

impl<'a> User<'a> {

    pub fn new(name: &'a str) -> Self {
        Self(Some(name))
    }

    pub fn none() -> Self {
        Self(None)
    }

    pub fn root() -> Self {
        Self(Some("root"))
    }

    pub fn sudo(&self, cmd: &str) -> Sudo {
        let mut sudo = Sudo::new();
        sudo.user(self.0).arg(cmd);
        sudo
    }
}