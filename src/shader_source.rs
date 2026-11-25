macro_rules! shaders {
    ($($file:literal),+ $(,)?) => {
        pub const WGSL: &str = concat!($(include_str!($file), "\n",)+);
        #[allow(dead_code)] // Used by Cargo's build script only.
        pub const FILES: &[&str] = &[$($file),+];
    };
}

shaders!(
    "demo.wgsl",
    "titlebar/atlas.wgsl",
    "titlebar/render.wgsl",
    "crt.wgsl",
    "titlebar/monitor.wgsl",
    "dock/atlas.wgsl",
    "dock/render.wgsl",
    "wallpapers.wgsl",
    "wallpapers_catalog.wgsl",
    "home/toolbar_labels.wgsl",
    "home.wgsl",
);
