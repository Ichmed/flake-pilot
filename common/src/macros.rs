
#[macro_export]
macro_rules! paths {
    ($($name:ident = $path:literal;)*) => {
        lazy_static::lazy_static!($(pub static ref $name: &'static Path = Path::new($path);)*);
    };
}